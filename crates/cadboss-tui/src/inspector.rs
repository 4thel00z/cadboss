//! The inspector's content: a title and the facts of the selected row, by row kind.

use cadboss_core::{BBox, Point};
use cadboss_model::{format_number, Entity, EntityKind, TextItem, TextSource};
use cadboss_output::one_line;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::app::App;
use crate::outline::{hex, RowKind};
use crate::theme::Theme;
use crate::ui::{entity_total, printable};

/// A section heading: the bold muted title, then a rule to the edge.
fn section(theme: &Theme, title: &str, width: usize) -> Line<'static> {
    let rule = "─".repeat(width.saturating_sub(title.chars().count() + 1));
    Line::from(vec![
        Span::styled(
            format!("{title} "),
            theme.muted().add_modifier(Modifier::BOLD),
        ),
        Span::styled(rule, theme.fg(theme.border)),
    ])
}

/// A blank line, the heading and the body; nothing when the body is empty.
fn titled(
    theme: &Theme,
    title: &str,
    width: usize,
    body: Vec<Line<'static>>,
) -> Vec<Line<'static>> {
    if body.is_empty() {
        return Vec::new();
    }
    [Line::raw(""), section(theme, title, width)]
        .into_iter()
        .chain(body)
        .collect()
}

/// One fact: the key muted in an 11-column cell, then the value.
fn fact(theme: &Theme, key: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<11}"), theme.muted()),
        value,
    ])
}

/// Facts with plain values, leaving out the ones whose value is empty.
fn facts(theme: &Theme, entries: Vec<(&str, String)>) -> Vec<Line<'static>> {
    entries
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| fact(theme, key, Span::raw(printable(&value))))
        .collect()
}

/// A layer flag: `yes` in `warning`, or `no`.
fn flag(theme: &Theme, key: &str, set: bool) -> Line<'static> {
    if !set {
        return fact(theme, key, Span::raw("no"));
    }
    fact(theme, key, Span::styled("yes", theme.fg(theme.warning)))
}

fn yes_no(set: bool) -> String {
    if set {
        return "yes".to_owned();
    }
    "no".to_owned()
}

fn point(at: &Point) -> String {
    format!("{}, {}", format_number(at.x), format_number(at.y))
}

fn extents(bbox: &BBox) -> String {
    format!(
        "({}, {}) to ({}, {})",
        format_number(bbox.min.x),
        format_number(bbox.min.y),
        format_number(bbox.max.x),
        format_number(bbox.max.y)
    )
}

fn length(len: usize, noun: &str) -> String {
    format!("{len} {noun}")
}

fn text_value(text: &cadboss_model::Text) -> String {
    one_line(&text.to_string())
}

fn optional_text(text: Option<&cadboss_model::Text>) -> String {
    text.map(text_value).unwrap_or_default()
}

fn optional_number(value: Option<f64>) -> String {
    value.map(format_number).unwrap_or_default()
}

fn source_label(source: TextSource) -> &'static str {
    match source {
        TextSource::TitleBlock => "Title block",
        TextSource::Annotation => "Annotation",
        TextSource::Dimension => "Dimension",
        TextSource::Leader => "Leader",
        TextSource::BlockName => "Block name",
        TextSource::LayerName => "Layer name",
        TextSource::Attribute => "Attribute",
        TextSource::Xref => "Xref",
        TextSource::ImagePath => "Image path",
        TextSource::Hyperlink => "Hyperlink",
        TextSource::Property => "Property",
    }
}

/// Metadata facts, then once loaded the counts, the entity types and the notes the report holds.
fn file_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let document = &app.outline.document;
    let metadata = &document.metadata;
    let mut lines = facts(
        theme,
        vec![
            (
                "Format",
                format!("{} {}", metadata.format.label(), metadata.version),
            ),
            ("Units", metadata.units.label()),
            ("Codepage", optional_text(metadata.codepage.as_ref())),
            ("Author", optional_text(metadata.author.as_ref())),
            ("Created", optional_number(metadata.created)),
            ("Updated", optional_number(metadata.updated)),
            (
                "Extents",
                metadata.extents.as_ref().map(extents).unwrap_or_default(),
            ),
        ],
    );
    if app.loading {
        return lines;
    }
    lines.extend(facts(
        theme,
        vec![
            ("Layers", metadata.layer_count.to_string()),
            ("Blocks", metadata.block_count.to_string()),
            ("Layouts", metadata.layout_count.to_string()),
            ("Entities", entity_total(metadata).to_string()),
        ],
    ));
    let types = metadata
        .entity_counts
        .iter()
        .map(|(name, count)| {
            Line::from(vec![
                Span::styled(format!("{:<12}", printable(name)), theme.fg(theme.info)),
                Span::raw(count.to_string()),
            ])
        })
        .collect();
    lines.extend(titled(theme, "Entity types", width, types));
    let notes: Vec<Line<'static>> = document
        .report
        .lines()
        .into_iter()
        .map(|note| Line::styled(note, theme.muted()))
        .collect();
    let heading = format!("Notes while reading ({})", notes.len());
    lines.extend(titled(theme, &heading, width, notes));
    lines
}

fn layout_lines(theme: &Theme, layout: &cadboss_model::Layout) -> Vec<Line<'static>> {
    facts(
        theme,
        vec![
            ("Kind", layout.kind.label().to_owned()),
            ("Block", text_value(&layout.block_name)),
            (
                "Extents",
                layout.extents.as_ref().map(extents).unwrap_or_default(),
            ),
            ("Entities", layout.entity_count.to_string()),
        ],
    )
}

fn block_lines(theme: &Theme, block: &cadboss_model::Block) -> Vec<Line<'static>> {
    facts(
        theme,
        vec![
            ("Base", point(&block.base)),
            ("Xref", optional_text(block.xref_path.as_ref())),
            ("Anonymous", yes_no(block.anonymous)),
            ("Entities", block.entity_count.to_string()),
        ],
    )
}

fn blocks_folder_lines(theme: &Theme, blocks: &[cadboss_model::Block]) -> Vec<Line<'static>> {
    let anonymous = blocks.iter().filter(|block| block.anonymous).count();
    let xrefs = blocks
        .iter()
        .filter(|block| block.xref_path.is_some())
        .count();
    facts(
        theme,
        vec![
            ("Blocks", blocks.len().to_string()),
            ("Anonymous", anonymous.to_string()),
            ("Xrefs", xrefs.to_string()),
        ],
    )
}

fn layer_lines(app: &App, layer: &cadboss_model::Layer) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let count = app
        .outline
        .layer_counts
        .get(layer.name.as_bytes())
        .copied()
        .unwrap_or(0);
    vec![
        fact(theme, "Color", Span::raw(layer.color.to_string())),
        flag(theme, "Frozen", layer.frozen),
        flag(theme, "Off", layer.off),
        flag(theme, "Locked", layer.locked),
        fact(theme, "Entities", Span::raw(count.to_string())),
    ]
}

/// The kind's own fields: points as `x, y`, numbers through `format_number`, lists as their length.
fn kind_facts(kind: &EntityKind) -> Vec<(&'static str, String)> {
    match kind {
        EntityKind::Line { start, end } => vec![("Start", point(start)), ("End", point(end))],
        EntityKind::Circle { center, radius } => vec![
            ("Center", point(center)),
            ("Radius", format_number(*radius)),
        ],
        EntityKind::Arc {
            center,
            radius,
            start,
            end,
        } => vec![
            ("Center", point(center)),
            ("Radius", format_number(*radius)),
            ("Start", format_number(*start)),
            ("End", format_number(*end)),
        ],
        EntityKind::Ellipse {
            center,
            major,
            ratio,
            start,
            end,
        } => vec![
            ("Center", point(center)),
            ("Major", point(major)),
            ("Ratio", format_number(*ratio)),
            ("Start", format_number(*start)),
            ("End", format_number(*end)),
        ],
        EntityKind::Polyline { points, closed, .. } => vec![
            ("Points", length(points.len(), "points")),
            ("Closed", yes_no(*closed)),
        ],
        EntityKind::Spline {
            control,
            fit: fitted,
            degree,
            closed,
        } => vec![
            ("Control", length(control.len(), "points")),
            ("Fit", length(fitted.len(), "points")),
            ("Degree", degree.to_string()),
            ("Closed", yes_no(*closed)),
        ],
        EntityKind::Hatch {
            pattern,
            solid,
            loops,
        } => vec![
            ("Pattern", text_value(pattern)),
            ("Solid", yes_no(*solid)),
            ("Loops", loops.to_string()),
        ],
        EntityKind::Text {
            text,
            position,
            height,
            rotation,
            style,
        } => vec![
            ("Text", text_value(text)),
            ("Position", point(position)),
            ("Height", format_number(*height)),
            ("Rotation", format_number(*rotation)),
            ("Style", text_value(style)),
        ],
        EntityKind::MText {
            text,
            position,
            height,
            width,
            rotation,
            style,
        } => vec![
            ("Text", text_value(text)),
            ("Position", point(position)),
            ("Height", format_number(*height)),
            ("Width", format_number(*width)),
            ("Rotation", format_number(*rotation)),
            ("Style", text_value(style)),
        ],
        EntityKind::Dimension {
            text,
            measurement,
            text_position,
            dim_type,
            block_name,
            style,
        } => vec![
            ("Text", text_value(text)),
            ("Measured", optional_number(*measurement)),
            (
                "Text at",
                text_position.as_ref().map(point).unwrap_or_default(),
            ),
            ("Dim type", dim_type.to_string()),
            ("Block", text_value(block_name)),
            ("Style", text_value(style)),
        ],
        EntityKind::Leader { text, points } => vec![
            ("Text", optional_text(text.as_ref())),
            ("Points", length(points.len(), "points")),
        ],
        EntityKind::Insert {
            block_name,
            position,
            scale,
            rotation,
            ..
        } => vec![
            ("Block", text_value(block_name)),
            ("Position", point(position)),
            ("Scale", point(scale)),
            ("Rotation", format_number(*rotation)),
        ],
        EntityKind::Point { position } => vec![("Position", point(position))],
        EntityKind::Solid { points } => vec![("Points", length(points.len(), "points"))],
        EntityKind::Image {
            path,
            image_def,
            position,
            size,
        } => vec![
            ("Path", optional_text(path.as_ref())),
            ("Image def", hex(*image_def)),
            ("Position", point(position)),
            ("Size", point(size)),
        ],
        EntityKind::Viewport {
            center,
            width,
            height,
            view_center,
            view_height,
            frozen_layers,
        } => vec![
            ("Center", point(center)),
            ("Width", format_number(*width)),
            ("Height", format_number(*height)),
            ("View at", point(view_center)),
            ("View size", format_number(*view_height)),
            ("Frozen", length(frozen_layers.len(), "layers")),
        ],
        EntityKind::Unknown { .. } => Vec::new(),
    }
}

/// The common fields, the kind's own fields and, for an INSERT, its attributes as `TAG  value` lines.
fn entity_lines(theme: &Theme, entity: &Entity, width: usize) -> Vec<Line<'static>> {
    let common = &entity.common;
    let mut entries = vec![
        ("Type", entity.display_type().to_owned()),
        ("Layer", text_value(&common.layer)),
        ("Layout", text_value(&common.layout)),
        ("Handle", hex(common.handle)),
        ("Color", common.color.to_string()),
        ("Hyperlink", optional_text(common.hyperlink.as_ref())),
    ];
    entries.extend(kind_facts(&entity.kind));
    let mut lines = facts(theme, entries);
    let EntityKind::Insert { attributes, .. } = &entity.kind else {
        return lines;
    };
    let body = attributes
        .iter()
        .map(|attribute| {
            Line::from(vec![
                Span::styled(
                    format!("{}  ", printable(&text_value(&attribute.tag))),
                    theme.muted(),
                ),
                Span::raw(printable(&text_value(&attribute.value))),
            ])
        })
        .collect();
    lines.extend(titled(theme, "Attributes", width, body));
    lines
}

/// The text line by line in bold, then where it came from.
fn text_item_lines(theme: &Theme, item: &TextItem) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = item
        .text
        .to_string()
        .lines()
        .map(|line| Line::styled(printable(line), theme.bold()))
        .collect();
    lines.push(Line::raw(""));
    lines.extend(facts(
        theme,
        vec![
            ("Source", source_label(item.source).to_owned()),
            ("Layer", text_value(&item.layer)),
            ("Layout", text_value(&item.layout)),
            (
                "Position",
                item.position.as_ref().map(point).unwrap_or_default(),
            ),
            ("Height", optional_number(item.height)),
            ("Handle", hex(item.handle)),
        ],
    ));
    lines
}

/// Everything the inspector shows for the selected row, with section rules `width` columns wide.
pub(crate) fn inspector_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let outline = &app.outline;
    let document = &outline.document;
    let Some(row) = outline.rows.get(app.selected) else {
        return Vec::new();
    };
    match &row.kind {
        RowKind::File => file_lines(app, width),
        RowKind::LayoutsFolder => {
            facts(theme, vec![("Layouts", document.layouts.len().to_string())])
        }
        RowKind::Layout(position) => document
            .layouts
            .get(*position)
            .map(|layout| layout_lines(theme, layout))
            .unwrap_or_default(),
        RowKind::BlocksFolder => blocks_folder_lines(theme, &document.blocks),
        RowKind::Block(index) => document
            .blocks
            .get(*index)
            .map(|block| block_lines(theme, block))
            .unwrap_or_default(),
        RowKind::LayersFolder => facts(theme, vec![("Layers", document.layers.len().to_string())]),
        RowKind::Layer(index) => document
            .layers
            .get(*index)
            .map(|layer| layer_lines(app, layer))
            .unwrap_or_default(),
        RowKind::TextFolder => facts(theme, vec![("Items", document.text.len().to_string())]),
        RowKind::TextGroup(group) => facts(
            theme,
            vec![
                ("Group", group.label().to_owned()),
                ("Items", group.items(&document.text).len().to_string()),
            ],
        ),
        RowKind::TextItem(group, index) => group
            .items(&document.text)
            .get(*index)
            .map(|item| text_item_lines(theme, item))
            .unwrap_or_default(),
        RowKind::TypeGroup {
            type_name,
            total,
            shown,
            ..
        } => facts(
            theme,
            vec![
                ("Type", type_name.clone()),
                ("Count", total.to_string()),
                ("Shown", shown.to_string()),
            ],
        ),
        RowKind::Entity { slot } => outline
            .entities
            .get(*slot)
            .map(|located| entity_lines(theme, &located.entity, width))
            .unwrap_or_default(),
        RowKind::More { .. } => vec![Line::styled(printable(&row.label), theme.muted())],
    }
}

fn entity_title(entity: &Entity) -> String {
    let Some(handle) = entity.common.handle else {
        return entity.display_type().to_owned();
    };
    format!("{} {handle:X}", entity.display_type())
}

/// The inspector's title: the selected row's kind, with the name for layouts, blocks and layers.
pub(crate) fn inspector_title(app: &App) -> String {
    let outline = &app.outline;
    let Some(row) = outline.rows.get(app.selected) else {
        return String::new();
    };
    match &row.kind {
        RowKind::File => "Drawing".to_owned(),
        RowKind::LayoutsFolder => "Layouts".to_owned(),
        RowKind::Layout(_) => format!("Layout {}", row.label),
        RowKind::BlocksFolder => "Blocks".to_owned(),
        RowKind::Block(_) => format!("Block {}", row.label),
        RowKind::LayersFolder => "Layers".to_owned(),
        RowKind::Layer(_) => format!("Layer {}", row.label),
        RowKind::TextFolder => "Text".to_owned(),
        RowKind::TextGroup(group) => group.label().to_owned(),
        RowKind::TextItem(group, index) => group
            .items(&outline.document.text)
            .get(*index)
            .map(|item| source_label(item.source).to_owned())
            .unwrap_or_default(),
        RowKind::TypeGroup { type_name, .. } => type_name.clone(),
        RowKind::Entity { slot } => outline
            .entities
            .get(*slot)
            .map(|located| entity_title(&located.entity))
            .unwrap_or_default(),
        RowKind::More { .. } => "More".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use cadboss_core::{SkipKind, SkipReason};
    use cadboss_dxf::DxfDocument;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::app::Msg;
    use crate::outline::{Outline, ROOT};

    fn demo() -> App {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/demo.dxf");
        let doc = Arc::new(DxfDocument::open(Path::new(path)).unwrap());
        let mut app = App::new(
            Arc::clone(&doc),
            "demo.dxf".to_owned(),
            "assets/demo.dxf".to_owned(),
            (80, 24),
        );
        app.update(Msg::Loaded(Box::new(Outline::build(&doc, "demo.dxf"))));
        app
    }

    fn row(app: &App, path: &[&str]) -> usize {
        path.iter().fold(ROOT, |row, label| {
            *app.outline.rows[row]
                .children
                .iter()
                .find(|child| app.outline.rows[**child].label == *label)
                .unwrap()
        })
    }

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(Line::to_string).collect()
    }

    #[test]
    fn inspector_titles_name_the_row_kind() {
        let mut app = demo();
        let mut title = |path: &[&str]| {
            let target = row(&app, path);
            app.select(target);
            inspector_title(&app)
        };
        assert_eq!(title(&[]), "Drawing");
        assert_eq!(title(&["Layouts"]), "Layouts");
        assert_eq!(title(&["Layouts", "Model"]), "Layout Model");
        assert_eq!(title(&["Blocks"]), "Blocks");
        assert_eq!(title(&["Blocks", "PUMP"]), "Block PUMP");
        assert_eq!(title(&["Layers"]), "Layers");
        assert_eq!(title(&["Layers", "WALLS"]), "Layer WALLS");
        assert_eq!(title(&["Text"]), "Text");
        assert_eq!(title(&["Text", "attributes"]), "attributes");
        assert_eq!(title(&["Text", "attributes", "TAG=P-101"]), "Attribute");
        assert_eq!(title(&["Text", "title block", "DRAWN"]), "Annotation");
        assert_eq!(title(&["Text", "annotations", "6000"]), "Dimension");
        assert_eq!(
            title(&["Text", "references", "../xrefs/site-plan.dxf"]),
            "Xref"
        );
        assert_eq!(title(&["Text", "names", "WALLS"]), "Layer name");
        assert_eq!(title(&["Layouts", "Model", "LINE"]), "LINE");

        app.update(Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        let first = app.outline.rows[app.selected].children[0];
        app.select(first);
        assert_eq!(inspector_title(&app), "LINE 135");
    }

    #[test]
    fn entity_inspector_lists_kind_fields_and_attributes() {
        let mut app = demo();
        let insert = row(&app, &["Layouts", "Model", "INSERT"]);
        app.select(insert);
        app.update(Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        app.select(app.outline.rows[insert].children[0]);
        let lines = texts(&inspector_lines(&app, 30));
        assert_eq!(
            lines[..6],
            [
                "Type       INSERT",
                "Layer      EQUIPMENT",
                "Layout     Model",
                "Handle     136",
                "Color      256",
                "Block      PUMP",
            ]
        );
        assert!(lines.iter().any(|line| line.starts_with("Position   ")));
        assert!(lines.contains(&"Scale      1, 1".to_owned()));
        assert!(lines.contains(&"Rotation   0".to_owned()));
        assert!(lines.contains(&"Attributes ───────────────────".to_owned()));
        assert!(lines.contains(&"TAG  P-101".to_owned()));
        assert!(!lines.iter().any(|line| line.starts_with("Hyperlink")));

        app.select(row(&app, &["Layouts", "Model", "INSERT"]));
        assert_eq!(
            texts(&inspector_lines(&app, 30)),
            ["Type       INSERT", "Count      3", "Shown      3"]
        );
        app.select(row(&app, &["Layers", "WALLS"]));
        assert_eq!(
            texts(&inspector_lines(&app, 30)),
            [
                "Color      7",
                "Frozen     no",
                "Off        no",
                "Locked     no",
                "Entities   2"
            ]
        );
        app.select(row(&app, &["Blocks"]));
        assert_eq!(
            texts(&inspector_lines(&app, 30)),
            ["Blocks     6", "Anonymous  3", "Xrefs      1"]
        );
        app.select(row(&app, &["Blocks", "SITE-PLAN"]));
        assert!(texts(&inspector_lines(&app, 30))
            .contains(&"Xref       ../xrefs/site-plan.dxf".to_owned()));
    }

    #[test]
    fn file_inspector_lists_entity_types_then_notes() {
        let mut app = demo();
        let lines = texts(&inspector_lines(&app, 24));
        assert_eq!(lines[11], "");
        assert_eq!(lines[12], "Entity types ───────────");
        assert_eq!(lines[13], "INSERT      4");
        assert_eq!(lines.len(), 21);
        assert!(!lines.iter().any(|line| line.starts_with("Notes")));

        app.outline
            .document
            .report
            .note(SkipKind::Entity, SkipReason::Malformed);
        let lines = texts(&inspector_lines(&app, 24));
        assert_eq!(lines[22], "Notes while reading (1) ");
        assert_eq!(lines[23], "1 entity skipped (malformed)");

        app.loading = true;
        let lines = texts(&inspector_lines(&app, 24));
        assert_eq!(lines.last().unwrap(), "Extents    (0, 0) to (12500, 8000)");
    }
}
