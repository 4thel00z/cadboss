//! Rendering: the layout, one painter per pane, the header and status bar, and the copy and key popups.

use std::ops::RangeInclusive;

use cadboss_core::{BBox, Point};
use cadboss_dxf::{value_kind, ValueKind};
use cadboss_model::{format_number, Entity, EntityKind, Metadata, TextItem, TextSource};
use cadboss_output::one_line;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use ratatui::Frame;

use crate::app::{App, Pane, Toast};
use crate::outline::{hex, RowKind};
use crate::source::View;
use crate::theme::Theme;
use crate::yank::{yank_text, YankTarget, TARGETS};

/// The screen areas: a header line, the outline, the inspector above the source, a status line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Panes {
    pub header: Rect,
    pub tree: Rect,
    pub inspector: Rect,
    pub source: Rect,
    pub status: Rect,
}

/// The two pane dividers as percentages: the tree column's width and the inspector's share of the right column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Splits {
    pub tree: u16,
    pub right_top: u16,
}

impl Default for Splits {
    fn default() -> Splits {
        Splits {
            tree: 42,
            right_top: 62,
        }
    }
}

pub const SPLIT_STEP: u16 = 5;
pub const TREE_SPLIT: RangeInclusive<u16> = 15..=70;
pub const RIGHT_TOP_SPLIT: RangeInclusive<u16> = 20..=85;
/// Spinner frames shown while the outline is built, one per tick.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
/// The narrowest label a tree row keeps before it drops its detail column.
const LABEL_ROOM: usize = 18;
/// Blank columns kept between the header's left and right parts.
const HEADER_GAP: usize = 2;
const COPY_POPUP: (u16, u16) = (76, 11);
const KEYS_POPUP: (u16, u16) = (64, 15);
const HINTS: [(&str, &str); 6] = [
    ("/", "search"),
    ("]", "next layout"),
    ("x", "hex"),
    ("y", "copy"),
    ("?", "keys"),
    ("q", "quit"),
];
const KEYS: [(&str, &str); 12] = [
    ("↑ ↓  j k", "move, or scroll the focused pane"),
    ("← →  h l", "fold, or go to the parent or first child"),
    ("enter", "fold, unfold, or load more entities"),
    ("g G", "first row, last row"),
    ("tab", "focus outline, inspector, source in turn"),
    ("/", "search names, text, layers and handles"),
    ("n N", "next, previous match"),
    ("] [", "next, previous layout"),
    ("x", "source as DXF pairs or hex"),
    ("y", "copy text, layer, handle, DXF, hex, a query"),
    ("alt arrows", "resize the panes"),
    ("q esc", "quit"),
];

pub fn panes(area: Rect, splits: Splits) -> Panes {
    let [header, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let [tree, right] = Layout::horizontal([
        Constraint::Percentage(splits.tree),
        Constraint::Percentage(100u16.saturating_sub(splits.tree)),
    ])
    .areas(body);
    let [inspector, source] = Layout::vertical([
        Constraint::Percentage(splits.right_top),
        Constraint::Percentage(100u16.saturating_sub(splits.right_top)),
    ])
    .areas(right);
    Panes {
        header,
        tree,
        inspector,
        source,
        status,
    }
}

/// Paints the whole screen. The inspector and source scroll offsets are clamped to their content and written back.
pub fn draw(app: &mut App, frame: &mut Frame) {
    let split = panes(frame.area(), app.splits);
    draw_inspector(app, frame, split.inspector);
    draw_source(app, frame, split.source);
    let app = &*app;
    draw_header(app, frame, split.header);
    draw_tree(app, frame, split.tree);
    draw_status(app, frame, split.status);
    if app.yanking {
        draw_yank(app, frame);
    }
    if app.help {
        draw_help(app, frame);
    }
}

/// `text` cut to `width` characters, ending in `…` when cut.
fn fit(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// `text` with control characters shown as `.`, so bytes from the file never reach the terminal as escapes.
fn printable(text: &str) -> String {
    text.chars().map(visible).collect()
}

fn visible(c: char) -> char {
    if c.is_control() {
        return '.';
    }
    c
}

fn pane(theme: &Theme, title: &str, focused: bool) -> Block<'static> {
    let (border, heading) = pane_styles(theme, focused);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Span::styled(format!(" {} ", printable(title)), heading))
}

fn pane_styles(theme: &Theme, focused: bool) -> (Style, Style) {
    if focused {
        return (theme.fg(theme.accent), theme.bold().fg(theme.accent));
    }
    (theme.fg(theme.border), theme.bold().fg(Color::Reset))
}

fn entity_total(metadata: &Metadata) -> u64 {
    metadata
        .entity_counts
        .iter()
        .fold(0, |total, (_, count)| total.saturating_add(*count))
}

fn spinner(ticks: u64) -> char {
    let index = usize::try_from(ticks % SPINNER.len() as u64).unwrap_or(0);
    SPINNER.get(index).copied().unwrap_or(' ')
}

/// The chip, the file name, the format, the units and, once loaded, the counts.
/// While the line is wider than `room` the units, then the counts, are left out.
fn header_left(app: &App, room: usize) -> Line<'static> {
    let theme = &app.theme;
    let metadata = &app.outline.document.metadata;
    let head = [
        Span::styled(" cadboss ", theme.chip(theme.accent)),
        Span::raw("  "),
        Span::styled(printable(&app.title), theme.bold()),
        Span::styled(
            printable(&format!(
                "   {} {}",
                metadata.format.label(),
                metadata.version
            )),
            theme.muted(),
        ),
    ];
    let units = Span::styled(format!("   {}", metadata.units.label()), theme.muted());
    let counts = (!app.loading).then(|| {
        Span::styled(
            format!(
                "   {} entities in {} layouts",
                entity_total(metadata),
                metadata.layout_count
            ),
            theme.muted(),
        )
    });
    let optional: Vec<Span<'static>> = std::iter::once(units).chain(counts).collect();
    (0..=optional.len())
        .map(|dropped| {
            let kept = optional.iter().skip(dropped).cloned();
            Line::from(head.iter().cloned().chain(kept).collect::<Vec<_>>())
        })
        .find(|line| line.width() <= room)
        .unwrap_or_else(|| Line::from(head.to_vec()))
}

/// The spinner while loading, then the text item count and the skipped count when the report has notes.
fn header_right(app: &App) -> Line<'static> {
    let theme = &app.theme;
    if app.loading {
        return Line::styled(
            format!("{} reading the drawing ", spinner(app.ticks)),
            theme.muted(),
        );
    }
    let document = &app.outline.document;
    let items = Span::styled(
        format!("{} text items ", document.text.len()),
        theme.muted(),
    );
    let report = &document.report;
    if report.is_empty() {
        return Line::from(items);
    }
    let skipped = report.skipped.iter().fold(report.unlisted, |total, entry| {
        total.saturating_add(entry.count)
    });
    Line::from(vec![
        items,
        Span::styled(
            format!("  ! {skipped} skipped "),
            theme.fg(theme.warning).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// One line with `left` at the start and `right` flush with the end.
fn spread(frame: &mut Frame, area: Rect, left: Line<'static>, right: Line<'static>) {
    let width = u16::try_from(right.width()).unwrap_or(u16::MAX);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(width)]).areas(area);
    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right),
        right_area,
    );
}

fn draw_header(app: &App, frame: &mut Frame, area: Rect) {
    let right = header_right(app);
    let room = usize::from(area.width).saturating_sub(right.width() + HEADER_GAP);
    spread(frame, area, header_left(app, room), right);
}

/// `text` as spans with every case-insensitive occurrence of `query` styled `mark`.
fn marked(text: &str, query: &str, base: Style, mark: Style) -> Vec<Span<'static>> {
    let lower = text.to_lowercase();
    let needle = query.trim().to_lowercase();
    if needle.is_empty() || lower.len() != text.len() {
        return vec![Span::styled(text.to_owned(), base)];
    }
    let mut spans = Vec::new();
    let mut at = 0;
    for (start, found) in lower.match_indices(&needle) {
        let end = start + found.len();
        let (Some(before), Some(hit)) = (text.get(at..start), text.get(start..end)) else {
            continue;
        };
        spans.push(Span::styled(before.to_owned(), base));
        spans.push(Span::styled(hit.to_owned(), mark));
        at = end;
    }
    spans.push(Span::styled(
        text.get(at..).unwrap_or_default().to_owned(),
        base,
    ));
    spans
}

/// Indent guides: a rule under every ancestor with siblings still to come, blank under the last ones and under the file row.
fn guides(app: &App, index: usize) -> String {
    app.outline
        .ancestors(index)
        .iter()
        .skip(1)
        .rev()
        .map(|ancestor| guide(app.outline.is_last_child(*ancestor)))
        .collect()
}

fn guide(last: bool) -> &'static str {
    if last {
        return "  ";
    }
    "│ "
}

/// A row that opens: one with children, or a type group with entities, whose rows load when it opens.
fn opens(app: &App, index: usize) -> bool {
    let entities = matches!(
        app.outline.rows.get(index).map(|row| &row.kind),
        Some(RowKind::TypeGroup { total, .. }) if *total > 0
    );
    entities || app.has_children(index)
}

fn fold(app: &App, index: usize) -> &'static str {
    if !opens(app, index) {
        return "  ";
    }
    if app.is_expanded(index) {
        return "▾ ";
    }
    "▸ "
}

fn band(theme: &Theme, selected: bool) -> Style {
    if selected {
        return Style::default().bg(theme.selection);
    }
    Style::default()
}

fn gutter(theme: &Theme, selected: bool, band: Style) -> Span<'static> {
    if selected {
        return Span::styled("▌ ", band.fg(theme.accent));
    }
    Span::styled("  ", band)
}

/// Folders and the file row bold, type groups in `info`, `More` rows muted; the selected row bold.
fn label_style(theme: &Theme, kind: &RowKind, band: Style, selected: bool) -> Style {
    let style = match kind {
        RowKind::File
        | RowKind::LayoutsFolder
        | RowKind::BlocksFolder
        | RowKind::LayersFolder
        | RowKind::TextFolder => band.add_modifier(Modifier::BOLD),
        RowKind::TypeGroup { .. } => band.fg(theme.info),
        RowKind::More { .. } => band.fg(theme.muted),
        RowKind::Layout(_)
        | RowKind::Block(_)
        | RowKind::Layer(_)
        | RowKind::TextGroup(_)
        | RowKind::TextItem(..)
        | RowKind::Entity { .. } => band,
    };
    if !selected {
        return style;
    }
    style.add_modifier(Modifier::BOLD)
}

/// The search query when the row is a hit, so only hits get marks.
fn hit_query(app: &App, index: usize) -> &str {
    if !app.search.hits.contains(&index) {
        return "";
    }
    &app.search.query
}

/// The label with search matches marked; an entity row's type is in `info`, apart from its layer and text.
fn label_spans(
    app: &App,
    index: usize,
    kind: &RowKind,
    label: &str,
    base: Style,
) -> Vec<Span<'static>> {
    let theme = &app.theme;
    let query = hit_query(app, index);
    let mark = base
        .fg(theme.accent)
        .add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
    let split = matches!(kind, RowKind::Entity { .. })
        .then(|| label.find("  ").unwrap_or(label.len()))
        .and_then(|at| label.split_at_checked(at));
    let Some((head, rest)) = split else {
        return marked(label, query, base, mark);
    };
    let mut spans = marked(head, query, base.fg(theme.info), mark);
    spans.extend(marked(rest, query, base, mark));
    spans
}

/// The detail column muted; a layer's frozen, off and locked flags in `warning`.
fn detail_spans(theme: &Theme, kind: &RowKind, detail: &str, band: Style) -> Vec<Span<'static>> {
    let muted = band.fg(theme.muted);
    let flagged = matches!(kind, RowKind::Layer(_))
        .then(|| detail.split_once(" · "))
        .flatten();
    let Some((count, flags)) = flagged else {
        return vec![Span::styled(detail.to_owned(), muted)];
    };
    vec![
        Span::styled(count.to_owned(), muted),
        Span::styled(format!(" · {flags}"), band.fg(theme.warning)),
    ]
}

/// Gutter, guides, fold marker, label and the right-aligned detail one column from the edge.
/// The detail is dropped before the label gets narrower than `LABEL_ROOM`.
fn tree_line(app: &App, index: usize, width: usize) -> Option<Line<'static>> {
    let theme = &app.theme;
    let row = app.outline.rows.get(index)?;
    let selected = index == app.selected;
    let band = band(theme, selected);
    let width = width.saturating_sub(1);
    let guides = guides(app, index);
    let fold = fold(app, index);
    let used = 2 + guides.chars().count() + fold.chars().count();
    let detail = printable(&row.detail);
    let detail = Some(detail)
        .filter(|detail| width.saturating_sub(used + detail.chars().count() + 1) >= LABEL_ROOM)
        .unwrap_or_default();
    let room = width.saturating_sub(used + detail.chars().count() + 1);
    let label = fit(&printable(&row.label), room);
    let pad = width.saturating_sub(used + label.chars().count() + detail.chars().count());
    let base = label_style(theme, &row.kind, band, selected);
    let mut spans = vec![
        gutter(theme, selected, band),
        Span::styled(guides, band.fg(theme.border)),
        Span::styled(fold, band.fg(theme.muted)),
    ];
    spans.extend(label_spans(app, index, &row.kind, &label, base));
    spans.push(Span::styled(" ".repeat(pad), band));
    spans.extend(detail_spans(theme, &row.kind, &detail, band));
    spans.push(Span::styled(" ", band));
    Some(Line::from(spans))
}

fn scrollbar(theme: &Theme, frame: &mut Frame, area: Rect, length: usize, position: usize) {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    let visible = usize::from(inner.height);
    if length <= visible {
        return;
    }
    let mut state = ScrollbarState::new(length.saturating_sub(visible)).position(position);
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .track_style(theme.fg(theme.border))
        .thumb_symbol("┃")
        .thumb_style(theme.fg(theme.muted));
    frame.render_stateful_widget(bar, inner, &mut state);
}

fn draw_tree(app: &App, frame: &mut Frame, area: Rect) {
    let block = pane(&app.theme, "Outline", app.focus == Pane::Tree);
    let height = usize::from(area.height.saturating_sub(2));
    let width = usize::from(area.width.saturating_sub(2));
    let rows = app.visible_rows();
    let position = rows
        .iter()
        .position(|row| *row == app.selected)
        .unwrap_or(0);
    let offset = position.saturating_sub(height.saturating_sub(1));
    let lines: Vec<Line> = rows
        .iter()
        .skip(offset)
        .take(height)
        .filter_map(|index| tree_line(app, *index, width))
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
    scrollbar(&app.theme, frame, area, rows.len(), offset);
}

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
fn inspector_lines(app: &App, width: usize) -> Vec<Line<'static>> {
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
fn inspector_title(app: &App) -> String {
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

fn draw_inspector(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = pane(
        &app.theme,
        &inspector_title(app),
        app.focus == Pane::Inspector,
    )
    .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    let paragraph =
        Paragraph::new(inspector_lines(app, usize::from(inner.width))).wrap(Wrap { trim: false });
    let last = paragraph.line_count(inner.width).saturating_sub(1);
    app.inspector_scroll = app
        .inspector_scroll
        .min(u16::try_from(last).unwrap_or(u16::MAX));
    frame.render_widget(
        paragraph.scroll((app.inspector_scroll, 0)).block(block),
        area,
    );
}

/// `{code:>4}  {value}`: the code muted, a numeric value in `number`.
fn pair_line(theme: &Theme, code: i16, value: &str) -> Line<'static> {
    let style = match value_kind(code) {
        ValueKind::Float | ValueKind::Integer => theme.fg(theme.number),
        ValueKind::Text => Style::default(),
    };
    Line::from(vec![
        Span::styled(format!("{code:>4}  "), theme.muted()),
        Span::styled(printable(value), style),
    ])
}

/// A hex dump line with the offset muted and the ASCII column in `number`.
fn hex_line(theme: &Theme, line: &str) -> Line<'static> {
    let parts = line.split_once("  ").and_then(|(offset, rest)| {
        let (bytes, ascii) = rest.split_at_checked(rest.find('|')?)?;
        Some((offset, bytes, ascii))
    });
    let Some((offset, bytes, ascii)) = parts else {
        return Line::raw(line.to_owned());
    };
    Line::from(vec![
        Span::styled(offset.to_owned(), theme.muted()),
        Span::raw(format!("  {bytes}")),
        Span::styled(ascii.to_owned(), theme.fg(theme.number)),
    ])
}

fn view_title(view: View) -> &'static str {
    match view {
        View::Pairs => "DXF",
        View::Hex => "Hex",
    }
}

/// The view's name, and the record's byte range right-aligned when there is a record.
fn source_block(app: &App) -> Block<'static> {
    let theme = &app.theme;
    let block = pane(theme, view_title(app.view), app.focus == Pane::Source)
        .padding(Padding::horizontal(1));
    if app.source.pairs.is_empty() {
        return block;
    }
    let bytes = &app.source.bytes;
    block.title(
        Line::styled(
            format!(" 0x{:x}..0x{:x} ", bytes.start, bytes.end),
            theme.muted(),
        )
        .right_aligned(),
    )
}

/// The lines from the scroll offset on, and past the last one the count of bytes the pane does not lay out.
fn source_lines(app: &App, height: usize) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let source = &app.source;
    if source.pairs.is_empty() {
        return vec![Line::styled("no source record", theme.muted())];
    }
    let scroll = app.source_scroll;
    let mut lines: Vec<Line<'static>> = match app.view {
        View::Pairs => source
            .pairs
            .iter()
            .skip(scroll)
            .take(height)
            .map(|(code, value)| pair_line(theme, *code, value))
            .collect(),
        View::Hex => source
            .hex
            .iter()
            .skip(scroll)
            .take(height)
            .map(|line| hex_line(theme, line))
            .collect(),
    };
    let shown_to_end = scroll.saturating_add(height) >= source.line_count(app.view);
    if source.more == 0 || !shown_to_end {
        return lines;
    }
    lines.push(Line::styled(
        format!("{} more bytes: cadboss obj shows them", source.more),
        theme.muted(),
    ));
    lines
}

fn draw_source(app: &mut App, frame: &mut Frame, area: Rect) {
    let count = app.source.line_count(app.view);
    app.source_scroll = app.source_scroll.min(count.saturating_sub(1));
    let height = usize::from(area.height.saturating_sub(2));
    frame.render_widget(
        Paragraph::new(source_lines(app, height)).block(source_block(app)),
        area,
    );
    scrollbar(&app.theme, frame, area, count, app.source_scroll);
}

fn hint(theme: &Theme, key: &str, label: &str) -> [Span<'static>; 2] {
    [
        Span::styled(key.to_owned(), theme.key()),
        Span::styled(format!(" {label}   "), theme.muted()),
    ]
}

fn mode(app: &App) -> &'static str {
    if app.help {
        return "keys";
    }
    if app.search.editing {
        return "search";
    }
    if app.yanking {
        return "copy";
    }
    "browse"
}

fn matches_label(count: usize) -> String {
    if count == 1 {
        return "1 match".to_owned();
    }
    format!("{count} matches")
}

fn search_status(app: &App) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let theme = &app.theme;
    let left = vec![
        Span::styled("/", theme.key()),
        Span::raw(printable(&app.search.query)),
        Span::styled("█", theme.fg(theme.accent)),
    ];
    let mut right = vec![Span::styled(
        format!("{}   ", matches_label(app.search.hits.len())),
        theme.muted(),
    )];
    right.extend(hint(theme, "enter", "go"));
    right.extend(hint(theme, "esc", "cancel"));
    (left, right)
}

fn toast_span(theme: &Theme, toast: &Toast) -> Span<'static> {
    if toast.problem {
        return Span::styled(
            format!("{} ", toast.text),
            theme.fg(theme.error).add_modifier(Modifier::BOLD),
        );
    }
    Span::styled(
        format!("✓ {} ", toast.text),
        theme.fg(theme.ok).add_modifier(Modifier::BOLD),
    )
}

/// The toast while one is up, else the position among the search hits.
fn browse_right(app: &App) -> Vec<Span<'static>> {
    let theme = &app.theme;
    if let Some(toast) = &app.toast {
        return vec![toast_span(theme, toast)];
    }
    if app.search.hits.is_empty() {
        return Vec::new();
    }
    let mut spans = vec![Span::styled(
        format!(
            "match {} of {}   ",
            app.search.current.saturating_add(1),
            app.search.hits.len()
        ),
        theme.muted(),
    )];
    spans.extend(hint(theme, "n", "next"));
    spans
}

fn mode_status(app: &App) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let theme = &app.theme;
    if app.search.editing {
        return search_status(app);
    }
    if app.yanking {
        let left = hint(theme, "key", "copies that line")
            .into_iter()
            .chain(hint(theme, "esc", "cancel"))
            .collect();
        return (left, Vec::new());
    }
    let left = HINTS
        .iter()
        .flat_map(|(key, label)| hint(theme, key, label))
        .collect();
    (left, browse_right(app))
}

fn status_spans(app: &App) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let theme = &app.theme;
    let (rest, right) = mode_status(app);
    let mut left = vec![
        Span::styled(format!(" {} ", mode(app)), theme.chip(theme.accent)),
        Span::raw("  "),
    ];
    left.extend(rest);
    (left, right)
}

/// The status bar as plain text, the left part then the right part.
pub fn status_text(app: &App) -> String {
    let (left, right) = status_spans(app);
    left.iter()
        .chain(&right)
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn draw_status(app: &App, frame: &mut Frame, area: Rect) {
    let (left, right) = status_spans(app);
    spread(frame, area, Line::from(left), Line::from(right));
}

/// A centered rectangle of the given size, shrunk to fit `area`.
fn popup(area: Rect, (width, height): (u16, u16)) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(row);
    cell
}

fn popup_block(theme: &Theme, title: &str, footer: &str) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.fg(theme.accent))
        .title(Span::styled(
            format!(" {title} "),
            theme.bold().fg(theme.accent),
        ))
        .title_bottom(Line::styled(format!(" {footer} "), theme.muted()).right_aligned())
        .padding(Padding::new(2, 2, 1, 0))
}

/// The target's key, its name and a one-line preview of what it copies, or `none here` in `border` when the row has none.
fn yank_line(app: &App, target: YankTarget, width: usize) -> Line<'static> {
    let theme = &app.theme;
    let name = format!("{:<15}", target.name());
    let text = yank_text(app, target);
    if text.is_empty() {
        let dim = theme.fg(theme.border);
        return Line::from(vec![
            Span::styled(format!("{}  ", target.key()), dim),
            Span::styled(name, dim),
            Span::styled("none here", dim),
        ]);
    }
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview = fit(
        &printable(&flat),
        width.saturating_sub(name.chars().count() + 3),
    );
    Line::from(vec![
        Span::styled(format!("{}  ", target.key()), theme.key()),
        Span::raw(name),
        Span::styled(preview, theme.muted()),
    ])
}

fn draw_yank(app: &App, frame: &mut Frame) {
    let area = popup(frame.area(), COPY_POPUP);
    let width = usize::from(area.width.saturating_sub(6));
    let lines: Vec<Line> = TARGETS
        .iter()
        .map(|target| yank_line(app, *target, width))
        .collect();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(popup_block(&app.theme, "Copy", "esc closes")),
        area,
    );
}

fn draw_help(app: &App, frame: &mut Frame) {
    let theme = &app.theme;
    let lines: Vec<Line> = KEYS
        .iter()
        .map(|(key, what)| {
            Line::from(vec![
                Span::styled(format!("{key:<14}"), theme.key()),
                Span::raw(*what),
            ])
        })
        .collect();
    let area = popup(frame.area(), KEYS_POPUP);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(popup_block(theme, "Keys", "any key closes")),
        area,
    );
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

    fn span<'l>(line: &'l Line<'static>, text: &str) -> &'l Span<'static> {
        line.spans.iter().find(|span| span.content == text).unwrap()
    }

    #[test]
    fn fit_cuts_with_an_ellipsis() {
        assert_eq!(fit("TITLEBLOCK", 20), "TITLEBLOCK");
        assert_eq!(fit("TITLEBLOCK", 10), "TITLEBLOCK");
        assert_eq!(fit("TITLEBLOCK", 6), "TITLE…");
    }

    #[test]
    fn marked_finds_every_occurrence_ignoring_case() {
        let mark = Style::default().add_modifier(Modifier::BOLD);
        let spans = marked("P-101 and p-102", "P-10", Style::default(), mark);
        let parts: Vec<&str> = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(parts, ["", "P-10", "1 and ", "p-10", "2"]);
        assert_eq!(spans[1].style, mark);
        assert_eq!(spans[2].style, Style::default());
        assert_eq!(marked("WALLS", " ", Style::default(), mark).len(), 1);
    }

    #[test]
    fn printable_shows_control_characters_as_dots() {
        assert_eq!(printable("A\u{1b}[2J\tB"), "A.[2J.B");
        assert_eq!(printable("§ 4"), "§ 4");
    }

    #[test]
    fn tree_rows_color_type_groups_entity_types_and_layer_flags() {
        let mut app = demo();
        let theme = app.theme;
        let dimension = row(&app, &["Layouts", "Model", "DIMENSION"]);
        app.select(dimension);
        let line = tree_line(&app, dimension, 40).unwrap();
        assert_eq!(span(&line, "DIMENSION").style.fg, Some(theme.info));
        assert_eq!(span(&line, "▸ ").style.fg, Some(theme.muted));
        assert_eq!(span(&line, "3").style.fg, Some(theme.muted));
        assert!(line.to_string().starts_with("▌ "));
        assert_eq!(line.width(), 40);

        let insert = row(&app, &["Layouts", "Model", "INSERT"]);
        app.select(insert);
        app.update(Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        let entity = app.outline.rows[insert].children[0];
        let line = tree_line(&app, entity, 40).unwrap();
        assert_eq!(span(&line, "INSERT").style.fg, Some(theme.info));
        assert_eq!(span(&line, "  EQUIPMENT  PUMP").style.fg, None);
        assert_eq!(span(&line, "136").style.fg, Some(theme.muted));

        let spans = detail_spans(
            &theme,
            &RowKind::Layer(1),
            "2 entities · frozen · off",
            Style::default(),
        );
        assert_eq!(spans[0].content, "2 entities");
        assert_eq!(spans[0].style.fg, Some(theme.muted));
        assert_eq!(spans[1].content, " · frozen · off");
        assert_eq!(spans[1].style.fg, Some(theme.warning));
        let plain = detail_spans(&theme, &RowKind::Layer(0), "11 entities", Style::default());
        assert_eq!(plain.len(), 1);
    }

    #[test]
    fn source_lines_color_codes_numbers_offsets_and_ascii() {
        let theme = Theme::truecolor();
        let pair = pair_line(&theme, 40, "2.5");
        assert_eq!(pair.to_string(), "  40  2.5");
        assert_eq!(pair.spans[0].style.fg, Some(theme.muted));
        assert_eq!(pair.spans[1].style.fg, Some(theme.number));
        let text = pair_line(&theme, 8, "WALLS\u{7}");
        assert_eq!(text.spans[1].content, "WALLS.");
        assert_eq!(text.spans[1].style.fg, None);

        let dump = "000016ee  20 20 30 0a 49 4e 53 45  52 54 0a 20 20 35 0a 31  |  0.INSERT.  5.1|";
        let line = hex_line(&theme, dump);
        assert_eq!(line.to_string(), dump);
        assert_eq!(line.spans[0].content, "000016ee");
        assert_eq!(line.spans[0].style.fg, Some(theme.muted));
        assert_eq!(line.spans[2].content, "|  0.INSERT.  5.1|");
        assert_eq!(line.spans[2].style.fg, Some(theme.number));
        assert_eq!(hex_line(&theme, "odd").to_string(), "odd");
    }

    #[test]
    fn header_leaves_out_the_units_then_the_counts_when_short_of_room() {
        let mut app = demo();
        let wide = header_left(&app, 200).to_string();
        assert_eq!(
            wide,
            " cadboss   demo.dxf   DXF AC1032   Millimeters   15 entities in 3 layouts"
        );
        assert_eq!(
            header_left(&app, 64).to_string(),
            " cadboss   demo.dxf   DXF AC1032   15 entities in 3 layouts"
        );
        assert_eq!(
            header_left(&app, 40).to_string(),
            " cadboss   demo.dxf   DXF AC1032"
        );
        assert_eq!(header_right(&app).to_string(), "35 text items ");

        app.outline
            .document
            .report
            .note(SkipKind::Entity, SkipReason::Malformed);
        let right = header_right(&app);
        assert_eq!(right.to_string(), "35 text items   ! 1 skipped ");
        assert_eq!(right.spans[1].style.fg, Some(app.theme.warning));

        app.loading = true;
        app.ticks = 11;
        assert_eq!(header_right(&app).to_string(), "⠙ reading the drawing ");
        assert_eq!(
            header_left(&app, 200).to_string(),
            " cadboss   demo.dxf   DXF AC1032   Millimeters"
        );
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
