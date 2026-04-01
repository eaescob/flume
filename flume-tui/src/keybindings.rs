use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use flume_core::config::keybindings::KeybindingMode;

/// A hashable key combination.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyCombo {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn plain(code: KeyCode) -> Self {
        Self { code, modifiers: KeyModifiers::NONE }
    }

    pub fn ctrl(c: char) -> Self {
        Self { code: KeyCode::Char(c), modifiers: KeyModifiers::CONTROL }
    }

    pub fn alt(code: KeyCode) -> Self {
        Self { code, modifiers: KeyModifiers::ALT }
    }

    pub fn alt_char(c: char) -> Self {
        Self { code: KeyCode::Char(c), modifiers: KeyModifiers::ALT }
    }

    pub fn from_event(event: &KeyEvent) -> Self {
        // Strip SHIFT from modifiers for regular chars (crossterm includes it for uppercase)
        let mods = if matches!(event.code, KeyCode::Char(_)) {
            event.modifiers & !KeyModifiers::SHIFT
        } else {
            event.modifiers
        };
        Self { code: event.code, modifiers: mods }
    }
}

/// Every action the keybinding system can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    // Text editing
    DeleteCharBack,
    DeleteCharForward,
    DeleteWordBack,
    DeleteToLineStart,
    DeleteToLineEnd,
    TransposeChars,
    CursorLeft,
    CursorRight,
    CursorWordLeft,
    CursorWordRight,
    CursorHome,
    CursorEnd,
    // History
    HistoryPrev,
    HistoryNext,
    // Submission
    Submit,
    TabComplete,
    // Buffer navigation
    ScrollUp,
    ScrollDown,
    BufferNext,
    BufferPrev,
    BufferJump(u8),
    ServerCycle,
    // App
    Quit,
    SwapSplitFocus,
    // Vi-specific (only in vi normal mode map)
    ViEnterInsert,
    ViEnterInsertAfter,
    ViEnterInsertEnd,
    ViEnterInsertStart,
    ViDeleteChar,
    ViDeleteCharBack,
    ViDeleteLine,
    ViChangeLine,
    ViChangeToEnd,
    ViEnterNormal,
}

/// A set of keybinding maps for a given mode.
pub struct Keymap {
    pub global: HashMap<KeyCombo, InputAction>,
    pub mode: HashMap<KeyCombo, InputAction>,
}

/// Build the global bindings that work in every mode.
fn global_bindings() -> HashMap<KeyCombo, InputAction> {
    let mut m = HashMap::new();

    // App control
    m.insert(KeyCombo::ctrl('c'), InputAction::Quit);
    m.insert(KeyCombo::ctrl('x'), InputAction::ServerCycle);

    // Buffer navigation
    for i in 1u8..=9 {
        m.insert(
            KeyCombo::alt_char((b'0' + i) as char),
            InputAction::BufferJump(i),
        );
    }
    m.insert(KeyCombo::alt(KeyCode::Left), InputAction::BufferPrev);
    m.insert(KeyCombo::alt(KeyCode::Right), InputAction::BufferNext);

    // Split focus
    m.insert(KeyCombo::alt(KeyCode::Tab), InputAction::SwapSplitFocus);

    // Scrolling
    m.insert(KeyCombo::plain(KeyCode::PageUp), InputAction::ScrollUp);
    m.insert(KeyCombo::plain(KeyCode::PageDown), InputAction::ScrollDown);

    // Submission
    m.insert(KeyCombo::plain(KeyCode::Enter), InputAction::Submit);
    m.insert(KeyCombo::plain(KeyCode::Tab), InputAction::TabComplete);

    m
}

/// Build the emacs-mode keymap (readline-style).
fn emacs_bindings() -> HashMap<KeyCombo, InputAction> {
    let mut m = HashMap::new();

    // Cursor movement
    m.insert(KeyCombo::ctrl('b'), InputAction::CursorLeft);
    m.insert(KeyCombo::ctrl('f'), InputAction::CursorRight);
    m.insert(KeyCombo::ctrl('a'), InputAction::CursorHome);
    m.insert(KeyCombo::ctrl('e'), InputAction::CursorEnd);
    m.insert(KeyCombo::alt_char('b'), InputAction::CursorWordLeft);
    m.insert(KeyCombo::alt_char('f'), InputAction::CursorWordRight);

    // Deletion
    m.insert(KeyCombo::ctrl('d'), InputAction::DeleteCharForward);
    m.insert(KeyCombo::ctrl('k'), InputAction::DeleteToLineEnd);
    m.insert(KeyCombo::ctrl('u'), InputAction::DeleteToLineStart);
    m.insert(KeyCombo::ctrl('w'), InputAction::DeleteWordBack);
    m.insert(KeyCombo::alt(KeyCode::Backspace), InputAction::DeleteWordBack);
    m.insert(KeyCombo::ctrl('t'), InputAction::TransposeChars);

    // History
    m.insert(KeyCombo::ctrl('p'), InputAction::HistoryPrev);
    m.insert(KeyCombo::ctrl('n'), InputAction::HistoryNext);

    // Arrow keys + basic editing (same as current behavior)
    m.insert(KeyCombo::plain(KeyCode::Left), InputAction::CursorLeft);
    m.insert(KeyCombo::plain(KeyCode::Right), InputAction::CursorRight);
    m.insert(KeyCombo::plain(KeyCode::Home), InputAction::CursorHome);
    m.insert(KeyCombo::plain(KeyCode::End), InputAction::CursorEnd);
    m.insert(KeyCombo::plain(KeyCode::Up), InputAction::HistoryPrev);
    m.insert(KeyCombo::plain(KeyCode::Down), InputAction::HistoryNext);
    m.insert(KeyCombo::plain(KeyCode::Backspace), InputAction::DeleteCharBack);
    m.insert(KeyCombo::plain(KeyCode::Delete), InputAction::DeleteCharForward);

    // Esc quits in emacs mode (preserves current behavior)
    m.insert(KeyCombo::plain(KeyCode::Esc), InputAction::Quit);

    m
}

/// Build the vi insert-mode keymap.
fn vi_insert_bindings() -> HashMap<KeyCombo, InputAction> {
    let mut m = HashMap::new();

    // Esc → normal mode
    m.insert(KeyCombo::plain(KeyCode::Esc), InputAction::ViEnterNormal);

    // Basic editing (arrow keys etc)
    m.insert(KeyCombo::plain(KeyCode::Left), InputAction::CursorLeft);
    m.insert(KeyCombo::plain(KeyCode::Right), InputAction::CursorRight);
    m.insert(KeyCombo::plain(KeyCode::Home), InputAction::CursorHome);
    m.insert(KeyCombo::plain(KeyCode::End), InputAction::CursorEnd);
    m.insert(KeyCombo::plain(KeyCode::Up), InputAction::HistoryPrev);
    m.insert(KeyCombo::plain(KeyCode::Down), InputAction::HistoryNext);
    m.insert(KeyCombo::plain(KeyCode::Backspace), InputAction::DeleteCharBack);
    m.insert(KeyCombo::plain(KeyCode::Delete), InputAction::DeleteCharForward);

    // Some readline bindings that don't conflict with vi
    m.insert(KeyCombo::ctrl('w'), InputAction::DeleteWordBack);
    m.insert(KeyCombo::ctrl('u'), InputAction::DeleteToLineStart);
    m.insert(KeyCombo::ctrl('k'), InputAction::DeleteToLineEnd);

    m
}

/// Build the vi normal-mode keymap.
fn vi_normal_bindings() -> HashMap<KeyCombo, InputAction> {
    let mut m = HashMap::new();

    // Mode switching
    m.insert(KeyCombo::plain(KeyCode::Char('i')), InputAction::ViEnterInsert);
    m.insert(KeyCombo::plain(KeyCode::Char('a')), InputAction::ViEnterInsertAfter);
    m.insert(KeyCombo::new(KeyCode::Char('A'), KeyModifiers::NONE), InputAction::ViEnterInsertEnd);
    m.insert(KeyCombo::new(KeyCode::Char('I'), KeyModifiers::NONE), InputAction::ViEnterInsertStart);

    // Cursor movement
    m.insert(KeyCombo::plain(KeyCode::Char('h')), InputAction::CursorLeft);
    m.insert(KeyCombo::plain(KeyCode::Char('l')), InputAction::CursorRight);
    m.insert(KeyCombo::plain(KeyCode::Char('w')), InputAction::CursorWordRight);
    m.insert(KeyCombo::plain(KeyCode::Char('b')), InputAction::CursorWordLeft);
    m.insert(KeyCombo::plain(KeyCode::Char('e')), InputAction::CursorWordRight); // simplified: same as w
    m.insert(KeyCombo::plain(KeyCode::Char('0')), InputAction::CursorHome);
    m.insert(KeyCombo::new(KeyCode::Char('$'), KeyModifiers::NONE), InputAction::CursorEnd);
    m.insert(KeyCombo::plain(KeyCode::Char('^')), InputAction::CursorHome);
    m.insert(KeyCombo::plain(KeyCode::Left), InputAction::CursorLeft);
    m.insert(KeyCombo::plain(KeyCode::Right), InputAction::CursorRight);
    m.insert(KeyCombo::plain(KeyCode::Home), InputAction::CursorHome);
    m.insert(KeyCombo::plain(KeyCode::End), InputAction::CursorEnd);

    // Deletion
    m.insert(KeyCombo::plain(KeyCode::Char('x')), InputAction::ViDeleteChar);
    m.insert(KeyCombo::new(KeyCode::Char('X'), KeyModifiers::NONE), InputAction::ViDeleteCharBack);
    m.insert(KeyCombo::new(KeyCode::Char('C'), KeyModifiers::NONE), InputAction::ViChangeToEnd);

    // History
    m.insert(KeyCombo::plain(KeyCode::Char('j')), InputAction::HistoryNext);
    m.insert(KeyCombo::plain(KeyCode::Char('k')), InputAction::HistoryPrev);
    m.insert(KeyCombo::plain(KeyCode::Up), InputAction::HistoryPrev);
    m.insert(KeyCombo::plain(KeyCode::Down), InputAction::HistoryNext);

    m
}

/// Build the complete keymap for a given mode.
pub fn build_keymap(mode: KeybindingMode) -> Keymap {
    let global = global_bindings();
    let mode_map = match mode {
        KeybindingMode::Emacs | KeybindingMode::Custom => emacs_bindings(),
        // For Vi mode, the insert keymap is used as the default mode map.
        // Normal mode map is separate and used when vi_mode == Normal.
        KeybindingMode::Vi => vi_insert_bindings(),
    };
    Keymap { global, mode: mode_map }
}

/// Build the vi normal-mode keymap (only needed when mode == Vi).
pub fn build_vi_normal_keymap() -> HashMap<KeyCombo, InputAction> {
    vi_normal_bindings()
}

/// Resolve a key event to an action, checking global bindings first, then mode-specific.
pub fn resolve(
    event: &KeyEvent,
    keymap: &Keymap,
    vi_normal_map: Option<&HashMap<KeyCombo, InputAction>>,
    is_vi_normal: bool,
) -> Option<InputAction> {
    let combo = KeyCombo::from_event(event);

    // Global bindings take priority
    if let Some(action) = keymap.global.get(&combo) {
        return Some(*action);
    }

    // Vi normal mode has its own map
    if is_vi_normal {
        if let Some(map) = vi_normal_map {
            return map.get(&combo).copied();
        }
    }

    // Mode-specific (emacs or vi-insert)
    keymap.mode.get(&combo).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_bindings_present() {
        let g = global_bindings();
        assert_eq!(g.get(&KeyCombo::ctrl('c')), Some(&InputAction::Quit));
        assert_eq!(g.get(&KeyCombo::ctrl('x')), Some(&InputAction::ServerCycle));
        assert_eq!(g.get(&KeyCombo::alt_char('1')), Some(&InputAction::BufferJump(1)));
        assert_eq!(g.get(&KeyCombo::alt_char('9')), Some(&InputAction::BufferJump(9)));
        assert_eq!(g.get(&KeyCombo::plain(KeyCode::PageUp)), Some(&InputAction::ScrollUp));
        assert_eq!(g.get(&KeyCombo::plain(KeyCode::Enter)), Some(&InputAction::Submit));
        assert_eq!(g.get(&KeyCombo::plain(KeyCode::Tab)), Some(&InputAction::TabComplete));
    }

    #[test]
    fn emacs_readline_bindings() {
        let m = emacs_bindings();
        assert_eq!(m.get(&KeyCombo::ctrl('a')), Some(&InputAction::CursorHome));
        assert_eq!(m.get(&KeyCombo::ctrl('e')), Some(&InputAction::CursorEnd));
        assert_eq!(m.get(&KeyCombo::ctrl('b')), Some(&InputAction::CursorLeft));
        assert_eq!(m.get(&KeyCombo::ctrl('f')), Some(&InputAction::CursorRight));
        assert_eq!(m.get(&KeyCombo::ctrl('d')), Some(&InputAction::DeleteCharForward));
        assert_eq!(m.get(&KeyCombo::ctrl('k')), Some(&InputAction::DeleteToLineEnd));
        assert_eq!(m.get(&KeyCombo::ctrl('u')), Some(&InputAction::DeleteToLineStart));
        assert_eq!(m.get(&KeyCombo::ctrl('w')), Some(&InputAction::DeleteWordBack));
        assert_eq!(m.get(&KeyCombo::ctrl('p')), Some(&InputAction::HistoryPrev));
        assert_eq!(m.get(&KeyCombo::ctrl('n')), Some(&InputAction::HistoryNext));
        assert_eq!(m.get(&KeyCombo::alt_char('b')), Some(&InputAction::CursorWordLeft));
        assert_eq!(m.get(&KeyCombo::alt_char('f')), Some(&InputAction::CursorWordRight));
    }

    #[test]
    fn emacs_arrow_keys() {
        let m = emacs_bindings();
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Left)), Some(&InputAction::CursorLeft));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Right)), Some(&InputAction::CursorRight));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Up)), Some(&InputAction::HistoryPrev));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Down)), Some(&InputAction::HistoryNext));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Backspace)), Some(&InputAction::DeleteCharBack));
    }

    #[test]
    fn vi_insert_has_esc() {
        let m = vi_insert_bindings();
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Esc)), Some(&InputAction::ViEnterNormal));
    }

    #[test]
    fn vi_insert_basic_editing() {
        let m = vi_insert_bindings();
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Left)), Some(&InputAction::CursorLeft));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Backspace)), Some(&InputAction::DeleteCharBack));
        assert_eq!(m.get(&KeyCombo::ctrl('w')), Some(&InputAction::DeleteWordBack));
    }

    #[test]
    fn vi_normal_movement() {
        let m = vi_normal_bindings();
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('h'))), Some(&InputAction::CursorLeft));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('l'))), Some(&InputAction::CursorRight));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('w'))), Some(&InputAction::CursorWordRight));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('b'))), Some(&InputAction::CursorWordLeft));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('0'))), Some(&InputAction::CursorHome));
    }

    #[test]
    fn vi_normal_mode_switching() {
        let m = vi_normal_bindings();
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('i'))), Some(&InputAction::ViEnterInsert));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('a'))), Some(&InputAction::ViEnterInsertAfter));
    }

    #[test]
    fn vi_normal_editing() {
        let m = vi_normal_bindings();
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('x'))), Some(&InputAction::ViDeleteChar));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('j'))), Some(&InputAction::HistoryNext));
        assert_eq!(m.get(&KeyCombo::plain(KeyCode::Char('k'))), Some(&InputAction::HistoryPrev));
    }

    #[test]
    fn resolve_global_takes_priority() {
        let keymap = build_keymap(KeybindingMode::Emacs);
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = resolve(&event, &keymap, None, false);
        assert_eq!(result, Some(InputAction::Quit));
    }

    #[test]
    fn resolve_emacs_mode() {
        let keymap = build_keymap(KeybindingMode::Emacs);
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let result = resolve(&event, &keymap, None, false);
        assert_eq!(result, Some(InputAction::CursorHome));
    }

    #[test]
    fn resolve_vi_normal_mode() {
        let keymap = build_keymap(KeybindingMode::Vi);
        let vi_normal = build_vi_normal_keymap();
        let event = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        let result = resolve(&event, &keymap, Some(&vi_normal), true);
        assert_eq!(result, Some(InputAction::CursorLeft));
    }

    #[test]
    fn resolve_vi_insert_mode() {
        let keymap = build_keymap(KeybindingMode::Vi);
        let vi_normal = build_vi_normal_keymap();
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = resolve(&event, &keymap, Some(&vi_normal), false);
        assert_eq!(result, Some(InputAction::ViEnterNormal));
    }

    #[test]
    fn resolve_unbound_key_returns_none() {
        let keymap = build_keymap(KeybindingMode::Emacs);
        let event = KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE);
        let result = resolve(&event, &keymap, None, false);
        assert_eq!(result, None);
    }

    #[test]
    fn key_combo_from_event_strips_shift_for_chars() {
        // Uppercase 'A' comes with SHIFT modifier from crossterm
        let event = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
        let combo = KeyCombo::from_event(&event);
        assert_eq!(combo.modifiers, KeyModifiers::NONE);
        assert_eq!(combo.code, KeyCode::Char('A'));
    }

    #[test]
    fn key_combo_from_event_preserves_ctrl() {
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let combo = KeyCombo::from_event(&event);
        assert_eq!(combo.modifiers, KeyModifiers::CONTROL);
    }
}
