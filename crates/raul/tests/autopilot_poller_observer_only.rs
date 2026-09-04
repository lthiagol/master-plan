//! M217 / AC-06 — the poller is observer-only.
//!
//! raul is a viewer, not the control plane. This file pins that
//! mechanically along three axes:
//!
//! * **Argv.** Every command the poller issues comes from
//!   [`poll_argv`], and every row of it passes
//!   [`is_observer_only_argv`] — `mp autopilot` reads only, no
//!   control verbs, never `herdr`.
//! * **Source.** A fixture over `poll.rs` proves no `herdr agent
//!   prompt` (nor any `herdr` invocation, nor a
//!   `Command::new`) exists on the poll path — a future edit that
//!   adds one fails here rather than in production.
//! * **Outcome.** Driving the poller against a nonexistent `mp`
//!   binary — the closest available stand-in for "raul is absent
//!   / broken" — leaves session state untouched and never errors,
//!   so orchestration outcomes cannot depend on raul.

use raul::mp_runner::MpRunner;
use raul::tui::app::App;
use raul::tui::poll::{is_observer_only_argv, poll_argv, poll_autopilot_lane, AutopilotPoller};

fn poll_source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("poll.rs");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    // Production code only — the module's own `#[cfg(test)]` block
    // legitimately names the strings the guards reject.
    match src.find("#[cfg(test)]") {
        Some(i) => src[..i].to_string(),
        None => src,
    }
}

// ─── argv contract ──────────────────────────────────────────────

#[test]
fn m217_ac06_all_refresh_data_comes_from_mp_autopilot_read_apis() {
    let rows = poll_argv("alpha");
    assert_eq!(rows.len(), 2, "exactly two reads per poll: {rows:?}");
    assert_eq!(
        rows[0],
        vec!["autopilot", "session", "show", "alpha", "--format", "json"]
    );
    assert_eq!(rows[1], vec!["autopilot", "status", "--format", "json"]);
    for row in &rows {
        assert!(
            is_observer_only_argv(row),
            "poller argv must be observer-only: {row:?}"
        );
    }
}

#[test]
fn m217_ac06_argv_never_mentions_herdr() {
    for row in poll_argv("alpha") {
        for arg in &row {
            assert!(
                !arg.contains("herdr"),
                "raul must never dispatch through herdr: {row:?}"
            );
        }
    }
}

#[test]
fn m217_ac06_empty_session_id_still_produces_a_valid_read() {
    let rows = poll_argv("");
    for row in &rows {
        assert!(is_observer_only_argv(row), "{row:?}");
    }
}

#[test]
fn m217_ac06_control_verbs_are_rejected_by_the_argv_guard() {
    let rejected = [
        vec!["autopilot", "control", "pause", "alpha"],
        vec!["autopilot", "control", "resume", "alpha"],
        vec!["autopilot", "control", "cancel", "alpha"],
        vec!["autopilot", "control", "steer", "alpha", "go"],
        vec!["autopilot", "start", "217"],
        vec!["autopilot", "session", "update", "alpha"],
        vec!["milestone", "complete", "217"],
        vec!["watch", "217", "--detach"],
        vec!["herdr", "agent", "prompt", "w1:p1", "hi"],
    ];
    for argv in rejected {
        let owned: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        assert!(
            !is_observer_only_argv(&owned),
            "must be rejected as a mutation: {argv:?}"
        );
    }
}

#[test]
fn m217_ac06_read_verbs_are_accepted_by_the_argv_guard() {
    let accepted = [
        vec!["autopilot", "status", "--format", "json"],
        vec!["autopilot", "session", "show", "alpha", "--format", "json"],
        vec!["autopilot", "session", "list", "--format", "json"],
    ];
    for argv in accepted {
        let owned: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        assert!(is_observer_only_argv(&owned), "must be accepted: {argv:?}");
    }
}

#[test]
fn m217_ac06_empty_argv_is_rejected() {
    assert!(!is_observer_only_argv(&[]));
}

// ─── source fixture ─────────────────────────────────────────────

#[test]
fn m217_ac06_poll_module_issues_no_herdr_prompt() {
    let src = poll_source();
    // Scan executable lines only — doc comments discuss `herdr`
    // precisely because the module must never invoke it, and the
    // rejection guard names the string it rejects. Everything
    // else mentioning it is a bug.
    for (i, line) in src.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with("//") || code.starts_with("*") {
            continue;
        }
        if code.contains("a.contains(\"herdr\")") || code.contains("argv.iter().any") {
            continue; // the rejection guard itself
        }
        assert!(
            !code.contains("herdr"),
            "poll.rs:{} must not invoke herdr — raul is not in the dispatch path: {line:?}",
            i + 1
        );
    }
    // No direct process spawning either: every shell-out goes
    // through `MpRunner`, which can only run the `mp` binary.
    for forbidden in ["Command::new", "std::process::Command"] {
        assert!(
            !src.contains(forbidden),
            "poll.rs must not contain {forbidden:?} — all shell-outs go through MpRunner"
        );
    }
}

#[test]
fn m217_ac06_poll_module_shells_out_only_through_the_enumerated_argv() {
    let src = poll_source();
    // The one and only fetch helper builds its commands from
    // `poll_argv` and debug-asserts the observer-only guard. If a
    // future edit adds a second, unaudited call site, this pins
    // the expectation that it must go through the same door.
    assert!(
        src.contains("for argv in poll_argv(session_id)"),
        "the fetch helper must iterate the enumerated argv list"
    );
    assert!(
        src.contains("is_observer_only_argv(&argv)"),
        "the fetch helper must assert the observer-only guard"
    );
    // The only other command the production path runs is the
    // config read used by the resolution chain; it is a read.
    let mutating = [
        "control\"",
        "\"pause\"",
        "\"resume\"",
        "\"cancel\"",
        "\"steer\"",
        "\"--detach\"",
    ];
    for token in mutating {
        assert!(
            !src.contains(&format!("run_raw_allow_failure(\"autopilot\", &[{token}")),
            "poll.rs must not invoke a control verb ({token})"
        );
    }
}

#[test]
fn m217_ac06_no_second_scheduler_relays_progress_to_the_orchestrator() {
    // M179's poller wrote back into the control plane's status
    // surface; the M217 cutover removed it. Nothing in the poll
    // module may reach for the control-plane write verbs.
    let src = poll_source();
    for forbidden in [
        "watch-control",
        "start_watch",
        "stop_watch",
        "poll_watch_state",
    ] {
        assert!(
            !src.contains(forbidden),
            "poll.rs must not reach the legacy control surface ({forbidden})"
        );
    }
}

// ─── outcome: raul cannot affect the drive ──────────────────────

#[test]
fn m217_ac06_polling_a_missing_mp_binary_never_errors() {
    // The idle hook must never take the TUI down, and a failed
    // read must never be mistaken for "the session ended".
    let runner = MpRunner::with_mp_bin("/nonexistent/mp/for/m217/ac06");
    let mut app = App::new();
    app.autopilot_poller.set_focused(true);
    for now in (0..20_000).step_by(2_000) {
        poll_autopilot_lane(&runner, &mut app, now);
    }
    assert!(app.autopilot_poller.fired_count() >= 10);
}

#[test]
fn m217_ac06_poller_holds_no_session_state_of_its_own() {
    // The poller's memory is the render snapshot and its own
    // schedule — nothing that could be mistaken for authoritative
    // session state. A fresh poller equals a polled-then-reset
    // poller in every scheduling respect.
    let mut p = AutopilotPoller::new();
    p.set_focused(true);
    p.begin(0);
    p.finish(0);
    assert!(
        p.last_snapshot().is_none(),
        "begin/finish alone must not fabricate a snapshot — only observe() records one"
    );
}

#[test]
fn m217_ac06_orchestration_outcome_is_unchanged_when_raul_never_polls() {
    // "raul is absent" == the poller is never focused. The
    // observable consequence must be exactly zero commands: no
    // reads, and therefore certainly no writes.
    let runner = MpRunner::with_mp_bin("/nonexistent/mp/for/m217/ac06");
    let mut app = App::new();
    let version_before = app.version();
    for now in (0..60_000).step_by(250) {
        // Never focused → the production hook would not even call
        // the poll function, but calling it anyway must be inert.
        poll_autopilot_lane(&runner, &mut app, now);
    }
    assert_eq!(app.autopilot_poller.fired_count(), 0);
    assert_eq!(
        app.version(),
        version_before,
        "an unfocused lane must not even redraw, let alone drive anything"
    );
}
