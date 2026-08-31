//! M169-rev2 user report: 'h' was dispatching as `PreviousLane` on
//! List view (the "h" tab jumped to Overview) instead of
//! `ToggleHideDone`. The `previous_lane` default binding included 'h'
//! as a vim-style alias; the dispatch in `modes::normal::handle_key`
//! matched `previous_lane` BEFORE the per-lane handler could dispatch
//! `ToggleHideDone`. The fix in `modes/normal.rs` overrides the
//! `PreviousLane` action with `ToggleHideDone` when the key also
//! matches `hide_done` AND the active content is `List`.
//!
//! Pre-fix repro:
//!   1. On Milestones tab (List), press `h` → tab indicator moves to
//!      Overview (PreviousLane), not toggle hide-done.
//!   2. Press `1` → moves to Overview too (digit-1 = Overview lane).
//!
//! Post-M169-rev2: 'h' on List view → `ToggleHideDone`. 'h' on
//! Detail view → `PreviousLane` (no contextual meaning, vim alias
//! wins).
//!
//! M200 S1: the `h` alias was dropped from `previous_lane` entirely
//! (the deconflict moved the overlap from contextual routing to a
//! cleaner default-config layer). The Detail-view test below was
//! rewritten to assert that `h` no longer emits any action on
//! MilestoneDetail — the previous-lane routing now lives on `Left`
//! and `BackTab`. The List-view tests still apply: `h` on a list
//! still toggles `hide_done` (it now matches only `hide_done`,
//! not the contextual `previous_lane`).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::app::{App, ContentState, Lane};
use raul::tui::modes;

/// `h` key — empty modifiers, no shift.
fn h_key() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty())
}

#[test]
fn rev2_h_on_milestones_list_emits_toggle_hide_done_not_previous_lane() {
    // **User report:** pressing `h` on the previous screen (Milestones)
    // moved the highlighted tab to Overview (PreviousLane). Same as
    // pressing `1`. Fix: on List view, `h` = ToggleHideDone.
    let mut app = App::new();
    assert_eq!(app.active_lane, Lane::Overview, "sanity: default lane");
    app.active_lane = Lane::Milestones;
    app.content = ContentState::List;

    let actions = modes::normal::handle_key(h_key(), &app);
    assert_eq!(
        actions,
        vec![Action::ToggleHideDone],
        "BUG (M169-rev2 user report): 'h' on Milestones List must emit ToggleHideDone, not PreviousLane"
    );
}

#[test]
fn rev2_h_on_overview_list_emits_toggle_hide_done() {
    let mut app = App::new();
    app.active_lane = Lane::Overview;
    app.content = ContentState::List;

    let actions = modes::normal::handle_key(h_key(), &app);
    assert_eq!(
        actions,
        vec![Action::ToggleHideDone],
        "'h' on Overview List must toggle hide_done"
    );
}

#[test]
fn rev2_h_on_backlog_list_emits_toggle_hide_done() {
    let mut app = App::new();
    app.active_lane = Lane::Backlog;
    app.content = ContentState::List;

    let actions = modes::normal::handle_key(h_key(), &app);
    assert_eq!(
        actions,
        vec![Action::ToggleHideDone],
        "'h' on Backlog List must toggle hide_done"
    );
}

#[test]
fn rev2_h_on_milestone_detail_emits_no_action_after_m200() {
    // M200 S1: the `h` alias was dropped from `previous_lane` to
    // deconflict with `keybinds.hide_done`. The user can still move
    // back to the list with `Left` or `BackTab` (the remaining
    // `previous_lane` defaults), but `h` itself no longer matches.
    // On MilestoneDetail there is no other `h` binding, so `h`
    // emits no action — the contextual routing that previously
    // emitted `PreviousLane` here no longer fires.
    let mut app = App::new();
    app.active_lane = Lane::Milestones;
    app.content = ContentState::MilestoneDetail;

    let actions = modes::normal::handle_key(h_key(), &app);
    assert!(
        actions.is_empty(),
        "M200 S1: 'h' on MilestoneDetail is no longer a PreviousLane alias; got: {actions:?}"
    );
}

#[test]
fn rev2_press_1_still_jumps_to_overview() {
    // The user's report noted 'h' had the same effect as '1'. After
    // the fix, 'h' toggles hide_done while '1' still jumps to
    // Overview (the first lane). Pin the regression so a future
    // change doesn't conflate them again.
    let app = App::new();
    assert_eq!(app.active_lane, Lane::Overview);
    let actions = modes::normal::handle_key(
        KeyEvent::new(KeyCode::Char('1'), KeyModifiers::empty()),
        &app,
    );
    assert_eq!(
        actions,
        vec![Action::JumpLane(0)],
        "digit-1 must still jump to lane 0 (Overview)"
    );
}

#[test]
fn rev2_other_lane_nav_keys_unaffected_by_h_fix() {
    // The 'h' fix is targeted — 'l', Left, Right, Tab, BackTab
    // should all still emit their normal lane-nav actions. Spot-check
    // a few.
    let mut app = App::new();
    app.active_lane = Lane::Milestones;
    app.content = ContentState::List;

    assert_eq!(
        modes::normal::handle_key(
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::empty()),
            &app
        ),
        vec![Action::NextLane]
    );
    assert_eq!(
        modes::normal::handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()), &app),
        vec![Action::NextLane]
    );
    assert_eq!(
        modes::normal::handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::empty()), &app),
        vec![Action::PreviousLane]
    );
    // `h` still emits ToggleHideDone when content is List (the
    // common case). The "hide_done not bound" branch would require
    // constructing a Keybinds with hide_done=[]; the dispatcher
    // is pure, so we trust any_matches here. The targeted checks
    // above cover the real bug.
    let actions = modes::normal::handle_key(h_key(), &app);
    assert_eq!(actions, vec![Action::ToggleHideDone]);
}
