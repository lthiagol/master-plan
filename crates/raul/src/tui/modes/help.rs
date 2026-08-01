//! M136: `Mode::Help` handler — the help-overlay screen.
//!
//! Pre-M136 the help-open guard was the first branch in `handle_event`:
//! only `Event::Help` (i.e. `?`) and `Event::Quit` (`q`/`Q`) closed the
//! overlay; every other key was a no-op. We mirror that table exactly
//! because the help screen intentionally consumes *every* key the user
//! presses (typing a slash into the help overlay would be confusing).
//!
//! The small asymmetry pre-M136 had: on the Overview lane, 'Q' (capital)
//! also closed help + quit, while 'q' (lowercase) didn't reach the help
//! branch (it was caught by the Overview dispatcher's help-open guard that
//! only mapped 'Q'). We keep that: help.rs maps both 'Q' (close + quit)
//! and 'q' (close only) for parity, because on every non-Overview mode
//! the original `q`/`Q` both closed help + quit, and dropping that would
//! change the existing keyboard contract.
//!
//! ## Side effects
//!
//! None. `apply_action::CloseHelp` is what flips `app.active_mode` back to
//! `Normal`.

use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::action::Action;

pub fn handle_key(key: KeyEvent) -> Vec<Action> {
    if !key.modifiers.is_empty() {
        return Vec::new();
    }
    match key.code {
        KeyCode::Char('?') => vec![Action::CloseHelp],
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            vec![Action::CloseHelp, Action::Quit]
        }
        _ => Vec::new(),
    }
}
