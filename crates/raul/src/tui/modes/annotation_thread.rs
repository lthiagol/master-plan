//! M136: `Mode::AnnotationThread` handler.
//!
//! Pre-M136 the `ContentState::AnnotationThread` arm of `handle_event`
//! handled Up/Down/Resolve/Reopen/CreateAnnotation/Enter. M136 promotes
//! AnnotationThread to a mode so the per-mode table is just a flat
//! `key → Action` mapping (the content-state/Event 2-D table moves into
//! [`crate::tui::action::apply_action`] via the dedicated `Annotation*`
//! actions).
//!
//! Global keys in AnnotationThread:
//!
//! * `q`/`Q`      → Quit
//! * `Esc`        → `Action::CloseAnnotationThread` (close the thread,
//!   return to MilestoneDetail). This mirrors pre-M136, where Esc on
//!   `ContentState::AnnotationThread` called `app.go_back()` and fell
//!   out of the inline `Mode::AnnotationThread` state.
//!
//! Threads never show help / review-menu / input overlays; those still
//! require an action that doesn't reach this handler.
//!
//! ## Side effects
//!
//! None. `apply_action` owns `App` mutation and `mp` shell-outs.

use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::action::Action;

pub fn handle_key(key: KeyEvent) -> Vec<Action> {
    if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
        return vec![Action::Quit];
    }
    if key.code == KeyCode::Char('Q') && key.modifiers.is_empty() {
        return vec![Action::Quit];
    }
    if key.code == KeyCode::Esc && key.modifiers.is_empty() {
        return vec![Action::CloseAnnotationThread];
    }

    if !key.modifiers.is_empty() {
        return Vec::new();
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => vec![Action::Up],
        KeyCode::Down | KeyCode::Char('j') => vec![Action::Down],
        KeyCode::PageUp => vec![Action::PageUp],
        KeyCode::PageDown => vec![Action::PageDown],
        KeyCode::Char('r') => vec![Action::ResolveAnnotation],
        KeyCode::Char('R') => vec![Action::ReopenAnnotation],
        KeyCode::Char('A') => vec![Action::CreateAnnotation],
        KeyCode::Enter => vec![Action::EnterCoApproval],
        _ => Vec::new(),
    }
}
