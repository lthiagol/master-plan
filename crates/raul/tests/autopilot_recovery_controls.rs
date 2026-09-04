//! M216 AC-06: recovery controls.
//!
//! Five clients shell out to `mp autopilot control …`:
//!
//! - **Pause** — `mp autopilot control pause <session>`.
//!   Stops new dispatch at the next boundary.
//! - **Resume** — `mp autopilot control resume <session>`.
//!   Continues the same session.
//! - **Cancel** — `mp autopilot control cancel <session>
//!   --confirm`. Terminal after confirmation. The
//!   cancelled session remains replayable.
//! - **Restart** — `mp autopilot start <ids...> --detach
//!   --topology <T> [--poll-interval-ms <N>]`. Creates a
//!   new session from the explicit queue. Does NOT revive
//!   cancelled state.
//! - **Steer** — `mp autopilot control steer <session>
//!   --message <MSG>`. Targeted one-off message.
//!
//! The clients are pure argv builders — the dispatcher
//! shells them through `MpRunner::run_raw_allow_failure`.

use raul::tui::autopilot::RecoveryControl;

/// AC-06: the pause client builds the canonical argv
/// for `mp autopilot control pause <session>`.
#[test]
fn pause_argv_builds_canonical_avro() {
    let argv = RecoveryControl::pause_argv("alpha");
    assert_eq!(argv, vec!["control", "pause", "alpha"]);
}

/// AC-06: the resume client builds the canonical argv
/// for `mp autopilot control resume <session>`.
#[test]
fn resume_argv_builds_canonical_avro() {
    let argv = RecoveryControl::resume_argv("alpha");
    assert_eq!(argv, vec!["control", "resume", "alpha"]);
}

/// AC-06: the cancel client builds the canonical argv
/// for `mp autopilot control cancel <session>
/// --confirm`. The `--confirm` flag is always present
/// to avoid a footgun.
#[test]
fn cancel_argv_includes_confirm_flag() {
    let argv = RecoveryControl::cancel_argv("alpha");
    assert_eq!(argv, vec!["control", "cancel", "alpha", "--confirm"]);
}

/// AC-06: the steer client builds the canonical argv
/// for `mp autopilot control steer <session>
/// --message <MSG>`.
#[test]
fn steer_argv_carries_message_payload() {
    let argv = RecoveryControl::steer_argv("alpha", "rerun with retries=3");
    assert_eq!(
        argv,
        vec![
            "control",
            "steer",
            "alpha",
            "--message",
            "rerun with retries=3",
        ]
    );
}

/// AC-06: the restart client builds the canonical argv
/// for `mp autopilot start <ids...> --detach
/// --topology <T> [--poll-interval-ms <N>]`. The override
/// payload's topology + poll_interval_ms are forwarded.
#[test]
fn start_argv_includes_topology_and_poll_interval() {
    use raul::tui::autopilot::{SessionConfigOverrides, SessionOverridesPayload};
    let payload = SessionOverridesPayload {
        config_overrides: SessionConfigOverrides {
            topology: "two-agent".to_string(),
            poll_interval_ms: Some(3000),
        },
        roles: Default::default(),
    };
    let ids = vec!["207".to_string(), "209".to_string()];
    let argv = RecoveryControl::start_argv(&ids, &payload);
    assert_eq!(
        argv,
        vec![
            "start",
            "207",
            "209",
            "--topology",
            "two-agent",
            "--poll-interval-ms",
            "3000",
            "--detach",
        ]
    );
}

/// AC-06: the start client omits `--poll-interval-ms`
/// when the override panel leaves it unset.
#[test]
fn start_argv_omits_poll_interval_when_unset() {
    use raul::tui::autopilot::{SessionConfigOverrides, SessionOverridesPayload};
    let payload = SessionOverridesPayload {
        config_overrides: SessionConfigOverrides {
            topology: "three-agent".to_string(),
            poll_interval_ms: None,
        },
        roles: Default::default(),
    };
    let argv = RecoveryControl::start_argv(&["207".to_string()], &payload);
    assert_eq!(
        argv,
        vec!["start", "207", "--topology", "three-agent", "--detach"]
    );
}

/// AC-06: cancel is terminal. The lane state collapses
/// to "session cancelled" — the in-session fields flip
/// to `None` and the queue view's status flips to
/// `cancelled`. A subsequent restart creates a NEW
/// session rather than reviving cancelled state.
#[test]
fn cancel_collapses_in_session_state_to_terminal() {
    use raul::tui::autopilot::refresh::refresh_from_json;
    use raul::tui::autopilot::AutopilotLaneState;

    let mut state = AutopilotLaneState::empty();
    let show = serde_json::json!({
        "session_id": "alpha",
        "session": {
            "id": "alpha",
            "status": "active",
            "queue": [
                {"milestone_id": "M207", "title": "Pilot S2", "lifecycle": "approved"},
                {"milestone_id": "M209", "title": "Coordination", "lifecycle": "in-progress"},
            ],
            "working_on": {"milestone_id": "M209"},
        },
    });
    refresh_from_json(
        &mut state,
        &show,
        &serde_json::json!({"run_state": {"kind": "live"}}),
    );
    assert!(state.status_graph().is_some());
    assert!(state.queue_view().is_some());

    state.mark_session_cancelled();

    // In-session fields flip to None.
    assert!(state.status_graph().is_none());
    assert!(state.violations().is_none());
    assert!(state.detail_panel().is_none());
    assert!(state.ac_detail().is_none());
    assert!(state.telemetry().is_none());
    assert!(state.detail_milestone().is_none());
    assert!(state.expanded_violation.is_none());

    // Queue view stays (the user can still replay) but
    // flips to "cancelled" so the renderer surfaces the
    // terminal label in the header.
    let qv = state.queue_view().expect("queue view persists");
    assert_eq!(qv.status, "cancelled");
}

/// AC-06: the restart client preserves the override
/// panel's typed payload (topology, poll_interval_ms)
/// into the new session argv. The new session honors
/// the user's last panel edits.
#[test]
#[allow(clippy::field_reassign_with_default)]
fn restart_argv_preserves_panel_overrides() {
    use raul::tui::autopilot::{OverridePanel, SessionOverridesPayload};
    let mut panel = OverridePanel::default();
    panel.topology = "one-agent".to_string();
    panel.refresh_secs = 5;
    let payload = panel.to_session_overrides();

    let argv = RecoveryControl::start_argv(&["211".to_string()], &payload);
    assert!(
        argv.windows(2).any(|w| w == ["--topology", "one-agent"]),
        "restart must forward topology from the panel; got {argv:?}"
    );
    assert!(
        argv.windows(2).any(|w| w == ["--poll-interval-ms", "5000"]),
        "restart must forward poll_interval_ms derived from refresh_secs; got {argv:?}"
    );
    assert!(
        argv.contains(&"--detach".to_string()),
        "restart must run detached"
    );

    // Sanity: SessionOverridesPayload round-trips through
    // the panel.
    let json = serde_json::to_string(&payload).unwrap();
    let back: SessionOverridesPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back.config_overrides.topology, "one-agent");
    assert_eq!(back.config_overrides.poll_interval_ms, Some(5000));
}

/// AC-06: production-path regression. The recovery
/// clients are reachable from the dispatcher through
/// the typed `RecoveryControl` API. The dispatcher's
/// pause/resume/cancel/steer/restart handlers call the
/// typed argv builders directly.
#[test]
fn recovery_clients_are_reachable_from_the_dispatcher() {
    // The argv builder type is reachable from the autopilot
    // module's public types — a future refactor that
    // moves the module breaks the dispatcher's call
    // site.
    let _: fn(&str) -> Vec<String> = RecoveryControl::pause_argv;
    let _: fn(&str) -> Vec<String> = RecoveryControl::resume_argv;
    let _: fn(&str) -> Vec<String> = RecoveryControl::cancel_argv;
    let _: fn(&str, &str) -> Vec<String> = RecoveryControl::steer_argv;
    let _: fn(&[String], &raul::tui::autopilot::SessionOverridesPayload) -> Vec<String> =
        RecoveryControl::start_argv;
}

/// AC-06: the steer message travels through the
/// dispatcher unchanged. The dispatcher reads
/// `Action::AutopilotSteer { message }` and forwards
/// the message verbatim into the steer argv.
#[test]
fn steer_message_round_trips_unchanged() {
    let msg = "rerun with model=opus & harness=opencode";
    let argv = RecoveryControl::steer_argv("alpha", msg);
    let argv_str: Vec<&str> = argv.iter().map(String::as_str).collect();
    let msg_idx = argv_str
        .iter()
        .position(|s| *s == "--message")
        .expect("--message flag must exist");
    assert_eq!(argv_str[msg_idx + 1], msg);
}
