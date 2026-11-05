//! Key events to intents; pure so the bindings are unit-testable.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::yank::YankTarget;

/// Where keys route: the search prompt and the copy popup capture keys before
/// the normal bindings apply.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyContext {
    Normal,
    Search,
    Yank,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Quit,
    OpenSearch,
    SearchChar(char),
    SearchBackspace,
    SearchAccept,
    SearchCancel,
    NextHit,
    PrevHit,
    MoveUp,
    MoveDown,
    Collapse,
    Expand,
    Toggle,
    Parent,
    FocusNext,
    Top,
    Bottom,
    PageUp,
    PageDown,
    NextLayout,
    PrevLayout,
    ToggleHex,
    OpenYank,
    Yank(YankTarget),
    YankCancel,
    Help,
    ResizeLeft,
    ResizeRight,
    ResizeUp,
    ResizeDown,
    Noop,
}

/// Maps a key press within `context`. Alt+arrows resize the panes, with
/// Ctrl+arrows for terminals that swallow Alt and Alt+b / Alt+f for the
/// Option+Left/Right word motions macOS terminals send.
pub fn action(key: KeyEvent, context: KeyContext) -> Action {
    if key.kind != KeyEventKind::Press {
        return Action::Noop;
    }
    match context {
        KeyContext::Search => search_action(key.code),
        KeyContext::Yank => YankTarget::from_key(key.code).map_or(Action::YankCancel, Action::Yank),
        KeyContext::Normal => normal_action(key),
    }
}

fn search_action(code: KeyCode) -> Action {
    match code {
        KeyCode::Esc => Action::SearchCancel,
        KeyCode::Enter => Action::SearchAccept,
        KeyCode::Backspace => Action::SearchBackspace,
        KeyCode::Char(c) => Action::SearchChar(c),
        _ => Action::Noop,
    }
}

fn resize_action(key: KeyEvent) -> Option<Action> {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match (key.code, alt || ctrl, alt) {
        (KeyCode::Left, true, _) | (KeyCode::Char('b'), _, true) => Some(Action::ResizeLeft),
        (KeyCode::Right, true, _) | (KeyCode::Char('f'), _, true) => Some(Action::ResizeRight),
        (KeyCode::Up, true, _) => Some(Action::ResizeUp),
        (KeyCode::Down, true, _) => Some(Action::ResizeDown),
        _ => None,
    }
}

fn normal_action(key: KeyEvent) -> Action {
    if let Some(resize) = resize_action(key) {
        return resize;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('/') => Action::OpenSearch,
        KeyCode::Char('n') => Action::NextHit,
        KeyCode::Char('N') => Action::PrevHit,
        KeyCode::Up | KeyCode::Char('k') => Action::MoveUp,
        KeyCode::Down | KeyCode::Char('j') => Action::MoveDown,
        KeyCode::Left | KeyCode::Char('h') => Action::Collapse,
        KeyCode::Right | KeyCode::Char('l') => Action::Expand,
        KeyCode::Enter | KeyCode::Char(' ') => Action::Toggle,
        KeyCode::Backspace => Action::Parent,
        KeyCode::Tab => Action::FocusNext,
        KeyCode::Char('g') | KeyCode::Home => Action::Top,
        KeyCode::Char('G') | KeyCode::End => Action::Bottom,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Char(']') => Action::NextLayout,
        KeyCode::Char('[') => Action::PrevLayout,
        KeyCode::Char('x') => Action::ToggleHex,
        KeyCode::Char('y') => Action::OpenYank,
        KeyCode::Char('?') => Action::Help,
        _ => Action::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn normal_keys_map_to_actions() {
        let normal = |code| action(press(code), KeyContext::Normal);
        assert_eq!(normal(KeyCode::Char('q')), Action::Quit);
        assert_eq!(normal(KeyCode::Esc), Action::Quit);
        assert_eq!(normal(KeyCode::Char('/')), Action::OpenSearch);
        assert_eq!(normal(KeyCode::Char('j')), Action::MoveDown);
        assert_eq!(normal(KeyCode::Char('k')), Action::MoveUp);
        assert_eq!(normal(KeyCode::Char('h')), Action::Collapse);
        assert_eq!(normal(KeyCode::Char('l')), Action::Expand);
        assert_eq!(normal(KeyCode::Enter), Action::Toggle);
        assert_eq!(normal(KeyCode::Char(']')), Action::NextLayout);
        assert_eq!(normal(KeyCode::Char('[')), Action::PrevLayout);
        assert_eq!(normal(KeyCode::Char('x')), Action::ToggleHex);
        assert_eq!(normal(KeyCode::Char('y')), Action::OpenYank);
        assert_eq!(normal(KeyCode::Char('?')), Action::Help);
        assert_eq!(normal(KeyCode::Char('m')), Action::Noop);
    }

    #[test]
    fn search_captures_printable_keys() {
        let search = |code| action(press(code), KeyContext::Search);
        assert_eq!(search(KeyCode::Char('q')), Action::SearchChar('q'));
        assert_eq!(search(KeyCode::Esc), Action::SearchCancel);
        assert_eq!(search(KeyCode::Enter), Action::SearchAccept);
        assert_eq!(search(KeyCode::Backspace), Action::SearchBackspace);
    }

    #[test]
    fn yank_keys_pick_targets_and_others_cancel() {
        let yank = |code| action(press(code), KeyContext::Yank);
        assert_eq!(yank(KeyCode::Char('x')), Action::Yank(YankTarget::Hex));
        assert_eq!(yank(KeyCode::Char('d')), Action::Yank(YankTarget::Dxf));
        assert_eq!(yank(KeyCode::Char('z')), Action::YankCancel);
    }

    #[test]
    fn alt_and_ctrl_arrows_resize() {
        let normal = |key| action(key, KeyContext::Normal);
        assert_eq!(
            normal(with(KeyCode::Left, KeyModifiers::ALT)),
            Action::ResizeLeft
        );
        assert_eq!(
            normal(with(KeyCode::Right, KeyModifiers::CONTROL)),
            Action::ResizeRight
        );
        assert_eq!(
            normal(with(KeyCode::Up, KeyModifiers::ALT)),
            Action::ResizeUp
        );
        assert_eq!(
            normal(with(KeyCode::Down, KeyModifiers::CONTROL)),
            Action::ResizeDown
        );
        assert_eq!(
            normal(with(KeyCode::Char('b'), KeyModifiers::ALT)),
            Action::ResizeLeft
        );
        assert_eq!(
            normal(with(KeyCode::Char('f'), KeyModifiers::ALT)),
            Action::ResizeRight
        );
        assert_eq!(normal(press(KeyCode::Char('b'))), Action::Noop);
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut key = press(KeyCode::Char('q'));
        key.kind = KeyEventKind::Release;
        assert_eq!(action(key, KeyContext::Normal), Action::Noop);
    }
}
