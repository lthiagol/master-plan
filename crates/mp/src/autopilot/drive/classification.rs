//! M178 S4 / AC-03: latest-run state classification (live / stale /
//! terminal).
//!
//! Three-class discriminator:
//!
//! - `Live`       — recorded PID is alive (or zombie but present)
//!   AND the v2 contract fields say the run is in flight
//!   (`run_outcome.is_none()`).
//! - `Stale`      — the recorded PID is gone but the run never
//!   reached a terminal outcome. A subsequent `mp watch --resume`
//!   can re-attach to the recorded panes / queue.
//! - `Terminal`   — the v2 state carries a `run_outcome`.
//!
//! The herdr-list probe is a secondary signal: a `Live` run whose
//! recorded panes no longer exist in herdr is `Stale` (the driver
//! crashed mid-stage). A `Stale` run whose panes reappear in herdr
//! remains `Stale` — reappearance alone does not promote a run back
//! to `Live`; the caller decides whether to resume via `--resume`.

use serde::Serialize;

use crate::autopilot::drive::AutopilotRunState;

/// Discriminated state of the latest recorded watch run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunState {
    /// Recorded PID is alive and the run is in flight.
    Live,
    /// Recorded PID is gone (or panes are gone from herdr) but the
    /// run never reached a terminal outcome. Resumable.
    Stale { reason: String },
    /// The v2 state carries a `run_outcome`. Run finished.
    Terminal,
}

/// Structured status report consumed by `mp watch-control status`.
/// Carries the classification, the resolved file path, the PID
/// liveness probe, and the full v2 state (None when no state file
/// exists yet).
#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub run_state: RunState,
    pub state_file: String,
    pub schema_version: u32,
    /// The full v2 state. `None` when no state file exists yet.
    pub state: Option<AutopilotRunState>,
    /// Probe of the recorded PID via `kill(pid, 0)`. False on non-Unix
    /// or when no state is present.
    pub pid_alive: bool,
    /// True when a herdr-list payload was successfully fetched for
    /// the secondary classification signal. False when herdr is
    /// missing or returned non-zero — the classifier falls back to
    /// the PID probe alone in that case.
    pub herdr_listed: bool,
}

/// Classify the latest state into live/stale/terminal. The herdr
/// list is optional; pass `None` when herdr is unavailable and the
/// PID probe alone drives the verdict.
pub fn classify_state(
    state: Option<&AutopilotRunState>,
    herdr_list_json: Option<&str>,
) -> RunState {
    let Some(state) = state else {
        // No state file → no recorded run. The caller reports this
        // as `null` (or treats it as terminal-idle); we map it to
        // Stale with reason="no state file" so the status surface
        // doesn't have to special-case the empty case.
        return RunState::Stale {
            reason: "no state file".to_string(),
        };
    };

    // Terminal wins: a recorded run_outcome is the authoritative
    // signal that the driver wrote its terminal exit.
    if state.run_outcome.is_some() {
        return RunState::Terminal;
    }

    let pid_alive = crate::autopilot::drive::is_pid_alive(state.pid);
    if !pid_alive {
        return RunState::Stale {
            reason: format!("recorded pid {} not alive", state.pid),
        };
    }

    // PID alive but panes gone from herdr → stale.
    if let Some(json) = herdr_list_json {
        if !panes_present(&state.pane_ids, json) {
            return RunState::Stale {
                reason: "recorded panes missing from herdr".to_string(),
            };
        }
    }

    RunState::Live
}

/// True when every recorded pane_id appears in the herdr-list
/// JSON. A `herdr agent list` envelope (`{"agents": [...]}`) is the
/// common shape; bare `[...]` is tolerated.
fn panes_present(
    pane_ids: &std::collections::HashMap<crate::autopilot::drive::Role, String>,
    herdr_list_json: &str,
) -> bool {
    if pane_ids.is_empty() {
        return true;
    }
    let parsed: serde_json::Value = match serde_json::from_str(herdr_list_json) {
        Ok(v) => v,
        Err(_) => return true, // can't parse → assume live; the operator will see other signals
    };
    let agents = parsed
        .get("agents")
        .and_then(|v| v.as_array())
        .or_else(|| parsed.as_array());
    let Some(agents) = agents else {
        return true;
    };
    pane_ids.values().all(|id| {
        agents.iter().any(|a| {
            a.get("pane_id").and_then(|v| v.as_str()) == Some(id.as_str())
                || a.get("target").and_then(|v| v.as_str()) == Some(id.as_str())
                || a.get("id").and_then(|v| v.as_str()) == Some(id.as_str())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::drive::{Role, RunOutcome};
    use std::collections::HashMap;

    fn empty_state() -> AutopilotRunState {
        AutopilotRunState::fresh(&["170".to_string()])
    }

    #[test]
    fn no_state_is_stale_with_reason() {
        let r = classify_state(None, None);
        match r {
            RunState::Stale { reason } => assert_eq!(reason, "no state file"),
            _ => panic!("expected Stale, got {r:?}"),
        }
    }

    #[test]
    fn state_with_terminal_outcome_is_terminal() {
        let mut s = empty_state();
        s.set_run_outcome(RunOutcome::Completed);
        let r = classify_state(Some(&s), None);
        assert_eq!(r, RunState::Terminal);
    }

    #[test]
    fn state_with_dead_pid_is_stale() {
        // Use a PID that's almost certainly not alive. PID 1 is
        // owned by init on Linux and macOS but our classification
        // would call it "alive" via EPERM; pick a high PID that
        // almost certainly doesn't exist.
        let mut s = empty_state();
        s.pid = 999_999_999;
        s.pane_ids.insert(Role::Runner, "%5".to_string());
        let r = classify_state(Some(&s), None);
        match r {
            RunState::Stale { reason } => {
                assert!(reason.contains("999999999"), "reason={reason}");
            }
            _ => panic!("expected Stale, got {r:?}"),
        }
    }

    #[test]
    fn live_state_with_alive_pid_and_panes_present_is_live() {
        let mut s = empty_state();
        s.pid = std::process::id();
        let mut panes = HashMap::new();
        panes.insert(Role::Runner, "%5".to_string());
        s.pane_ids = panes;
        let herdr_json = r#"{"agents":[{"name":"role-runner-1","pane_id":"%5"}]}"#;
        let r = classify_state(Some(&s), Some(herdr_json));
        assert_eq!(r, RunState::Live);
    }

    #[test]
    fn live_pid_but_panes_missing_from_herdr_is_stale() {
        let mut s = empty_state();
        s.pid = std::process::id();
        let mut panes = HashMap::new();
        panes.insert(Role::Runner, "%5".to_string());
        panes.insert(Role::Coordinator, "%7".to_string());
        s.pane_ids = panes;
        // Only one pane appears in the live list.
        let herdr_json = r#"{"agents":[{"name":"role-runner-1","pane_id":"%5"}]}"#;
        let r = classify_state(Some(&s), Some(herdr_json));
        match r {
            RunState::Stale { reason } => assert!(reason.contains("missing")),
            _ => panic!("expected Stale, got {r:?}"),
        }
    }

    #[test]
    fn empty_pane_ids_is_not_a_stale_signal() {
        // The fresh state has no pane ids yet (the driver records
        // them on first ensure_pane). Until the first pane is
        // spawned, a "live" pid is enough to call the run live.
        let mut s = empty_state();
        s.pid = std::process::id();
        let r = classify_state(Some(&s), Some(r#"{"agents":[]}"#));
        assert_eq!(r, RunState::Live);
    }
}
