//! M185: Mode::LifecycleFilter key handler.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::action::Action;

pub fn handle_key(key: KeyEvent) -> Vec<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL)
        || key.modifiers.contains(KeyModifiers::ALT)
        || key.modifiers.contains(KeyModifiers::SUPER)
    {
        return Vec::new();
    }
    match key.code {
        KeyCode::Esc => vec![Action::LifecycleFilterCancel],
        KeyCode::Enter => vec![Action::LifecycleFilterCommit],
        KeyCode::Up | KeyCode::Char('k') => vec![Action::LifecycleFilterPrev],
        KeyCode::Down | KeyCode::Char('j') => vec![Action::LifecycleFilterNext],
        KeyCode::Char(' ') => vec![Action::LifecycleFilterToggle],
        _ => Vec::new(),
    }
}
