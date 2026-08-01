//! M136: `Mode::ReviewMenu(_)` handler.
//!
//! Pre-M136 the review-menu-open arm of `handle_event` handled
//! Up/Down/PageUp/PageDown/Enter. M136 promotes ReviewMenu to a mode;
//! the menu's selection state lives inside the variant, so the handler
//! stays trivially pure — it just emits `Action::Up` / `Action::Down` and
//! `apply_action` advances the selection index inside the variant.
//!
//! ## Side effects
//!
//! None. `apply_action` for `Action::ExecuteReviewAction` reads the
//! selected item from `Mode::ReviewMenu` and shells out to `mp`
//! accordingly.
//!
//! ## Closing the menu
//!
//! Esc and `q` close the menu (and only the menu) without quitting the
//! TUI: the menu state lives inside `Mode::ReviewMenu(_)` and discarding
//! the variant drops the selection by construction.

use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::action::Action;

pub fn handle_key(key: KeyEvent) -> Vec<Action> {
    if !key.modifiers.is_empty() {
        return Vec::new();
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
            vec![Action::CloseReviewMenu]
        }
        KeyCode::Up | KeyCode::Char('k') => vec![Action::Up],
        KeyCode::Down | KeyCode::Char('j') => vec![Action::Down],
        KeyCode::PageUp => vec![Action::PageUp],
        KeyCode::PageDown => vec![Action::PageDown],
        KeyCode::Enter => vec![Action::ExecuteReviewAction],
        _ => Vec::new(),
    }
}
