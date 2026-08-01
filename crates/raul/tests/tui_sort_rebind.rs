//! M172 S5: sort-rebind inline menu.
//!
//! Tests cover:
//! - `open_sort_rebind` opens a menu with the documented keys for the
//!   active lane and remembers the current sort key as the highlight
//! - `cycle_sort_rebind_next` / `cycle_sort_rebind_prev` wrap around
//!   the menu (Last → First and First → Last)
//! - `confirm_sort_rebind` writes the highlighted key to the lane's
//!   sort preference and closes the menu
//! - `cancel_sort_rebind` closes the menu without writing
//! - Lanes without a sort menu (Path / Overview / Settings) reject
//!   `open_sort_rebind` (the menu is empty, so cycles are no-ops)

use raul::tui::app::{App, Lane, SortKey};

#[test]
fn tui_sort_rebind_opens_with_default_keys_for_milestones() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.open_sort_rebind();
    assert!(app.sort_rebind_open(), "menu must open");
    assert_eq!(
        app.sort_rebind_menu.as_ref().map(|k| k.len()),
        Some(5),
        "Milestones lane exposes 5 sort keys (Id/Title/Priority/Lifecycle/Updated)"
    );
}

#[test]
fn tui_sort_rebind_default_highlights_current_key() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    // Set the lane's sort key to Lifecycle before opening.
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Lifecycle);
    app.open_sort_rebind();
    assert_eq!(
        app.sort_rebind_highlight(),
        Some(SortKey::Lifecycle),
        "menu highlight defaults to the lane's current sort key"
    );
}

#[test]
fn tui_sort_rebind_cycles_wrap_around() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.open_sort_rebind();

    // Default highlight = current sort (Id by default).
    assert_eq!(app.sort_rebind_highlight(), Some(SortKey::Id));

    // Cycle prev wraps to the last key.
    app.cycle_sort_rebind_prev();
    assert_eq!(
        app.sort_rebind_highlight(),
        Some(SortKey::Updated),
        "prev wraps from Id to Updated (last key)"
    );

    // Cycle next wraps back to Id.
    app.cycle_sort_rebind_next();
    assert_eq!(
        app.sort_rebind_highlight(),
        Some(SortKey::Id),
        "next wraps from Updated back to Id"
    );
}

#[test]
fn tui_sort_rebind_confirm_writes_lane_preference() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.open_sort_rebind();
    // M187: cycle order is Id → Title → Priority → Lifecycle → Updated;
    // one press highlights Title.
    app.cycle_sort_rebind_next(); // highlight = Title
    app.confirm_sort_rebind();
    assert!(!app.sort_rebind_open(), "menu must close on confirm");
    assert_eq!(
        app.lane_sort_key(Lane::Milestones),
        SortKey::Title,
        "lane's sort key must reflect the bound choice"
    );
}

#[test]
fn tui_sort_rebind_cancel_does_not_write() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.open_sort_rebind();
    app.cycle_sort_rebind_next();
    app.cancel_sort_rebind();
    assert!(!app.sort_rebind_open(), "menu must close on cancel");
    assert_eq!(
        app.lane_sort_key(Lane::Milestones),
        SortKey::Id,
        "cancel must NOT change the lane's sort key"
    );
}

#[test]
fn tui_sort_rebind_unsupported_lane_rejects_open() {
    // Path / Overview / Settings don't expose the menu — `open_sort_rebind`
    // leaves the menu closed (the keys vector is empty).
    let unsupported = [Lane::Path, Lane::Overview, Lane::Settings];
    for lane in &unsupported {
        let mut app = App::new();
        app.select_lane(lane.clone());
        app.open_sort_rebind();
        assert!(
            !app.sort_rebind_open(),
            "lane {:?} should not expose a sort menu",
            lane
        );
    }
}

#[test]
fn tui_sort_rebind_per_lane_persistence() {
    // Bind Milestones to Title and Backlog to Priority. Each lane
    // reads its own preference via `lane_sort_key`. M187: per-lane
    // key set is in column order — milestones cycle
    // Id → Title → Priority → Lifecycle → Updated; backlog cycles
    // Id → Title → Priority → Status.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.open_sort_rebind();
    app.cycle_sort_rebind_next(); // Id → Title
    app.confirm_sort_rebind();

    app.select_lane(Lane::Backlog);
    app.open_sort_rebind();
    app.cycle_sort_rebind_next(); // Id → Title
    app.cycle_sort_rebind_next(); // Title → Priority
    app.confirm_sort_rebind();

    assert_eq!(app.lane_sort_key(Lane::Milestones), SortKey::Title);
    assert_eq!(app.lane_sort_key(Lane::Backlog), SortKey::Priority);
}
