//! Grouping of the searchable text of a drawing.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use cadboss_core::{BBox, Point, Report, Transform};
use cadboss_model::{
    Entity, EntityKind, Layout, LayoutKind, Text, TextGroups, TextItem, TextSource,
};

use crate::document::DxfDocument;

const TITLE_MARKERS: [&str; 4] = ["TITLE", "TBLK", "SCHRIFTFELD", "CARTOUCHE"];
const BOTTOM_RIGHT_SHARE: f64 = 0.25;

struct Sheet<'l> {
    layout: &'l Layout,
    extents: Option<BBox>,
}

struct Collector {
    groups: TextGroups,
    links: Vec<TextItem>,
    images: Vec<TextItem>,
    defs: HashMap<u64, Text>,
    seen: HashSet<u64>,
}

pub(crate) fn collect(doc: &DxfDocument) -> (TextGroups, Report) {
    let mut report = Report::default();
    let defs = doc.image_defs(&mut report);
    let mut collector = Collector {
        groups: TextGroups::default(),
        links: Vec::new(),
        images: Vec::new(),
        defs: defs.iter().cloned().collect(),
        seen: HashSet::new(),
    };
    for layout in doc.layouts() {
        let Some(name) = layout.name.as_str() else {
            continue;
        };
        let extents = Some(layout.kind)
            .filter(|kind| *kind == LayoutKind::Paper)
            .and_then(|_| doc.layout_extents(name));
        let sheet = Sheet { layout, extents };
        report.merge(doc.walk_with_path(name, &mut |entity, transform, path| {
            collector.visit(&sheet, entity, transform, path);
        }));
    }
    let mut groups = collector.finish(doc, &defs);
    let layouts = doc.layouts();
    groups.title_block = rank(groups.title_block, layouts);
    groups.annotations = rank(groups.annotations, layouts);
    groups.names = rank(groups.names, layouts);
    groups.attributes = rank(groups.attributes, layouts);
    groups.references = rank(groups.references, layouts);
    (groups, report)
}

impl Collector {
    fn visit(&mut self, sheet: &Sheet, entity: &Entity, transform: &Transform, path: &[Text]) {
        self.hyperlink(sheet, entity, transform);
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
                sheet,
                entity,
                path,
                (text.clone(), TextSource::Annotation),
                (Some(transform.apply(*position)), Some(*height)),
            ),
            EntityKind::Dimension {
                text, measurement, ..
            } => {
                let Some(text) = dimension_text(text, *measurement) else {
                    return;
                };
                self.annotation(
                    sheet,
                    entity,
                    path,
                    (text, TextSource::Dimension),
                    (None, None),
                );
            }
            EntityKind::Leader {
                text: Some(text),
                points,
            } => self.annotation(
                sheet,
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
                        layout: sheet.layout.name.clone(),
                        position: Some(transform.apply(attribute.position)),
                        height: Some(attribute.height),
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
                    layout: sheet.layout.name.clone(),
                    position: Some(transform.apply(*position)),
                    height: None,
                });
            }
            _ => {}
        }
    }

    fn hyperlink(&mut self, sheet: &Sheet, entity: &Entity, transform: &Transform) {
        let Some(url) = &entity.common.hyperlink else {
            return;
        };
        self.links.push(TextItem {
            text: url.clone(),
            source: TextSource::Hyperlink,
            layer: entity.common.layer.clone(),
            layout: sheet.layout.name.clone(),
            position: anchor(entity).map(|point| transform.apply(point)),
            height: None,
        });
    }

    fn annotation(
        &mut self,
        sheet: &Sheet,
        entity: &Entity,
        path: &[Text],
        (text, source): (Text, TextSource),
        (position, height): (Option<Point>, Option<f64>),
    ) {
        if text.is_empty() {
            return;
        }
        let item = TextItem {
            text,
            source,
            layer: entity.common.layer.clone(),
            layout: sheet.layout.name.clone(),
            position,
            height,
        };
        if in_title_block(path) || bottom_right(sheet.extents, position) {
            self.groups.title_block.push(item);
            return;
        }
        self.groups.annotations.push(item);
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
    fixed.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn dimension_text(text: &Text, measurement: Option<f64>) -> Option<Text> {
    let Some(raw) = text.as_str() else {
        return Some(text.clone()).filter(|text| !text.is_empty());
    };
    let measured = measurement.map(format_measurement).unwrap_or_default();
    let shown = match raw {
        "" | "<>" => measured,
        other => other.replace("<>", &measured),
    };
    Some(Text::from(shown)).filter(|text| !display(text).trim().is_empty())
}

fn bytes(text: &Text) -> &[u8] {
    match text {
        Text::Str(text) => text.as_bytes(),
        Text::Bytes(bytes) => bytes,
    }
}

fn tab(layouts: &[Layout], layout: &Text) -> usize {
    layouts
        .iter()
        .position(|candidate| candidate.name == *layout)
        .unwrap_or(usize::MAX)
}

fn compare(first: &(usize, TextItem), second: &(usize, TextItem)) -> Ordering {
    match (first.1.position, second.1.position) {
        (Some(a), Some(b)) => first
            .0
            .cmp(&second.0)
            .then_with(|| bytes(&first.1.layer).cmp(bytes(&second.1.layer)))
            .then_with(|| b.y.total_cmp(&a.y))
            .then_with(|| a.x.total_cmp(&b.x)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn rank(items: Vec<TextItem>, layouts: &[Layout]) -> Vec<TextItem> {
    let mut ranked: Vec<(usize, TextItem)> = items
        .into_iter()
        .map(|item| (tab(layouts, &item.layout), item))
        .collect();
    ranked.sort_by(compare);
    ranked.into_iter().map(|(_, item)| item).collect()
}
