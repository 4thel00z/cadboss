//! Grouping of the searchable text of a drawing.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use cadboss_core::{BBox, Point, Report, SkipKind, SkipReason, Transform};
use cadboss_model::{
    Entity, EntityKind, Layout, LayoutKind, Text, TextGroups, TextItem, TextSource,
};

use crate::document::{DxfDocument, LayoutPass};

const TITLE_MARKERS: [&str; 4] = ["TITLE", "TBLK", "SCHRIFTFELD", "CARTOUCHE"];
const BOTTOM_RIGHT_SHARE: f64 = 0.25;

/// An annotation waiting for its layout's extents; `titled` is set when the entity sits inside a title-block insert.
struct Pending {
    item: TextItem,
    titled: bool,
}

struct Collector {
    groups: TextGroups,
    pending: Vec<Pending>,
    links: Vec<TextItem>,
    images: Vec<TextItem>,
    defs: HashMap<u64, Text>,
    seen: HashSet<u64>,
}

/// The text groups and one pass per layout, in `layouts()` order; `None` marks a layout whose name is not text.
pub(crate) struct Survey {
    pub(crate) groups: TextGroups,
    pub(crate) report: Report,
    pub(crate) passes: Vec<Option<LayoutPass>>,
}

/// Walks every layout once, collecting text and handing each top-level entity to `top`.
/// Annotations are placed in the title block when they sit in a title-block insert or in the bottom-right quarter of their paper layout's extents from the same walk.
pub(crate) fn survey(doc: &DxfDocument, top: &mut dyn FnMut(&Entity)) -> Survey {
    let mut report = Report::default();
    let defs = doc.image_defs(&mut report);
    let mut collector = Collector {
        groups: TextGroups::default(),
        pending: Vec::new(),
        links: Vec::new(),
        images: Vec::new(),
        defs: defs.iter().cloned().collect(),
        seen: HashSet::new(),
    };
    let mut passes = Vec::with_capacity(doc.layouts().len());
    for layout in doc.layouts() {
        let Some(name) = layout.name.as_str() else {
            report.note(SkipKind::Object, SkipReason::Unsupported);
            passes.push(None);
            continue;
        };
        let mut pass = doc.pass(name, top, &mut |entity, transform, path| {
            collector.visit(layout, entity, transform, path);
        });
        collector.settle(paper_extents(layout, pass.extents));
        report.merge(std::mem::take(&mut pass.walked));
        passes.push(Some(pass));
    }
    let mut groups = collector.finish(doc, &defs);
    let tabs = tab_map(doc.layouts());
    groups.title_block = rank(groups.title_block, &tabs);
    groups.annotations = rank(groups.annotations, &tabs);
    groups.attributes = rank(groups.attributes, &tabs);
    groups.references = rank(groups.references, &tabs);
    Survey {
        groups,
        report,
        passes,
    }
}

impl Collector {
    fn visit(&mut self, layout: &Layout, entity: &Entity, transform: &Transform, path: &[Text]) {
        self.hyperlink(layout, entity, transform);
        match &entity.kind {
            EntityKind::Text {
                text,
                position,
                height,
                ..
            }
            | EntityKind::MText {
                text,
                position,
                height,
                ..
            } => self.annotation(
                layout,
                entity,
                path,
                (text.clone(), TextSource::Annotation),
                (
                    Some(transform.apply(*position)),
                    Some(height * transform.scale_factor()),
                ),
            ),
            EntityKind::Dimension {
                text,
                measurement,
                text_position,
                ..
            } => {
                let Some(text) = dimension_text(text, *measurement) else {
                    return;
                };
                self.annotation(
                    layout,
                    entity,
                    path,
                    (text, TextSource::Dimension),
                    (text_position.map(|point| transform.apply(point)), None),
                );
            }
            EntityKind::Leader {
                text: Some(text),
                points,
            } => self.annotation(
                layout,
                entity,
                path,
                (text.clone(), TextSource::Leader),
                (points.first().map(|point| transform.apply(*point)), None),
            ),
            EntityKind::Insert { attributes, .. } => {
                for attribute in attributes {
                    self.groups.attributes.push(TextItem {
                        text: Text::from(format!(
                            "{}={}",
                            display(&attribute.tag),
                            display(&attribute.value)
                        )),
                        source: TextSource::Attribute,
                        layer: entity.common.layer.clone(),
                        layout: layout.name.clone(),
                        position: Some(transform.apply(attribute.position)),
                        height: Some(attribute.height * transform.scale_factor()),
                    });
                }
            }
            EntityKind::Image {
                image_def: Some(handle),
                position,
                ..
            } => {
                let Some(path) = self.defs.get(handle) else {
                    return;
                };
                if !self.seen.insert(*handle) {
                    return;
                }
                self.images.push(TextItem {
                    text: path.clone(),
                    source: TextSource::ImagePath,
                    layer: entity.common.layer.clone(),
                    layout: layout.name.clone(),
                    position: Some(transform.apply(*position)),
                    height: None,
                });
            }
            _ => {}
        }
    }

    fn hyperlink(&mut self, layout: &Layout, entity: &Entity, transform: &Transform) {
        let Some(url) = &entity.common.hyperlink else {
            return;
        };
        self.links.push(TextItem {
            text: url.clone(),
            source: TextSource::Hyperlink,
            layer: entity.common.layer.clone(),
            layout: layout.name.clone(),
            position: anchor(entity).map(|point| transform.apply(point)),
            height: None,
        });
    }

    fn annotation(
        &mut self,
        layout: &Layout,
        entity: &Entity,
        path: &[Text],
        (text, source): (Text, TextSource),
        (position, height): (Option<Point>, Option<f64>),
    ) {
        if text.is_empty() {
            return;
        }
        self.pending.push(Pending {
            item: TextItem {
                text,
                source,
                layer: entity.common.layer.clone(),
                layout: layout.name.clone(),
                position,
                height,
            },
            titled: in_title_block(path),
        });
    }

    fn settle(&mut self, extents: Option<BBox>) {
        for pending in self.pending.drain(..) {
            if pending.titled || bottom_right(extents, pending.item.position) {
                self.groups.title_block.push(pending.item);
                continue;
            }
            self.groups.annotations.push(pending.item);
        }
    }

    fn finish(mut self, doc: &DxfDocument, defs: &[(u64, Text)]) -> TextGroups {
        for layer in doc.layers() {
            self.groups
                .names
                .push(unplaced(layer.name.clone(), TextSource::LayerName));
        }
        for block in doc.blocks() {
            if !block.anonymous {
                self.groups
                    .names
                    .push(unplaced(block.name.clone(), TextSource::BlockName));
            }
            if let Some(path) = &block.xref_path {
                self.groups
                    .references
                    .push(unplaced(path.clone(), TextSource::Xref));
            }
        }
        self.groups.references.append(&mut self.images);
        for (handle, path) in defs {
            if self.seen.contains(handle) {
                continue;
            }
            self.groups
                .references
                .push(unplaced(path.clone(), TextSource::ImagePath));
        }
        self.groups.references.append(&mut self.links);
        self.groups
    }
}

fn paper_extents(layout: &Layout, extents: Option<BBox>) -> Option<BBox> {
    extents.filter(|_| layout.kind == LayoutKind::Paper)
}

fn tab_map(layouts: &[Layout]) -> HashMap<Vec<u8>, usize> {
    let mut tabs = HashMap::new();
    for (index, layout) in layouts.iter().enumerate() {
        tabs.entry(layout.name.as_bytes().to_vec()).or_insert(index);
    }
    tabs
}

fn unplaced(text: Text, source: TextSource) -> TextItem {
    TextItem {
        text,
        source,
        layer: Text::default(),
        layout: Text::default(),
        position: None,
        height: None,
    }
}

fn display(text: &Text) -> String {
    text.as_str()
        .map_or_else(|| text.to_string(), str::to_owned)
}

fn anchor(entity: &Entity) -> Option<Point> {
    match &entity.kind {
        EntityKind::Text { position, .. }
        | EntityKind::MText { position, .. }
        | EntityKind::Insert { position, .. }
        | EntityKind::Image { position, .. } => Some(*position),
        _ => None,
    }
}

fn in_title_block(path: &[Text]) -> bool {
    path.iter().filter_map(Text::as_str).any(|name| {
        let name = name.to_uppercase();
        TITLE_MARKERS.iter().any(|marker| name.contains(marker))
    })
}

fn bottom_right(extents: Option<BBox>, position: Option<Point>) -> bool {
    let (Some(extents), Some(position)) = (extents, position) else {
        return false;
    };
    position.x > extents.min.x + (1.0 - BOTTOM_RIGHT_SHARE) * extents.width()
        && position.y < extents.min.y + BOTTOM_RIGHT_SHARE * extents.height()
}

fn format_measurement(measurement: f64) -> String {
    let fixed = format!("{measurement:.2}");
    let trimmed = fixed.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        return "0".to_owned();
    }
    trimmed.to_owned()
}

fn dimension_text(text: &Text, measurement: Option<f64>) -> Option<Text> {
    let Some(raw) = text.as_str() else {
        return Some(text.clone()).filter(|text| !text.is_empty());
    };
    let measured = measurement
        .filter(|value| value.is_finite())
        .map(format_measurement)
        .unwrap_or_default();
    let shown = match raw {
        "" | "<>" => measured,
        other => other.replace("<>", &measured),
    };
    Some(Text::from(shown)).filter(|text| !display(text).trim().is_empty())
}

fn compare(first: &(usize, TextItem), second: &(usize, TextItem)) -> Ordering {
    match (first.1.position, second.1.position) {
        (Some(a), Some(b)) => first
            .0
            .cmp(&second.0)
            .then_with(|| first.1.layer.as_bytes().cmp(second.1.layer.as_bytes()))
            .then_with(|| b.y.total_cmp(&a.y))
            .then_with(|| a.x.total_cmp(&b.x)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn rank(items: Vec<TextItem>, tabs: &HashMap<Vec<u8>, usize>) -> Vec<TextItem> {
    let mut ranked: Vec<(usize, TextItem)> = items
        .into_iter()
        .map(|item| {
            (
                tabs.get(item.layout.as_bytes())
                    .copied()
                    .unwrap_or(usize::MAX),
                item,
            )
        })
        .collect();
    ranked.sort_by(compare);
    ranked.into_iter().map(|(_, item)| item).collect()
}
