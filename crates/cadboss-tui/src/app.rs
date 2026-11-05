//! Explorer state and its update function: messages in, state changes and commands for the event loop out.

use std::collections::HashSet;
use std::ops::RangeInclusive;
use std::sync::Arc;

use cadboss_dxf::DxfDocument;
use crossterm::event::{KeyEvent, KeyEventKind};

use crate::clipboard::Transport;
use crate::input::{action, Action, KeyContext};
use crate::outline::{Outline, RowKind, ROOT};
use crate::source::{hex_line_of, pair_line_of, source, Source, View};
use crate::theme::Theme;
use crate::ui::{Splits, RIGHT_TOP_SPLIT, SPLIT_STEP, TREE_SPLIT};
use crate::yank::{yank_text, YankTarget};

/// Ticks (100 ms each) a status-bar toast stays up.
pub const TOAST_TICKS: u32 = 30;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    Tree,
    Inspector,
    Source,
}

#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
    /// The outline built in the background replaces the file row shown while loading.
    Loaded(Box<Outline>),
    Copied {
        target: YankTarget,
        result: Result<Transport, String>,
    },
    /// No native clipboard took the text; the loop falls back to OSC 52.
    NoNativeClipboard {
        target: YankTarget,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cmd {
    /// Hand the text to the native clipboard, off the input loop.
    Copy { target: YankTarget, text: String },
    /// Write the OSC 52 sequence from the loop, between frames.
    Osc52 { target: YankTarget, text: String },
}

#[derive(Clone, Debug, Default)]
pub struct Search {
    /// The prompt is open and taking keys.
    pub editing: bool,
    pub query: String,
    pub hits: Vec<usize>,
    pub current: usize,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub text: String,
    pub ticks_left: u32,
    /// Something went wrong or was not there, as opposed to a done action.
    pub problem: bool,
}

/// The scroll lines of the last view switch: the line left in the old view and the line entered in the new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewSwitch {
    left: usize,
    entered: usize,
}

pub struct App {
    pub title: String,
    /// The path as given on the command line, for copied shell commands.
    pub target: String,
    pub doc: Arc<DxfDocument>,
    pub outline: Outline,
    /// The outline is still being built; only the file row exists.
    pub loading: bool,
    pub ticks: u64,
    /// Every row in tree order, folded or not.
    pub order: Vec<usize>,
    pub expanded: Vec<bool>,
    pub selected: usize,
    pub focus: Pane,
    pub inspector_scroll: u16,
    pub source_scroll: usize,
    /// The selected row's record.
    pub source: Source,
    pub view: View,
    pub search: Search,
    pub yanking: bool,
    /// The key overlay is open; the next key closes it.
    pub help: bool,
    pub toast: Option<Toast>,
    pub theme: Theme,
    pub splits: Splits,
    pub size: (u16, u16),
    pub should_quit: bool,
    last_switch: Option<ViewSwitch>,
}

/// Rows in tree order from the file row, descending only into rows `open` accepts.
fn tree_order(outline: &Outline, open: impl Fn(usize) -> bool) -> Vec<usize> {
    let mut order = Vec::with_capacity(outline.rows.len());
    let mut stack = vec![ROOT];
    while let Some(index) = stack.pop() {
        order.push(index);
        if !open(index) {
            continue;
        }
        let Some(row) = outline.rows.get(index) else {
            continue;
        };
        stack.extend(row.children.iter().rev());
    }
    order
}

/// Binary drawings open on their bytes, text drawings on their pairs.
fn opening_view(doc: &DxfDocument) -> View {
    if doc.tape().is_binary() {
        return View::Hex;
    }
    View::Pairs
}

impl App {
    /// The explorer before its outline arrives: the file row alone, keys other than quit ignored until [`Msg::Loaded`].
    pub fn new(doc: Arc<DxfDocument>, title: String, target: String, size: (u16, u16)) -> App {
        let outline = Outline::pending(&doc, &title);
        let view = opening_view(&doc);
        let mut app = App {
            title,
            target,
            doc,
            outline,
            loading: true,
            ticks: 0,
            order: Vec::new(),
            expanded: Vec::new(),
            selected: ROOT,
            focus: Pane::Tree,
            inspector_scroll: 0,
            source_scroll: 0,
            source: Source::default(),
            view,
            search: Search::default(),
            yanking: false,
            help: false,
            toast: None,
            theme: Theme::default(),
            splits: Splits::default(),
            size,
            should_quit: false,
            last_switch: None,
        };
        app.reset_tree();
        app
    }

    /// Rebuilds the order and folds for the current outline, with the file row open and selected.
    fn reset_tree(&mut self) {
        self.order = tree_order(&self.outline, |_| true);
        self.expanded = vec![false; self.outline.rows.len()];
        self.set_expanded(ROOT, true);
        self.select(ROOT);
    }

    /// Rows the tree pane shows, in order.
    pub fn visible_rows(&self) -> Vec<usize> {
        tree_order(&self.outline, |index| self.is_expanded(index))
    }

    pub fn is_expanded(&self, index: usize) -> bool {
        self.expanded.get(index).copied().unwrap_or(false)
    }

    pub fn has_children(&self, index: usize) -> bool {
        self.outline
            .rows
            .get(index)
            .is_some_and(|row| !row.children.is_empty())
    }

    fn kind(&self, index: usize) -> Option<&RowKind> {
        self.outline.rows.get(index).map(|row| &row.kind)
    }

    fn set_expanded(&mut self, index: usize, value: bool) {
        if let Some(slot) = self.expanded.get_mut(index) {
            *slot = value;
        }
    }

    /// Selects a row, opening every row above it and showing its record.
    /// A `More` row first loads its group's next page, and the first new row is selected in its place.
    pub fn select(&mut self, row: usize) {
        let row = self.load_more(row);
        if self.outline.rows.get(row).is_none() {
            return;
        }
        for ancestor in self.outline.ancestors(row).into_iter().skip(1) {
            self.set_expanded(ancestor, true);
        }
        self.selected = row;
        self.inspector_scroll = 0;
        self.source_scroll = 0;
        self.last_switch = None;
        self.source = source(&self.doc, self.outline.record(&self.doc, row));
    }

    /// The row at the `More` row's place after its group's next page loaded, any other row unchanged.
    fn load_more(&mut self, row: usize) -> usize {
        let Some(&RowKind::More { group }) = self.kind(row) else {
            return row;
        };
        let place = self
            .outline
            .rows
            .get(group)
            .and_then(|above| above.children.iter().rposition(|child| *child == row));
        let Some(place) = place else {
            return row;
        };
        self.load_page(group);
        self.outline
            .rows
            .get(group)
            .and_then(|above| above.children.get(place).copied())
            .unwrap_or(row)
    }

    /// Materializes the first page of a type group that has no entity rows yet.
    fn load_first_page(&mut self, row: usize) {
        let unloaded =
            matches!(self.kind(row), Some(RowKind::TypeGroup { .. })) && !self.has_children(row);
        if !unloaded {
            return;
        }
        self.load_page(row);
    }

    /// Adds the group's next page of entity rows, then refreshes the tree order, the folds and the search hits.
    fn load_page(&mut self, group: usize) {
        self.outline.load_page(&self.doc, group);
        self.order = tree_order(&self.outline, |_| true);
        self.expanded.resize(self.outline.rows.len(), false);
        if self.search.query.trim().is_empty() {
            return;
        }
        self.search.hits = self.matches();
    }

    fn key_context(&self) -> KeyContext {
        if self.search.editing {
            return KeyContext::Search;
        }
        if self.yanking {
            return KeyContext::Yank;
        }
        KeyContext::Normal
    }

    fn page(&self) -> isize {
        isize::try_from(self.size.1.saturating_sub(3))
            .unwrap_or(1)
            .max(1)
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: text.into(),
            ticks_left: TOAST_TICKS,
            problem: false,
        });
    }

    fn problem(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: text.into(),
            ticks_left: TOAST_TICKS,
            problem: true,
        });
    }

    pub fn update(&mut self, msg: Msg) -> Vec<Cmd> {
        match msg {
            Msg::Key(key) => return self.key(key),
            Msg::Loaded(outline) => {
                self.outline = *outline;
                self.loading = false;
                self.reset_tree();
            }
            Msg::NoNativeClipboard { target, text } => return vec![Cmd::Osc52 { target, text }],
            Msg::Resize(width, height) => self.size = (width, height),
            Msg::Tick => self.tick(),
            Msg::Copied { target, result } => self.copied(target, result),
        }
        Vec::new()
    }

    fn key(&mut self, key: KeyEvent) -> Vec<Cmd> {
        if key.kind != KeyEventKind::Press {
            return Vec::new();
        }
        if self.help {
            self.help = false;
            return Vec::new();
        }
        let intent = action(key, self.key_context());
        if self.loading && intent != Action::Quit {
            return Vec::new();
        }
        self.apply(intent)
    }

    fn copied(&mut self, target: YankTarget, result: Result<Transport, String>) {
        let name = target.name();
        match result {
            Ok(Transport::Native) => self.toast(format!("Copied {name}")),
            Ok(Transport::Osc52) => self.toast(format!("Copied {name} through the terminal")),
            Err(error) => self.problem(format!("Could not copy {name}: {error}")),
        }
    }

    fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        let Some(toast) = self.toast.as_mut() else {
            return;
        };
        toast.ticks_left = toast.ticks_left.saturating_sub(1);
        if toast.ticks_left == 0 {
            self.toast = None;
        }
    }

    fn apply(&mut self, intent: Action) -> Vec<Cmd> {
        match intent {
            Action::Quit => self.should_quit = true,
            Action::MoveUp => self.scroll(-1),
            Action::MoveDown => self.scroll(1),
            Action::PageUp => self.scroll(-self.page()),
            Action::PageDown => self.scroll(self.page()),
            Action::Top => self.scroll(isize::MIN),
            Action::Bottom => self.scroll(isize::MAX),
            Action::Collapse => self.collapse(),
            Action::Expand => self.expand(),
            Action::Toggle => self.toggle(),
            Action::Parent => self.parent(),
            Action::FocusNext => {
                self.focus = match self.focus {
                    Pane::Tree => Pane::Inspector,
                    Pane::Inspector => Pane::Source,
                    Pane::Source => Pane::Tree,
                };
            }
            Action::NextLayout => self.next_layout(true),
            Action::PrevLayout => self.next_layout(false),
            Action::ToggleHex => self.toggle_hex(),
            Action::OpenSearch => {
                self.search.editing = true;
                self.search.query.clear();
            }
            Action::SearchChar(c) => {
                self.search.query.push(c);
                self.search.hits = self.matches();
            }
            Action::SearchBackspace => {
                self.search.query.pop();
                self.search.hits = self.matches();
            }
            Action::SearchCancel => self.search.editing = false,
            Action::SearchAccept => self.accept_search(),
            Action::NextHit => self.step_hit(true),
            Action::PrevHit => self.step_hit(false),
            Action::OpenYank => self.yanking = true,
            Action::YankCancel => self.yanking = false,
            Action::Yank(target) => return self.yank(target),
            Action::Help => self.help = true,
            Action::ResizeLeft => self.splits.tree = step(self.splits.tree, false, TREE_SPLIT),
            Action::ResizeRight => self.splits.tree = step(self.splits.tree, true, TREE_SPLIT),
            Action::ResizeUp => {
                self.splits.right_top = step(self.splits.right_top, false, RIGHT_TOP_SPLIT);
            }
            Action::ResizeDown => {
                self.splits.right_top = step(self.splits.right_top, true, RIGHT_TOP_SPLIT);
            }
            Action::Noop => {}
        }
        Vec::new()
    }

    fn scroll(&mut self, delta: isize) {
        match self.focus {
            Pane::Tree => self.move_selection(delta),
            Pane::Inspector => {
                let next = usize::from(self.inspector_scroll).saturating_add_signed(delta);
                self.inspector_scroll = u16::try_from(next).unwrap_or(u16::MAX);
            }
            Pane::Source => {
                let last = self.source.line_count(self.view).saturating_sub(1);
                self.source_scroll = self.source_scroll.saturating_add_signed(delta).min(last);
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let rows = self.visible_rows();
        let position = rows
            .iter()
            .position(|row| *row == self.selected)
            .unwrap_or(0);
        let next = position
            .saturating_add_signed(delta)
            .min(rows.len().saturating_sub(1));
        let Some(row) = rows.get(next).copied() else {
            return;
        };
        self.select(row);
    }

    fn collapse(&mut self) {
        if self.is_expanded(self.selected) && self.has_children(self.selected) {
            self.set_expanded(self.selected, false);
            return;
        }
        self.parent();
    }

    fn expand(&mut self) {
        self.load_first_page(self.selected);
        if !self.has_children(self.selected) {
            return;
        }
        if !self.is_expanded(self.selected) {
            self.set_expanded(self.selected, true);
            return;
        }
        let first = self
            .outline
            .rows
            .get(self.selected)
            .and_then(|row| row.children.first().copied());
        let Some(first) = first else {
            return;
        };
        self.select(first);
    }

    fn toggle(&mut self) {
        if matches!(self.kind(self.selected), Some(RowKind::More { .. })) {
            self.select(self.selected);
            return;
        }
        self.load_first_page(self.selected);
        if !self.has_children(self.selected) {
            return;
        }
        let open = self.is_expanded(self.selected);
        self.set_expanded(self.selected, !open);
    }

    fn parent(&mut self) {
        if self.selected == ROOT {
            return;
        }
        let Some(parent) = self.outline.rows.get(self.selected).map(|row| row.parent) else {
            return;
        };
        self.select(parent);
    }

    /// Every row in tree order, starting next to the selection in the given direction and wrapping around to end on it.
    fn cycle_from_selection(&self, forward: bool) -> Vec<usize> {
        let position = self
            .order
            .iter()
            .position(|row| *row == self.selected)
            .unwrap_or(0);
        let after = self.order.get(position + 1..).unwrap_or(&[]);
        let before = self.order.get(..=position).unwrap_or(&[]);
        let mut rotated: Vec<usize> = after.iter().chain(before).copied().collect();
        if !forward {
            rotated.rotate_right(1);
            rotated.reverse();
        }
        rotated
    }

    /// Selects the next or previous layout row in tree order, wrapping around.
    pub fn next_layout(&mut self, forward: bool) {
        let next = self
            .cycle_from_selection(forward)
            .into_iter()
            .find(|row| matches!(self.kind(*row), Some(RowKind::Layout(_))));
        let Some(next) = next else {
            self.toast("No layouts in this drawing");
            return;
        };
        self.select(next);
    }

    /// Switches between the pairs and hex views at the same byte offset.
    /// Switching back without scrolling in between returns to the line that was left.
    fn toggle_hex(&mut self) {
        let left = self.source_scroll;
        let entered = self
            .last_switch
            .filter(|switch| switch.entered == left)
            .map_or_else(|| self.line_at_same_offset(), |switch| switch.left);
        self.view = match self.view {
            View::Pairs => View::Hex,
            View::Hex => View::Pairs,
        };
        self.source_scroll = entered;
        self.last_switch = Some(ViewSwitch { left, entered });
    }

    /// The line of the other view at the byte offset where the current view's scroll line begins.
    fn line_at_same_offset(&self) -> usize {
        let offsets = self.source.offsets(&self.doc);
        let base = self.source.bytes.start;
        if self.view == View::Hex {
            let offset = base.saturating_add(self.source_scroll.saturating_mul(16));
            return pair_line_of(&offsets, offset);
        }
        let offset = offsets.get(self.source_scroll).copied().unwrap_or(base);
        hex_line_of(offset, base).min(self.source.line_count(View::Hex).saturating_sub(1))
    }

    /// Rows whose label, layer, handle or text hold the query, in tree order, folded or not.
    pub fn matches(&self) -> Vec<usize> {
        let query = self.search.query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        self.order
            .iter()
            .copied()
            .filter(|row| {
                self.outline
                    .haystacks
                    .get(*row)
                    .is_some_and(|haystack| haystack.contains(&query))
            })
            .collect()
    }

    fn accept_search(&mut self) {
        self.search.editing = false;
        self.search.hits = self.matches();
        if self.search.query.trim().is_empty() {
            return;
        }
        let hits: HashSet<usize> = self.search.hits.iter().copied().collect();
        let Some(hit) = std::iter::once(self.selected)
            .chain(self.cycle_from_selection(true))
            .find(|row| hits.contains(row))
        else {
            self.problem(format!("Nothing matches \"{}\"", self.search.query));
            return;
        };
        self.search.current = self
            .search
            .hits
            .iter()
            .position(|row| *row == hit)
            .unwrap_or(0);
        self.select(hit);
    }

    fn step_hit(&mut self, forward: bool) {
        let count = self.search.hits.len();
        if count == 0 {
            return;
        }
        self.search.current = cycle(self.search.current, count, forward);
        let Some(hit) = self.search.hits.get(self.search.current).copied() else {
            return;
        };
        self.select(hit);
    }

    fn yank(&mut self, target: YankTarget) -> Vec<Cmd> {
        self.yanking = false;
        let text = yank_text(self, target);
        if text.is_empty() {
            self.problem(format!("This row has no {}", target.name()));
            return Vec::new();
        }
        vec![Cmd::Copy { target, text }]
    }
}

/// The position after `index` in a cycle of `count`, or before it when not `forward`; 0 in an empty cycle.
fn cycle(index: usize, count: usize, forward: bool) -> usize {
    if forward {
        return index.saturating_add(1).checked_rem(count).unwrap_or(0);
    }
    index
        .saturating_add(count.saturating_sub(1))
        .checked_rem(count)
        .unwrap_or(0)
}

/// `value` moved one `SPLIT_STEP` up or down, kept within `bounds`.
fn step(value: u16, grow: bool, bounds: RangeInclusive<u16>) -> u16 {
    let (low, high) = (*bounds.start(), *bounds.end());
    if grow {
        return value.saturating_add(SPLIT_STEP).max(low).min(high);
    }
    value.saturating_sub(SPLIT_STEP).max(low).min(high)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use cadboss_testkit::dxf::DxfBuilder;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::*;

    const TITLE: &str = "TITLE=PUMP STATION P-101 GENERAL ARRANGEMENT";

    fn open(path: &str) -> Arc<DxfDocument> {
        Arc::new(DxfDocument::open(Path::new(path)).unwrap())
    }

    fn demo() -> Arc<DxfDocument> {
        open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/demo.dxf"
        ))
    }

    fn pending(doc: &Arc<DxfDocument>) -> App {
        App::new(
            Arc::clone(doc),
            "demo.dxf".to_owned(),
            "assets/demo.dxf".to_owned(),
            (80, 24),
        )
    }

    fn loaded(doc: &Arc<DxfDocument>) -> App {
        let mut app = pending(doc);
        app.update(Msg::Loaded(Box::new(Outline::build(doc, "demo.dxf"))));
        app
    }

    fn press(app: &mut App, code: KeyCode) -> Vec<Cmd> {
        app.update(Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    fn press_with(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Vec<Cmd> {
        app.update(Msg::Key(KeyEvent::new(code, modifiers)))
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, KeyCode::Char(c));
        }
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

    fn label(app: &App) -> &str {
        &app.outline.rows[app.selected].label
    }

    fn toast(app: &App) -> (&str, bool) {
        let toast = app.toast.as_ref().unwrap();
        (toast.text.as_str(), toast.problem)
    }

    #[test]
    fn keys_other_than_quit_are_ignored_while_loading() {
        let doc = demo();
        let mut app = pending(&doc);
        assert!(app.loading);
        assert_eq!(app.outline.rows.len(), 1);
        assert_eq!(app.selected, ROOT);
        assert!(app.source.pairs.contains(&(9, "$ACADVER".to_owned())));
        for code in [
            KeyCode::Char('j'),
            KeyCode::Char('/'),
            KeyCode::Char('y'),
            KeyCode::Char('?'),
            KeyCode::Char('x'),
            KeyCode::Char(']'),
            KeyCode::Tab,
            KeyCode::Enter,
        ] {
            assert!(press(&mut app, code).is_empty());
        }
        press_with(&mut app, KeyCode::Left, KeyModifiers::ALT);
        assert!(!app.search.editing);
        assert!(!app.yanking);
        assert!(!app.help);
        assert!(app.toast.is_none());
        assert_eq!(app.focus, Pane::Tree);
        assert_eq!(app.view, View::Pairs);
        assert_eq!(app.splits, Splits::default());

        app.update(Msg::Tick);
        assert_eq!(app.ticks, 1);
        app.update(Msg::Resize(100, 40));
        assert_eq!(app.size, (100, 40));
        assert!(!app.should_quit);

        app.update(Msg::Loaded(Box::new(Outline::build(&doc, "demo.dxf"))));
        assert!(!app.loading);
        assert_eq!(app.selected, ROOT);
        assert!(app.is_expanded(ROOT));
        assert_eq!(app.order.len(), app.outline.rows.len());
        assert_eq!(app.expanded.len(), app.outline.rows.len());
        assert_eq!(
            app.visible_rows(),
            [
                ROOT,
                row(&app, &["Layouts"]),
                row(&app, &["Blocks"]),
                row(&app, &["Layers"]),
                row(&app, &["Text"])
            ]
        );

        let mut quitting = pending(&doc);
        press(&mut quitting, KeyCode::Char('q'));
        assert!(quitting.should_quit);
    }

    #[test]
    fn moving_and_folding_walk_the_visible_rows() {
        let mut app = loaded(&demo());
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(label(&app), "Layouts");
        press(&mut app, KeyCode::Char('l'));
        assert!(app.is_expanded(app.selected));
        press(&mut app, KeyCode::Char('l'));
        assert_eq!(label(&app), "Model");
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(label(&app), "Layout1");
        press(&mut app, KeyCode::Backspace);
        assert_eq!(label(&app), "Layouts");
        press(&mut app, KeyCode::Char('h'));
        assert!(!app.is_expanded(app.selected));
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.selected, ROOT);
        press(&mut app, KeyCode::Char('G'));
        assert_eq!(label(&app), "Text");
        press(&mut app, KeyCode::Enter);
        assert!(app.is_expanded(app.selected));
        press(&mut app, KeyCode::Char(' '));
        assert!(!app.is_expanded(app.selected));
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.selected, ROOT);
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.selected, ROOT);

        let title = row(&app, &["Text", "attributes", TITLE]);
        app.select(title);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Pane::Inspector);
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.inspector_scroll, 1);
        assert_eq!(app.selected, title);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Pane::Source);
        press(&mut app, KeyCode::PageDown);
        assert_eq!(app.source_scroll, 21);
        press(&mut app, KeyCode::End);
        assert_eq!(app.source_scroll, 119);
        press(&mut app, KeyCode::Home);
        assert_eq!(app.source_scroll, 0);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Pane::Tree);

        press(&mut app, KeyCode::Char('?'));
        assert!(app.help);
        let mut release = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert!(app.update(Msg::Key(release)).is_empty());
        assert!(app.help);
        press(&mut app, KeyCode::Char('j'));
        assert!(!app.help);
        assert_eq!(app.selected, title);
    }

    #[test]
    fn search_finds_text_items_without_expanding_entities() {
        let mut app = loaded(&demo());
        press(&mut app, KeyCode::Char('/'));
        assert!(app.search.editing);
        type_text(&mut app, "p-10");
        assert!(app.search.editing);
        assert!(app.search.hits.len() >= 2);
        assert_eq!(app.search.hits.len(), 4);
        assert_eq!(app.selected, ROOT);

        press(&mut app, KeyCode::Enter);
        assert!(!app.search.editing);
        let hits = app.search.hits.clone();
        assert_eq!(app.selected, hits[0]);
        assert_eq!(app.search.current, 0);
        assert!(matches!(
            app.outline.rows[app.selected].kind,
            RowKind::TextItem(..)
        ));
        assert!(label(&app).contains("P-10"));
        assert!(label(&app).starts_with("NOTES:"));
        assert!(app.is_expanded(row(&app, &["Text"])));
        assert!(app.is_expanded(row(&app, &["Text", "annotations"])));
        assert!(app.outline.entities.is_empty());

        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.selected, hits[1]);
        assert_eq!(app.search.current, 1);
        assert_eq!(label(&app), "TAG=P-101");
        assert!(app.is_expanded(row(&app, &["Text", "attributes"])));
        press(&mut app, KeyCode::Char('N'));
        assert_eq!(app.selected, hits[0]);
        press(&mut app, KeyCode::Char('N'));
        assert_eq!(app.selected, hits[3]);
        assert_eq!(app.search.current, 3);
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.selected, hits[0]);

        app.select(hits[2]);
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "P-10");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.selected, hits[2]);
        assert_eq!(app.search.current, 2);

        app.select(hits[3]);
        press(&mut app, KeyCode::Char('j'));
        assert_ne!(app.selected, hits[3]);
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "p-10");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.selected, hits[0]);

        assert_eq!(app.search.current, 0);
        press(&mut app, KeyCode::Char('/'));
        assert!(app.search.query.is_empty());
        assert_eq!(app.search.hits, hits);
        press(&mut app, KeyCode::Esc);
        assert!(!app.search.editing);
        assert!(!app.should_quit);
        assert_eq!(app.selected, hits[0]);
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.selected, hits[1]);

        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "p-10");
        press(&mut app, KeyCode::Esc);
        assert!(!app.search.editing);
        assert_eq!(app.search.hits, hits);
        assert_eq!(app.selected, hits[1]);
    }

    #[test]
    fn search_with_no_hits_toasts() {
        let mut app = loaded(&demo());
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "zzz");
        assert!(app.search.hits.is_empty());
        press(&mut app, KeyCode::Enter);
        assert_eq!(toast(&app), ("Nothing matches \"zzz\"", true));
        assert_eq!(app.selected, ROOT);
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.selected, ROOT);

        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "zzzx");
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.search.query, "zzz");
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Backspace);
        type_text(&mut app, "walls");
        assert_eq!(app.search.hits.len(), 2);
    }

    #[test]
    fn expanding_a_type_group_loads_a_page_and_more_loads_the_next() {
        let bytes = (0..1203)
            .fold(DxfBuilder::new(), |builder, step| {
                let y = f64::from(step);
                builder.line((0.0, y), (1.0, y))
            })
            .build();
        let doc = Arc::new(DxfDocument::load(bytes).unwrap());
        let mut app = App::new(
            Arc::clone(&doc),
            "lines.dxf".to_owned(),
            "lines.dxf".to_owned(),
            (80, 24),
        );
        app.update(Msg::Loaded(Box::new(Outline::build(&doc, "lines.dxf"))));
        let group = row(&app, &["Layouts", "Model", "LINE"]);
        app.select(group);
        assert!(!app.has_children(group));

        press(&mut app, KeyCode::Enter);
        assert!(app.is_expanded(group));
        assert_eq!(app.outline.entities.len(), 500);
        let children = app.outline.rows[group].children.clone();
        assert_eq!(children.len(), 501);
        let more = children[500];
        assert_eq!(app.outline.rows[more].kind, RowKind::More { group });
        assert_eq!(app.outline.rows[more].label, "… 703 more");
        assert_eq!(app.order.len(), app.outline.rows.len());
        assert_eq!(app.expanded.len(), app.outline.rows.len());
        assert_eq!(app.selected, group);

        app.select(children[499]);
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.selected, more);
        assert_eq!(app.outline.rows[more].kind, RowKind::Entity { slot: 500 });
        assert_eq!(app.outline.entities.len(), 1000);
        let children = app.outline.rows[group].children.clone();
        assert_eq!(children.len(), 1001);
        assert_eq!(children[500], more);
        assert_eq!(app.outline.rows[children[1000]].label, "… 203 more");
        assert_eq!(app.order.len(), app.outline.rows.len());
        assert_eq!(app.expanded.len(), app.outline.rows.len());
        assert!(app.source.pairs.contains(&(0, "LINE".to_owned())));

        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "line");
        press(&mut app, KeyCode::Esc);
        let hits = app.search.hits.len();
        assert!(hits >= 1000);
        app.select(children[1000]);
        assert_eq!(app.outline.entities.len(), 1203);
        assert_eq!(app.search.hits.len(), hits + 203);
        assert_eq!(app.order.len(), app.outline.rows.len());

        app.select(group);
        press(&mut app, KeyCode::Enter);
        assert!(!app.is_expanded(group));
        press(&mut app, KeyCode::Enter);
        assert!(app.is_expanded(group));
        assert_eq!(app.outline.entities.len(), 1203);

        let mut demo = loaded(&demo());
        let insert = row(&demo, &["Layouts", "Model", "INSERT"]);
        demo.select(insert);
        press(&mut demo, KeyCode::Char('l'));
        assert!(demo.is_expanded(insert));
        assert_eq!(demo.outline.entities.len(), 3);
        assert_eq!(demo.selected, insert);
        press(&mut demo, KeyCode::Char('l'));
        assert_eq!(demo.selected, demo.outline.rows[insert].children[0]);
        assert_eq!(label(&demo), "INSERT  EQUIPMENT  PUMP");
    }

    #[test]
    fn next_layout_cycles_layout_rows() {
        let mut app = loaded(&demo());
        press(&mut app, KeyCode::Char(']'));
        assert_eq!(label(&app), "Model");
        assert_eq!(app.outline.rows[app.selected].kind, RowKind::Layout(0));
        assert!(app.is_expanded(row(&app, &["Layouts"])));
        press(&mut app, KeyCode::Char(']'));
        assert_eq!(label(&app), "Layout1");
        press(&mut app, KeyCode::Char(']'));
        assert_eq!(label(&app), "Sheet 1");
        press(&mut app, KeyCode::Char(']'));
        assert_eq!(label(&app), "Model");
        press(&mut app, KeyCode::Char('['));
        assert_eq!(label(&app), "Sheet 1");
        assert!(app.toast.is_none());

        let mut waiting = pending(&demo());
        waiting.next_layout(true);
        assert_eq!(toast(&waiting), ("No layouts in this drawing", false));
        assert_eq!(waiting.selected, ROOT);
    }

    #[test]
    fn toggle_hex_keeps_the_byte_offset() {
        let doc = demo();
        let mut app = loaded(&doc);
        assert_eq!(app.view, View::Pairs);
        app.select(row(&app, &["Text", "attributes", TITLE]));
        assert_eq!(app.source.pairs.len(), 120);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        for _ in 0..40 {
            press(&mut app, KeyCode::Char('j'));
        }
        assert_eq!(app.source_scroll, 40);
        let offsets = app.source.offsets(&doc);
        let base = app.source.bytes.start;

        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.view, View::Hex);
        assert_eq!(app.focus, Pane::Source);
        let entered = app.source_scroll;
        assert_eq!(entered, hex_line_of(offsets[40], base));
        let line = base + entered * 16;
        assert!(line <= offsets[40] && offsets[40] < line + 16);

        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.view, View::Pairs);
        let next = base + (entered + 1) * 16;
        assert_eq!(app.source_scroll, pair_line_of(&offsets, next));
        assert!(offsets[app.source_scroll] >= next);
        assert!(offsets[app.source_scroll - 1] < next);
        assert_ne!(app.source_scroll, 40);

        press(&mut app, KeyCode::Char('G'));
        assert_eq!(app.source_scroll, offsets.len() - 1);
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(
            app.source_scroll,
            hex_line_of(*offsets.last().unwrap(), base)
        );
    }

    #[test]
    fn switching_views_back_without_scrolling_returns_to_the_line_left() {
        let doc = demo();
        let mut app = loaded(&doc);
        app.select(row(&app, &["Text", "attributes", TITLE]));
        let offsets = app.source.offsets(&doc);
        let base = app.source.bytes.start;
        let hex_of = |pair: usize| hex_line_of(offsets[pair], base);
        let pair_of = |line: usize| pair_line_of(&offsets, base + line * 16);

        let pair = (40..offsets.len())
            .find(|pair| pair_of(hex_of(*pair)) != *pair)
            .unwrap();
        app.source_scroll = pair;
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.source_scroll, hex_of(pair));
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.view, View::Pairs);
        assert_eq!(app.source_scroll, pair);
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.source_scroll, hex_of(pair));
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.source_scroll, pair);

        let line = (0..app.source.hex.len())
            .find(|line| hex_of(pair_of(*line)) != *line)
            .unwrap();
        press(&mut app, KeyCode::Char('x'));
        app.source_scroll = line;
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.view, View::Pairs);
        assert_eq!(app.source_scroll, pair_of(line));
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.view, View::Hex);
        assert_eq!(app.source_scroll, line);

        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.source_scroll, pair_of(line));
        app.select(app.selected);
        assert_eq!(app.source_scroll, 0);
        app.source_scroll = pair_of(line);
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.source_scroll, hex_of(pair_of(line)));
        assert_ne!(app.source_scroll, line);
    }

    #[test]
    fn binary_drawing_starts_in_hex() {
        let doc = open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/simple-binary.dxf"
        ));
        assert!(doc.tape().is_binary());
        let mut app = App::new(
            Arc::clone(&doc),
            "simple-binary.dxf".to_owned(),
            "simple-binary.dxf".to_owned(),
            (80, 24),
        );
        assert_eq!(app.view, View::Hex);
        app.update(Msg::Loaded(Box::new(Outline::build(
            &doc,
            "simple-binary.dxf",
        ))));
        assert_eq!(app.view, View::Hex);
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.view, View::Pairs);
        assert_eq!(app.source.pairs[0], (9, "$ACADVER".to_owned()));

        assert_eq!(loaded(&demo()).view, View::Pairs);
    }

    #[test]
    fn copy_returns_a_command_and_toasts_on_success_and_failure() {
        let mut app = loaded(&demo());
        app.select(row(&app, &["Text", "attributes", "TAG=P-101"]));
        assert!(press(&mut app, KeyCode::Char('y')).is_empty());
        assert!(app.yanking);
        let cmds = press(&mut app, KeyCode::Char('t'));
        assert!(!app.yanking);
        assert_eq!(
            cmds,
            [Cmd::Copy {
                target: YankTarget::Text,
                text: "TAG=P-101".to_owned()
            }]
        );

        app.update(Msg::Copied {
            target: YankTarget::Text,
            result: Ok(Transport::Native),
        });
        assert_eq!(toast(&app), ("Copied text", false));
        assert_eq!(app.toast.as_ref().unwrap().ticks_left, TOAST_TICKS);

        let cmds = app.update(Msg::NoNativeClipboard {
            target: YankTarget::Text,
            text: "TAG=P-101".to_owned(),
        });
        assert_eq!(
            cmds,
            [Cmd::Osc52 {
                target: YankTarget::Text,
                text: "TAG=P-101".to_owned()
            }]
        );
        app.update(Msg::Copied {
            target: YankTarget::Text,
            result: Ok(Transport::Osc52),
        });
        assert_eq!(toast(&app), ("Copied text through the terminal", false));
        app.update(Msg::Copied {
            target: YankTarget::Hex,
            result: Err("no native clipboard".to_owned()),
        });
        assert_eq!(
            toast(&app),
            ("Could not copy hex: no native clipboard", true)
        );

        press(&mut app, KeyCode::Char('y'));
        assert!(press(&mut app, KeyCode::Char('z')).is_empty());
        assert!(!app.yanking);

        app.select(row(&app, &["Layouts"]));
        press(&mut app, KeyCode::Char('y'));
        assert!(press(&mut app, KeyCode::Char('q')).is_empty());
        assert!(!app.should_quit);
        assert_eq!(toast(&app), ("This row has no jq query", true));

        for _ in 0..TOAST_TICKS - 1 {
            app.update(Msg::Tick);
        }
        assert!(app.toast.is_some());
        app.update(Msg::Tick);
        assert!(app.toast.is_none());
    }

    #[test]
    fn resize_steps_and_clamps() {
        let mut app = loaded(&demo());
        assert_eq!(
            app.splits,
            Splits {
                tree: 42,
                right_top: 62
            }
        );
        press_with(&mut app, KeyCode::Left, KeyModifiers::ALT);
        assert_eq!(app.splits.tree, 37);
        press_with(&mut app, KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(app.splits.tree, 42);
        for _ in 0..20 {
            press_with(&mut app, KeyCode::Left, KeyModifiers::ALT);
        }
        assert_eq!(app.splits.tree, *TREE_SPLIT.start());
        for _ in 0..20 {
            press_with(&mut app, KeyCode::Char('f'), KeyModifiers::ALT);
        }
        assert_eq!(app.splits.tree, *TREE_SPLIT.end());
        press_with(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
        assert_eq!(app.splits.tree, 65);

        press_with(&mut app, KeyCode::Up, KeyModifiers::ALT);
        assert_eq!(app.splits.right_top, 57);
        for _ in 0..20 {
            press_with(&mut app, KeyCode::Up, KeyModifiers::ALT);
        }
        assert_eq!(app.splits.right_top, *RIGHT_TOP_SPLIT.start());
        for _ in 0..20 {
            press_with(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
        }
        assert_eq!(app.splits.right_top, *RIGHT_TOP_SPLIT.end());
        assert_eq!(app.selected, ROOT);

        app.update(Msg::Resize(120, 40));
        assert_eq!(app.size, (120, 40));
    }
}
