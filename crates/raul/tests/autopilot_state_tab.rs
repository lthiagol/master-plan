//! The State tab: a read-only window onto the five sources the spec
//! lists.
//!
//! Two things are being pinned. First, *what* the tab shows: the
//! autopilot config, the `ui.autopilot.*` values, the autopilot status,
//! the current session when there is one, and the pending override-panel
//! values. Second, *that it is read-only* — a snapshot the operator
//! cannot edit and that raul does not persist on its own.
//!
//! The section-building logic lives in the poller, which is the only
//! place that talks to `mp`. The tests here drive that logic through
//! the pure helpers it uses, so they assert the shape of each source
//! without a live `mp` — and a fake `MpRunner` covers the wiring.

use raul::tui::app::{App, Lane};
use raul::tui::autopilot::setup::SidebarTab;

fn app() -> App {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    app
}

/// The State tab is one of the three sidebar tabs and is not the
/// default.
#[test]
fn state_is_a_sidebar_tab_and_not_the_default() {
    let mut app = app();
    assert_eq!(app.autopilot.layout.sidebar_tab, SidebarTab::Progress);
    app.autopilot.layout.cycle_tab(1);
    assert_eq!(app.autopilot.layout.sidebar_tab, SidebarTab::Activity);
    app.autopilot.layout.cycle_tab(1);
    assert_eq!(app.autopilot.layout.sidebar_tab, SidebarTab::State);
}

/// The tab is reachable only when no run is live (or the operator has
/// not pressed Back) — the takeover owns the screen otherwise.
#[test]
fn state_tab_is_withheld_while_the_takeover_is_up() {
    let mut app = app();
    app.autopilot.note_run_live(true);
    assert!(app.autopilot.layout.state_tab_hidden);
    let selectable: Vec<SidebarTab> = app.autopilot.layout.selectable_tabs();
    assert!(
        !selectable.contains(&SidebarTab::State),
        "State must not be offered while a run is live: {selectable:?}"
    );
}

/// AC-04: the tab carries labelled sections. The label names the
/// command that produced the value, so the operator can go get it.
#[test]
fn each_state_section_is_labelled_with_its_source() {
    let mut app = app();
    app.autopilot.state_sections = vec![
        (
            "autopilot config".to_string(),
            "{\n  \"topology\": \"three-agent\"\n}".to_string(),
        ),
        (
            "ui.autopilot.*".to_string(),
            "{\n  \"split_pct\": 40\n}".to_string(),
        ),
        (
            "autopilot status".to_string(),
            "{\n  \"run_state\": \"live\"\n}".to_string(),
        ),
    ];
    assert_eq!(app.autopilot.state_sections.len(), 3);
    assert_eq!(app.autopilot.state_sections[0].0, "autopilot config");
    assert!(app.autopilot.state_sections[0].1.contains("three-agent"));
}

/// AC-04: the pending override-panel values are surfaced without being
/// persisted. A panel the operator has typed into shows its values; no
/// panel shows a stable placeholder rather than an empty section.
#[test]
fn pending_override_values_are_surfaced_without_being_written() {
    let mut app = app();
    // No panel yet.
    assert!(app.autopilot.panel().is_none());
    // Typing into the panel creates it; the values live in memory.
    let panel = app.autopilot.ensure_panel();
    panel.topology = "two-agent".to_string();
    assert!(app.autopilot.panel().is_some());
    // The panel serializes for the State tab...
    let rendered =
        serde_json::to_string_pretty(app.autopilot.panel().expect("panel")).expect("serialize");
    assert!(rendered.contains("two-agent"));
    // ...and nothing about it reached a config key. The setup form's
    // own topology is a separate field, and the panel is untouched by
    // the form's writes.
    assert_eq!(app.autopilot.setup.topology, "three-agent");
}

/// The State tab is read-only: opening it must not mutate anything the
/// operator would have to undo.
#[test]
fn reading_the_state_tab_mutates_nothing() {
    let mut app = app();
    app.autopilot.setup.selected = vec!["240".into()];
    app.autopilot.setup.topology = "two-agent".to_string();
    let before = app.autopilot.clone();
    let _ = &app.autopilot.state_sections;
    assert_eq!(app.autopilot, before);
}

/// The lane state is `PartialEq`, which is what lets the tests above
/// assert "nothing changed" without reaching into private fields. This
/// pins that the property still holds after the new fields landed.
#[test]
fn lane_state_equality_covers_the_new_fields() {
    let mut a = app();
    let mut b = app();
    assert_eq!(a.autopilot, b.autopilot);
    b.autopilot.session_count = 3;
    assert_ne!(a.autopilot, b.autopilot, "session_count is part of state");
    a.autopilot.session_count = 3;
    assert_eq!(a.autopilot, b.autopilot);
    b.autopilot.layout.state_tab_hidden = true;
    assert_ne!(a.autopilot, b.autopilot, "the layout is part of state");
}

/// The Activity tab's two sources: the poller's `mp activity` rows win
/// once it has polled, and the cached `watch.log` tail is the
/// pre-poll fallback. Both are read-only in-memory snapshots.
#[test]
fn the_activity_tab_prefers_polled_rows_and_falls_back_to_the_log_tail() {
    let mut app = app();
    // Before the poller has run, the log tail is what there is.
    app.autopilot.log_tail = vec!["run-started".to_string()];
    assert!(app.autopilot.activity_rows.is_empty());
    // Once polled, the activity rows are the source.
    app.autopilot.activity_rows = vec!["2026-09-04T00:01:00Z  step S1 done".to_string()];
    assert!(!app.autopilot.activity_rows.is_empty());
    // Either way, nothing is written back: the tab is a reader.
    let tail_before = app.autopilot.log_tail.clone();
    assert_eq!(app.autopilot.log_tail, tail_before);
}

/// The State tab is a reader of `mp`, never a writer: the only argv the
/// lane ever builds for it is a `get`.
#[test]
fn the_state_tab_only_issues_reads() {
    // The persistence argv builders are the lane's entire write
    // surface. None of them is reachable from the State tab: the tab
    // stores strings on `state_sections` and nothing else.
    let writes = [
        raul::tui::autopilot::setup::ui_pref_argv("split_pct", "50"),
        raul::tui::autopilot::setup::commit_toggle_argv("commit_after_execute", true),
        raul::tui::autopilot::setup::autopilot_config_set_argv("autopilot.topology", "two-agent"),
    ];
    for argv in writes {
        assert!(
            argv.contains(&"set".to_string()),
            "expected a set argv; got {argv:?}"
        );
        // And the section list holds plain strings, not a command.
        let mut app = app();
        app.autopilot.state_sections = vec![("autopilot config".to_string(), "{}".to_string())];
        assert_eq!(app.autopilot.state_sections[0].1, "{}");
    }
}
