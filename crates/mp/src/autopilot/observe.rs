//! M246 WP3: operator observation verbs — `mp autopilot wait` and
//! `mp autopilot tail`.
//!
//! Before this module the only way to follow a run was to poll
//! `activity.json` by hand or re-run `mp autopilot status` in a loop.
//! Two verbs make that scriptable:
//!
//! - **wait** — block until a milestone reaches the run's target
//!   lifecycle (or `complete`), with a bounded timeout and a typed
//!   failure reason.
//! - **tail** — print the journal lines scoped to one milestone,
//!   oldest first, optionally following new ones until the run goes
//!   quiet.
//!
//! Both are built on [`std::time::Instant`] seams (`*_with`) so the
//! timing behaviour — timeout, idle-exit, poll cadence — is testable
//! without real sleeps. The wall-clock sleeps live only in the
//! production wrappers.
//!
//! The readers are injected as closures for the same reason: the
//! run-state file, the plan, and the journal are three different
//! on-disk sources, and a test needs to drive all of them
//! independently (including the failure paths that a real run
//! rarely produces on demand).

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::autopilot::drive::AutopilotRunState;

/// Default `--timeout` for [`wait_for_milestone`], in seconds.
pub const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 1800;

/// Default `--idle` for `tail --follow`, in seconds. A follow exits
/// after this long with no new matching event.
pub const DEFAULT_TAIL_IDLE_SECS: u64 = 5;

/// Lifecycle that counts as success regardless of the recorded
/// target: a milestone that is done is done.
const LIFECYCLE_COMPLETE: &str = "complete";

/// Why a [`wait_for_milestone`] poll loop gave up. Serialized
/// kebab-case on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaitReason {
    /// The timeout elapsed before the milestone reached the target.
    Timeout,
    /// The run is no longer driving this milestone: no state file,
    /// a dead PID, or the run moved on without reaching the target.
    RunStopped,
    /// The run recorded a terminal outcome that is not success
    /// (failed, skipped, spawn-failed, exhausted).
    RunFailed,
}

/// Result of a completed `mp autopilot wait`.
///
/// `reached` drives the exit code: 0 when true, 1 when false.
#[derive(Debug, Clone, Serialize)]
pub struct WaitReport {
    pub reached: bool,
    /// The lifecycle observed on the final poll, when one could be
    /// read. `None` when the milestone could not be resolved at all.
    pub lifecycle: Option<String>,
    /// The lifecycle the run was waiting for, when recorded.
    pub target_lifecycle: Option<String>,
    /// Why the wait stopped. `None` when `reached` is true.
    pub reason: Option<WaitReason>,
    /// Milliseconds spent waiting, for operator telemetry.
    pub elapsed_ms: u64,
}

/// True when `lifecycle` satisfies the wait: it equals the recorded
/// target, or it is `complete` (a finished milestone is a finished
/// wait, even if the recorded target was an earlier stage).
pub fn lifecycle_satisfies(lifecycle: &str, target: Option<&str>) -> bool {
    lifecycle == LIFECYCLE_COMPLETE || target.is_some_and(|t| lifecycle == t)
}

/// The lifecycle a run is waiting for on `milestone_id`.
///
/// Only the milestone the run is *actively* driving has a recorded
/// stage target; for anything else `complete` is the only sensible
/// success condition, so that is what we wait for.
pub fn target_for(state: Option<&AutopilotRunState>, milestone_id: &str) -> Option<String> {
    let state = state?;
    if state.active_milestone.as_deref() != Some(milestone_id) {
        return Some(LIFECYCLE_COMPLETE.to_string());
    }
    state
        .target_lifecycle
        .clone()
        .or_else(|| Some(LIFECYCLE_COMPLETE.to_string()))
}

/// Classification of one poll sample: keep waiting, or stop with a
/// reason.
#[derive(Debug, PartialEq, Eq)]
enum PollVerdict {
    Keep,
    Stop(WaitReason),
}

/// Decide whether a single sample ends the wait.
///
/// `target` is the lifecycle the run is waiting for. A run whose
/// driver has exited without a success outcome has stopped; one that
/// recorded a failure outcome has failed. Both are terminal even if
/// the milestone happens to sit at a matching lifecycle — a failed
/// run that reports the target lifecycle is still a failed run, and
/// saying otherwise would let a caller treat a dead run as success.
fn classify_sample(
    state: Option<&AutopilotRunState>,
    lifecycle: Option<&str>,
    target: Option<&str>,
) -> PollVerdict {
    // Reached wins over every failure signal: if the milestone is at
    // the target (or complete), the wait is satisfied regardless of
    // what the run recorded afterwards.
    if let Some(lc) = lifecycle {
        if lifecycle_satisfies(lc, target) {
            return PollVerdict::Keep; // caller checks satisfaction first
        }
    }

    let Some(state) = state else {
        return PollVerdict::Stop(WaitReason::RunStopped);
    };

    if let Some(outcome) = &state.run_outcome {
        return match outcome.label() {
            // A completed run will not move the milestone any
            // further. If the milestone is not at the target the
            // satisfaction check above did not pass, so the run
            // finished without this milestone reaching the goal —
            // terminal, and reported as stopped rather than burning
            // the whole timeout.
            "completed" => PollVerdict::Stop(WaitReason::RunStopped),
            _ => PollVerdict::Stop(WaitReason::RunFailed),
        };
    }

    // No outcome yet, but the driver is gone.
    if !crate::autopilot::drive::is_pid_alive(state.pid) {
        return PollVerdict::Stop(WaitReason::RunStopped);
    }

    // The run is alive but no longer driving this milestone and the
    // milestone has not reached the target: the run moved on.
    if state.active_milestone.is_some()
        && state.active_milestone.as_deref() != state.queue.first().map(|s| s.as_str())
    {
        // Not the queue head: this milestone is not being worked on.
        return PollVerdict::Stop(WaitReason::RunStopped);
    }

    PollVerdict::Keep
}

/// Block until `milestone_id` reaches its target lifecycle.
///
/// `read_state` loads `.mp/autopilot-run.state.json` (`Ok(None)`
/// when absent); `read_lifecycle` returns the milestone's current
/// lifecycle. Both are called on every poll. `now` is injected so
/// tests can drive the timeout deterministically.
///
/// The returned report carries `reached`; the caller maps that to the
/// process exit code.
pub fn wait_for_milestone_with<S, L, N>(
    mut read_state: S,
    mut read_lifecycle: L,
    milestone_id: &str,
    timeout: Duration,
    mut now: N,
) -> WaitReport
where
    S: FnMut() -> Option<AutopilotRunState>,
    L: FnMut() -> Option<String>,
    N: FnMut() -> Instant,
{
    let start = now();
    let mut last_lifecycle: Option<String> = None;

    loop {
        let state = read_state();
        let lifecycle = read_lifecycle();
        let target = target_for(state.as_ref(), milestone_id);
        if let Some(lc) = &lifecycle {
            last_lifecycle = Some(lc.clone());
        }

        if let Some(lc) = &lifecycle {
            if lifecycle_satisfies(lc, target.as_deref()) {
                return WaitReport {
                    reached: true,
                    lifecycle: Some(lc.clone()),
                    target_lifecycle: target,
                    reason: None,
                    elapsed_ms: elapsed_ms(&start, &mut now),
                };
            }
        }

        match classify_sample(state.as_ref(), lifecycle.as_deref(), target.as_deref()) {
            PollVerdict::Stop(reason) => {
                return WaitReport {
                    reached: false,
                    lifecycle: last_lifecycle,
                    target_lifecycle: target,
                    reason: Some(reason),
                    elapsed_ms: elapsed_ms(&start, &mut now),
                };
            }
            PollVerdict::Keep => {}
        }

        if now().saturating_duration_since(start) >= timeout {
            return WaitReport {
                reached: false,
                lifecycle: last_lifecycle,
                target_lifecycle: target,
                reason: Some(WaitReason::Timeout),
                elapsed_ms: elapsed_ms(&start, &mut now),
            };
        }
    }
}

fn elapsed_ms(start: &Instant, now: &mut impl FnMut() -> Instant) -> u64 {
    now().saturating_duration_since(*start).as_millis() as u64
}

/// How a `tail` invocation should end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailStop {
    /// `--follow` was not requested: the journal is exhausted.
    Eof,
    /// `--follow` was requested and no new event arrived within
    /// `--idle`.
    Idle,
}

/// A tail cursor: how far into the matching-event list we have
/// already printed. Index-based rather than timestamp-based so a
/// journal that grows between polls cannot re-print or skip an
/// event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TailCursor {
    emitted: usize,
}

impl TailCursor {
    pub fn emitted(&self) -> usize {
        self.emitted
    }
}

/// The events a `tail` invocation should print next.
#[derive(Debug, Clone, PartialEq)]
pub struct TailBatch {
    pub events: Vec<crate::activity::ActivityEvent>,
    pub stop: Option<TailStop>,
}

/// One `tail` poll: filter the journal to `subject == id`, honour
/// `since`, and emit everything the cursor has not printed yet.
///
/// `cursor` advances in place, so a `--follow` loop can call this
/// repeatedly and only ever see new events.
pub fn tail_batch(
    events: &[crate::activity::ActivityEvent],
    id: &str,
    since: Option<&str>,
    cursor: &mut TailCursor,
    following: bool,
    idle_budget: Duration,
    elapsed_since_last: Duration,
) -> TailBatch {
    let matching: Vec<&crate::activity::ActivityEvent> = events
        .iter()
        .filter(|e| e.subject == id)
        .filter(|e| match since {
            // ISO-8601 timestamps with the same offset sort
            // lexicographically; a malformed `--since` simply
            // matches nothing extra rather than erroring mid-stream.
            Some(s) => e.timestamp.as_str() > s,
            None => true,
        })
        .collect();

    let new_events: Vec<crate::activity::ActivityEvent> = matching
        .iter()
        .skip(cursor.emitted)
        .map(|e| (*e).clone())
        .collect();

    if !new_events.is_empty() {
        cursor.emitted = matching.len();
        return TailBatch {
            events: new_events,
            stop: None,
        };
    }

    TailBatch {
        events: Vec::new(),
        stop: if !following {
            // Journal drained and nobody asked us to keep watching.
            Some(TailStop::Eof)
        } else if elapsed_since_last >= idle_budget {
            Some(TailStop::Idle)
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::ActivityEvent;
    use crate::autopilot::drive::RunOutcome;

    fn ev(ts: &str, subject: &str) -> ActivityEvent {
        ActivityEvent {
            timestamp: ts.to_string(),
            r#type: "lifecycle-transition".to_string(),
            subject: subject.to_string(),
            summary: "s".to_string(),
            data: None,
        }
    }

    #[test]
    fn lifecycle_satisfies_on_target_or_complete() {
        assert!(lifecycle_satisfies("complete", Some("self-reviewed")));
        assert!(lifecycle_satisfies("self-reviewed", Some("self-reviewed")));
        assert!(!lifecycle_satisfies("in-progress", Some("self-reviewed")));
        assert!(!lifecycle_satisfies("in-progress", None));
    }

    #[test]
    fn target_for_non_active_milestone_is_complete() {
        let mut s = AutopilotRunState::fresh(&["7".to_string()]);
        s.active_milestone = Some("9".to_string());
        s.target_lifecycle = Some("in-progress".to_string());
        assert_eq!(target_for(Some(&s), "7").as_deref(), Some("complete"));
        assert_eq!(target_for(Some(&s), "9").as_deref(), Some("in-progress"));
    }

    #[test]
    fn tail_batch_filters_by_subject_and_advances_cursor() {
        let events = vec![
            ev("2026-01-01T00:00:00+00:00", "7"),
            ev("2026-01-01T00:00:01+00:00", "8"),
            ev("2026-01-01T00:00:02+00:00", "7"),
        ];
        let mut cursor = TailCursor::default();
        let first = tail_batch(
            &events,
            "7",
            None,
            &mut cursor,
            false,
            Duration::ZERO,
            Duration::ZERO,
        );
        assert_eq!(first.events.len(), 2);
        assert_eq!(first.stop, None);
        // Re-polling with the same journal yields nothing new.
        let second = tail_batch(
            &events,
            "7",
            None,
            &mut cursor,
            false,
            Duration::ZERO,
            Duration::ZERO,
        );
        assert!(second.events.is_empty());
        // A new event appears.
        let mut grown = events.clone();
        grown.push(ev("2026-01-01T00:00:03+00:00", "7"));
        let third = tail_batch(
            &grown,
            "7",
            None,
            &mut cursor,
            false,
            Duration::ZERO,
            Duration::ZERO,
        );
        assert_eq!(third.events.len(), 1);
        assert_eq!(third.events[0].summary, "s");
    }

    #[test]
    fn tail_batch_respects_since() {
        let events = vec![
            ev("2026-01-01T00:00:00+00:00", "7"),
            ev("2026-01-01T00:01:00+00:00", "7"),
        ];
        let mut cursor = TailCursor::default();
        let b = tail_batch(
            &events,
            "7",
            Some("2026-01-01T00:00:30+00:00"),
            &mut cursor,
            false,
            Duration::ZERO,
            Duration::ZERO,
        );
        assert_eq!(b.events.len(), 1);
        assert_eq!(b.events[0].timestamp, "2026-01-01T00:01:00+00:00");
    }

    #[test]
    fn tail_batch_stops_on_idle_when_following() {
        let mut cursor = TailCursor::default();
        let b = tail_batch(
            &[],
            "7",
            None,
            &mut cursor,
            true,
            Duration::from_secs(5),
            Duration::from_secs(6),
        );
        assert_eq!(b.stop, Some(TailStop::Idle));
        // Not following: EOF, never idle-exit.
        let b2 = tail_batch(
            &[],
            "7",
            None,
            &mut cursor,
            false,
            Duration::from_secs(5),
            Duration::from_secs(600),
        );
        assert_eq!(b2.stop, Some(TailStop::Eof));
    }

    #[test]
    fn classify_sample_fails_on_non_success_outcome() {
        let mut s = AutopilotRunState::fresh(&["7".to_string()]);
        s.pid = std::process::id();
        s.set_run_outcome(RunOutcome::PartialFailure);
        assert_eq!(
            classify_sample(Some(&s), Some("in-progress"), Some("complete")),
            PollVerdict::Stop(WaitReason::RunFailed)
        );
    }

    #[test]
    fn classify_sample_stops_when_state_is_absent() {
        assert_eq!(
            classify_sample(None, Some("in-progress"), Some("complete")),
            PollVerdict::Stop(WaitReason::RunStopped)
        );
    }
}
