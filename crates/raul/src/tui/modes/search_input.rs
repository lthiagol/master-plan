//! M186: Mode::SearchInput handler.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::action::Action;

pub fn handle_key(key: KeyEvent) -> Vec<Action> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return Vec::new();
    }
    match key.code {
        KeyCode::Esc => vec![Action::SearchInputCancel],
        KeyCode::Enter => vec![Action::SearchInputCommit],
        KeyCode::Backspace => vec![Action::SearchInputBackspace],
        KeyCode::Char(c) => vec![Action::SearchInputChar(c)],
        _ => Vec::new(),
    }
}
