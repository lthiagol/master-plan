//! M213 / AC-06: heartbeat acknowledgements and stale-state timeout.
//!
//! The headless engine uses explicit heartbeat acknowledgements
//! and state timestamps. Per spec:
//!
//! - A responsive lane may remain in the same workflow state
//!   without timing out (heartbeat_missed AND state_changed
//!   must BOTH be true to fire StaleStateTimeout).
//! - Only missed heartbeat + stale state produces the
//!   `StaleStateTimeout` outcome.
//! - When the lane's current state matches the
//!   last-acknowledged state, the heartbeat is "in sync" —
//!   the timeout is suppressed even after the window expires.
//! - When the heartbeat is fresh (now - last_ack <= timeout)
//!   but the state changed, the timeout is also suppressed —
//!   the lane ack'd after the state move.

use mp::autopilot::cycle::{
    classify_liveness, HeartbeatAckState, HeartbeatTracker, LivenessStatus,
};
use mp::autopilot::session::RoleName;

fn tracker(
    lane: RoleName,
    last_ack: &str,
    last_acked_state: HeartbeatAckState,
    current: HeartbeatAckState,
    state_change: &str,
) -> HeartbeatTracker {
    HeartbeatTracker {
        lane,
        last_ack_at: last_ack.into(),
        last_acknowledged_state: last_acked_state,
        current_state: current,
        state_changed_at: state_change.into(),
    }
}

#[test]
fn responsive_lane_in_same_state_does_not_timeout() {
    // AC-06 headline rule: a responsive lane may stay in the
    // same state indefinitely without timing out.
    let t = tracker(
        RoleName::Runner,
        "2026-01-01T00:00:00Z",
        HeartbeatAckState::Working,
        HeartbeatAckState::Working,
        "2026-01-01T00:00:00Z",
    );
    // Even at far-future now, no timeout because state is in
    // sync.
    let status = classify_liveness(&t, u64::MAX, 1000);
    assert!(matches!(status, LivenessStatus::Healthy));
}

#[test]
fn missed_heartbeat_with_state_change_times_out() {
    // Both conditions fire:
    //   (a) heartbeat missed (60s > 1s)
    //   (b) state changed (Working -> Blocked)
    // -> StaleStateTimeout.
    let t = tracker(
        RoleName::Runner,
        "2026-01-01T00:00:00Z",
        HeartbeatAckState::Working,
        HeartbeatAckState::Blocked,
        "2026-01-01T00:00:30Z",
    );
    let now_ms = parse_ts("2026-01-01T00:01:00Z").unwrap();
    let status = classify_liveness(&t, now_ms, 1000);
    assert!(status.is_stale());
}

#[test]
fn fresh_ack_after_state_change_is_healthy() {
    // Lane acked recently even though the state changed —
    // the ack covers the new state, so no timeout.
    let t = tracker(
        RoleName::Reviewer,
        "2026-01-01T00:00:30Z",
        HeartbeatAckState::Working,
        HeartbeatAckState::Blocked,
        "2026-01-01T00:00:30Z",
    );
    // now = ack + 500ms (still within the 1s window)
    let ack_ms = parse_ts("2026-01-01T00:00:30Z").unwrap();
    let now_ms = ack_ms + 500;
    let status = classify_liveness(&t, now_ms, 1000);
    assert!(matches!(status, LivenessStatus::Healthy));
}

#[test]
fn missed_heartbeat_with_state_in_sync_is_healthy() {
    // Heartbeat missed, but the state is in sync — a quiet
    // lane that has been sitting in Working for an hour.
    // AC-06 says: do not fire StaleStateTimeout.
    let t = tracker(
        RoleName::Runner,
        "2026-01-01T00:00:00Z",
        HeartbeatAckState::Working,
        HeartbeatAckState::Working,
        "2026-01-01T00:00:00Z",
    );
    let now_ms = parse_ts("2026-01-01T00:05:00Z").unwrap();
    let status = classify_liveness(&t, now_ms, 1000);
    assert!(matches!(status, LivenessStatus::Healthy));
}

#[test]
fn state_change_without_missed_heartbeat_is_healthy() {
    // State changed recently AND the heartbeat is fresh —
    // the lane acked after the state change.
    let t = tracker(
        RoleName::Orchestrator,
        "2026-01-01T00:01:00Z",
        HeartbeatAckState::Working,
        HeartbeatAckState::Done,
        "2026-01-01T00:01:00Z",
    );
    let ack_ms = parse_ts("2026-01-01T00:01:00Z").unwrap();
    let now_ms = ack_ms + 100; // 100ms after ack, well under 1s
    let status = classify_liveness(&t, now_ms, 1000);
    assert!(matches!(status, LivenessStatus::Healthy));
}

#[test]
fn stale_state_timeout_carries_lane_and_timestamps() {
    let t = tracker(
        RoleName::Runner,
        "2026-01-01T00:00:00Z",
        HeartbeatAckState::Working,
        HeartbeatAckState::Blocked,
        "2026-01-01T00:00:30Z",
    );
    let now_ms = parse_ts("2026-01-01T00:01:00Z").unwrap();
    let status = classify_liveness(&t, now_ms, 1000);
    match status {
        LivenessStatus::StaleStateTimeout {
            lane,
            last_ack_at,
            state_change_at,
        } => {
            assert_eq!(lane, RoleName::Runner);
            assert_eq!(last_ack_at, "2026-01-01T00:00:00Z");
            assert_eq!(state_change_at, "2026-01-01T00:00:30Z");
        }
        other => panic!("expected StaleStateTimeout, got {other:?}"),
    }
}

#[test]
fn no_prior_ack_treated_as_missed_when_state_changed() {
    // A lane with an UNPARSEABLE last_ack_at (no prior ack)
    // AND a state change -> fire immediately. The function
    // treats parse failure as "no prior ack" via the None
    // arm of `parse_rfc3339_ms`.
    let t = HeartbeatTracker {
        lane: RoleName::Runner,
        // Unparseable: heartbeat never landed.
        last_ack_at: "not-a-timestamp".into(),
        last_acknowledged_state: HeartbeatAckState::Idle,
        current_state: HeartbeatAckState::Working,
        state_changed_at: "2026-01-01T00:00:00Z".into(),
    };
    let now_ms = parse_ts("2026-01-01T00:00:30Z").unwrap();
    let status = classify_liveness(&t, now_ms, 1000);
    assert!(status.is_stale());
}

/// Parse a timestamp into epoch milliseconds — same shape as
/// the unit tests in cycle.rs, repeated here so the integration
/// suite is self-contained.
fn parse_ts(s: &str) -> Option<u64> {
    if s.len() < 19 {
        return None;
    }
    let y: i32 = s.get(0..4)?.parse().ok()?;
    let mo: u32 = s.get(5..7)?.parse().ok()?;
    let d: u32 = s.get(8..10)?.parse().ok()?;
    let h: u32 = s.get(11..13)?.parse().ok()?;
    let mi: u32 = s.get(14..16)?.parse().ok()?;
    let se: u32 = s.get(17..19)?.parse().ok()?;
    let secs = days_from_civil(y, mo, d) * 86400 + (h as u64) * 3600 + (mi as u64) * 60 + se as u64;
    Some(secs * 1000)
}

fn days_from_civil(y: i32, m: u32, d: u32) -> u64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era as i64 * 146097 + doe as i64 - 719468).max(0) as u64
}
