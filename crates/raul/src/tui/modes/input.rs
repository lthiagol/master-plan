//! M136: `Mode::Input(_)` handler — the text-input overlay.
//!
//! Pre-M136 this logic lived in `handle_input_event`. M136 lifts it into a
//! per-mode handler so the dispatcher routes `KeyEvent`s to a single
//! function while the Input overlay is up.
//!
//! ## Side effects
//!
//! None. The handler returns [`Action::PushInputChar`] /
//! [`Action::PopInputChar`] / [`Action::SubmitInput`] /
//! [`Action::CancelInput`]; `apply_action` is what actually mutates
//! `app.active_mode` and shells out to `mp annotation create`.
//!
//! ## Modifiers
//!
//! Modifiers other than Shift are rejected — the pre-M136 input handler
//! matched `KeyCode::Char(c)` unconditionally but the global dispatcher
//! always ran the input check *before* any modifier-gated global keys,
//! so a Ctrl+key never reached the input buffer. We keep that contract.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::action::Action;

pub fn handle_key(key: KeyEvent) -> Vec<Action> {
    // Modifiers other than Shift are not consumed by the input buffer in
    // pre-M136 code; reject them. (Pre-M136 effectively rejected them
    // because the global q/Q/Esc/Tab checks were modifier-gated and ran
    // *before* the input check — here we mirror that ordering by
    // short-circuiting on non-Shift modifier keys.)
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return Vec::new();
    }

    match key.code {
        KeyCode::Esc => vec![Action::CancelInput],
        KeyCode::Enter => vec![Action::SubmitInput],
        KeyCode::Backspace => vec![Action::PopInputChar],
        KeyCode::Char(c) => vec![Action::PushInputChar(c)],
        _ => Vec::new(),
    }
}
