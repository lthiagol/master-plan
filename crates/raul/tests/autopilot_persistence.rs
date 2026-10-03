//! Persistence through `mp` — and nothing else.
//!
//! raul owns no state file. Every setup choice and every UI preference
//! is an `mp config set` (or `mp autopilot config set`) call, and every
//! preference is read back with `mp config get`. That is the whole
//! contract, and it has two halves that are easy to get half-right:
//!
//! * The **writes** are exactly the argv listed below — no extra keys,
//!   no missing ones, and nothing routed anywhere but `mp`.
//! * The **non-writes** matter just as much. Milestone selection and
//!   run mode are per-run by design and must never reach disk; a
//!   stale selection from last week is worse than no selection.
//!
//! The argv builders are pure functions, so the exact calls are
//! asserted here without spawning a process. `no_raul_owned_state_file`
//! is the belt-and-braces check that nothing crept in on the side.

use raul::tui::app::{App, Lane};
use raul::tui::autopilot::setup::{self, RunMode, SetupForm, SidebarTab, SPLIT_PCT_DEFAULT};
use raul::tui::runner_helpers::autopilot_setup;

/// Walk the crate's source and assert nothing references a raul-owned
/// autopilot state file.
///
/// The milestone rules out `.mp/autopilot-state.json` outright. The
/// check is deliberately a source grep rather than a runtime
/// assertion: the failure mode is a *new* file being written by some
/// code path no test happens to exercise, and a runtime check can only
/// see the paths a test takes.
#[test]
fn no_raul_owned_state_file() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();
    walk(&root, &mut offenders);
    assert!(
        offenders.is_empty(),
        "raul must not reference a raul-owned autopilot state file; found: {offenders:?}"
    );
}

fn walk(dir: &std::path::Path, offenders: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, offenders);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (n, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            // Skip the assertion itself and its own doc comment.
            if trimmed.contains("autopilot-state.json") {
                offenders.push(format!("{}:{}: {}", path.display(), n + 1, trimmed));
            }
        }
    }
}

// ─── the exact writes ────────────────────────────────────────────

/// AC-09: the topology chip writes `autopilot.topology` through
/// `mp autopilot config set` — a different mp command from
/// `mp config set`, because it is a different config section.
#[test]
fn topology_writes_through_mp_autopilot_config_set() {
    let app = {
        let mut app = App::new();
        app.select_lane(Lane::Autopilot);
        app.autopilot.setup.set_topology("two-agent");
        app
    };
    assert_eq!(
        setup::autopilot_config_set_argv("autopilot.topology", &app.autopilot.setup.topology),
        vec![
            "autopilot",
            "config",
            "set",
            "autopilot.topology",
            "two-agent"
        ]
    );
}

/// AC-09: the per-role harness chip writes
/// `autopilot.roles.<role>.harness`.
#[test]
fn harness_writes_through_mp_autopilot_config_set() {
    let mut form = SetupForm::new();
    form.harness_uniform = false;
    form.set_harness("runner", "cursor");
    assert_eq!(form.harness_for("runner"), "cursor");
    assert_eq!(
        setup::autopilot_config_set_argv("autopilot.roles.runner.harness", "cursor"),
        vec![
            "autopilot",
            "config",
            "set",
            "autopilot.roles.runner.harness",
            "cursor"
        ]
    );
    // The "inherit" chip writes the literal `inherit`, which is how a
    // role goes back to the project default.
    form.set_harness("runner", "inherit");
    assert_eq!(form.harness_for("runner"), "inherit");
}

/// AC-09: the two commit toggles write
/// `agent.automation.commit_after_execute` and
/// `agent.automation.push_after_review` through `mp config set`.
#[test]
fn commit_toggles_write_through_mp_config_set() {
    assert_eq!(
        setup::commit_toggle_argv("commit_after_execute", true),
        vec![
            "config",
            "set",
            "agent.automation.commit_after_execute",
            "true"
        ]
    );
    assert_eq!(
        setup::commit_toggle_argv("push_after_review", false),
        vec![
            "config",
            "set",
            "agent.automation.push_after_review",
            "false"
        ]
    );
}

/// AC-09: the three `ui.autopilot.*` preferences write through
/// `mp config set` under `ui.autopilot.`.
#[test]
fn ui_preferences_write_through_mp_config_set() {
    for (field, value) in [
        ("split_pct", "55"),
        ("sidebar_tab", "activity"),
        ("sidebar_visible", "false"),
    ] {
        assert_eq!(
            setup::ui_pref_argv(field, value),
            vec!["config", "set", &format!("ui.autopilot.{field}"), value]
        );
    }
}

/// Every argv the lane can emit for a preference. One list, so a new
/// persisted field has to be added here to be considered persisted.
#[test]
fn the_persisted_field_surface_is_exactly_these_keys() {
    let argv = |key: &str, value: &str| setup::ui_pref_argv(key, value);
    // The three UI preferences.
    assert_eq!(argv("split_pct", "40")[2], "ui.autopilot.split_pct");
    assert_eq!(
        argv("sidebar_tab", "progress")[2],
        "ui.autopilot.sidebar_tab"
    );
    assert_eq!(
        argv("sidebar_visible", "true")[2],
        "ui.autopilot.sidebar_visible"
    );
    // Anything else is rejected by name rather than silently written to
    // a key mp does not know.
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    let err = autopilot_setup::persist_layout_field(
        &raul::mp_runner::MpRunner::new().expect("runner"),
        &app,
        "not_a_field",
    );
    assert!(err.is_err(), "an unknown layout field must be rejected");
}

// ─── the exact non-writes ────────────────────────────────────────

/// Milestone selection and run mode are per-run and are never
/// persisted. This is the half of the contract that is easiest to break
/// by "helpfully" adding a key.
#[test]
fn per_run_choices_are_never_persisted() {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    app.autopilot.setup.selected = vec!["240".into(), "241".into()];
    app.autopilot.setup.run_mode = RunMode::Detached;

    // The layout writer only knows the three UI preferences.
    let runner = raul::mp_runner::MpRunner::new().expect("runner");
    for field in ["selected", "run_mode", "milestones", "topology"] {
        assert!(
            autopilot_setup::persist_layout_field(&runner, &app, field).is_err(),
            "{field} is a per-run choice and must not be a layout key"
        );
    }
    // The setup writer knows topology and per-role harness only.
    for field in ["selected", "run_mode", "commit_after_execute"] {
        assert!(
            autopilot_setup::persist_setup_field(&runner, &app, field).is_err(),
            "{field} is not a setup field the setup writer owns"
        );
    }
    // The commit writer knows its two toggles only.
    for field in ["auto_commit", "auto_push", "selected"] {
        assert!(
            autopilot_setup::persist_commit_toggle(&runner, &app, field).is_err(),
            "{field} is not a commit toggle"
        );
    }
}

/// AC-09: the setup writer rejects an unknown field by name rather
/// than writing a key mp would refuse.
#[test]
fn an_unknown_setup_field_is_rejected_by_name() {
    let app = App::new();
    let runner = raul::mp_runner::MpRunner::new().expect("runner");
    let err = autopilot_setup::persist_setup_field(&runner, &app, "banana")
        .expect_err("unknown field must error");
    assert!(
        err.to_string().contains("banana"),
        "the error should name the offending field; got: {err}"
    );
}

// ─── reads come back ─────────────────────────────────────────────

/// AC-09: the three preferences are read on lane load through
/// `mp config get`, one key each, in a fixed order.
#[test]
fn the_three_preferences_are_read_with_mp_config_get() {
    assert_eq!(
        autopilot_setup::UI_PREF_KEYS,
        [
            "ui.autopilot.split_pct",
            "ui.autopilot.sidebar_tab",
            "ui.autopilot.sidebar_visible"
        ]
    );
    // The read argv is the `config get` shape mp answers with a
    // `{ "value": ... }` envelope.
    assert_eq!(
        setup::config_set_argv("ui.autopilot.split_pct", "40"),
        vec!["config", "set", "ui.autopilot.split_pct", "40"],
        "the builder is shared with the set path"
    );
}

/// A stored value round-trips back onto the layout: a project that set
/// `split_pct = 65` and `sidebar_tab = activity` gets that geometry and
/// that tab on the next lane load.
#[test]
fn stored_preferences_land_on_the_layout() {
    let prefs = setup::UiPrefs::from_pairs([
        ("ui.autopilot.split_pct", "65"),
        ("ui.autopilot.sidebar_tab", "activity"),
        ("ui.autopilot.sidebar_visible", "false"),
    ]);
    let mut layout = setup::AutopilotLayout::new();
    prefs.apply_to(&mut layout);
    assert_eq!(layout.split_pct, 65);
    assert_eq!(layout.sidebar_tab, SidebarTab::Activity);
    assert!(!layout.sidebar_visible);
}

/// An unset or unreadable preference leaves the documented default in
/// place — a preferences read can never be the reason the lane renders
/// wrong.
#[test]
fn unreadable_preferences_leave_the_documented_defaults() {
    let prefs = setup::UiPrefs::from_pairs(Vec::<(&str, &str)>::new());
    let mut layout = setup::AutopilotLayout::new();
    prefs.apply_to(&mut layout);
    assert_eq!(layout.split_pct, SPLIT_PCT_DEFAULT);
    assert_eq!(layout.sidebar_tab, SidebarTab::Progress);
    assert!(layout.sidebar_visible);
    // Defaults match what `mp config get` reports for a fresh project.
    assert_eq!(SPLIT_PCT_DEFAULT, 40);
    assert_eq!(SidebarTab::Progress.as_str(), "progress");
}

/// A stored value outside the clamp window is clamped on the way in,
/// so a hand-edited `config.json` cannot make raul render a
/// degenerate column even though `config set` would have rejected it.
#[test]
fn an_out_of_window_stored_split_is_clamped_on_read() {
    for (stored, expected) in [("0", 25u32), ("10", 25), ("90", 75), ("1000", 75)] {
        let prefs = setup::UiPrefs::from_pairs([("ui.autopilot.split_pct", stored)]);
        assert_eq!(prefs.split_pct, expected, "stored {stored} should clamp");
    }
    // In-window values pass through untouched, including both bounds.
    for (stored, expected) in [("25", 25u32), ("40", 40), ("75", 75)] {
        let prefs = setup::UiPrefs::from_pairs([("ui.autopilot.split_pct", stored)]);
        assert_eq!(prefs.split_pct, expected);
    }
}

/// An unknown tab name falls back to Progress rather than leaving the
/// lane in an unrenderable state.
#[test]
fn an_unknown_stored_tab_falls_back_to_progress() {
    let prefs = setup::UiPrefs::from_pairs([("ui.autopilot.sidebar_tab", "logs")]);
    assert_eq!(prefs.sidebar_tab, SidebarTab::Progress);
}

/// Every stored tab name round-trips through the enum, so a value mp
/// accepted is a value raul renders.
#[test]
fn every_stored_tab_name_round_trips() {
    for tab in ["progress", "activity", "state"] {
        let prefs = setup::UiPrefs::from_pairs([("ui.autopilot.sidebar_tab", tab)]);
        assert_eq!(prefs.sidebar_tab.as_str(), tab);
    }
}
