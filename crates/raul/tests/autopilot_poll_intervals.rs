//! M217 / AC-01 — coalescing single-flight poll intervals.
//!
//! The poller's clock is injected (`now_ms`), so every assertion
//! here is deterministic and nothing sleeps. Two behaviours are
//! pinned:
//!
//! * **Cadence** — one request per interval, no more.
//! * **Overrun** — a request that outlives its interval is never
//!   overlapped. Ticks that arrive while it is outstanding are
//!   *coalesced* (counted and dropped), and exactly one follow-up
//!   request fires once it completes, regardless of how many ticks
//!   were dropped.

use raul::tui::poll::{AutopilotPoller, PollDecision, DEFAULT_REFRESH_SECS};

/// A focused poller with the default 2s cadence, ready to fire.
fn focused() -> AutopilotPoller {
    let mut p = AutopilotPoller::new();
    p.set_focused(true);
    p
}

/// Fire one complete request/response round at `now_ms`.
fn round_trip(p: &mut AutopilotPoller, now_ms: u64) -> PollDecision {
    let decision = p.begin(now_ms);
    if decision.should_fire() {
        p.finish(now_ms);
    }
    decision
}

#[test]
fn m217_ac01_default_cadence_is_two_seconds() {
    let p = AutopilotPoller::new();
    assert_eq!(p.refresh_secs(), DEFAULT_REFRESH_SECS);
    assert_eq!(p.interval_ms(), 2_000);
}

#[test]
fn m217_ac01_first_tick_after_focus_fires_immediately() {
    let mut p = focused();
    assert_eq!(
        p.begin(0),
        PollDecision::Fire,
        "lane entry must produce the initial snapshot without waiting an interval"
    );
}

#[test]
fn m217_ac01_normal_interval_fires_once_per_period() {
    let mut p = focused();
    // Tick every 250ms for 10 simulated seconds against a 2s
    // cadence: 0, 2000, 4000, 6000, 8000, 10000 → 6 requests.
    for now in (0..=10_000).step_by(250) {
        round_trip(&mut p, now);
    }
    assert_eq!(
        p.fired_count(),
        6,
        "a 2s cadence over 10s of 250ms ticks must fire 6 times, not once per tick"
    );
    assert_eq!(
        p.coalesced_count(),
        0,
        "nothing should coalesce when every request completes instantly"
    );
}

#[test]
fn m217_ac01_ticks_between_intervals_are_not_due() {
    let mut p = focused();
    round_trip(&mut p, 0);
    assert_eq!(p.begin(1), PollDecision::NotDue);
    assert_eq!(p.begin(1_999), PollDecision::NotDue);
    assert_eq!(p.begin(2_000), PollDecision::Fire);
}

#[test]
fn m217_ac01_time_until_due_counts_down_within_the_interval() {
    let mut p = focused();
    round_trip(&mut p, 1_000);
    assert_eq!(p.time_until_due_ms(1_000), 2_000);
    assert_eq!(p.time_until_due_ms(2_500), 500);
    assert_eq!(p.time_until_due_ms(3_000), 0);
    assert_eq!(
        p.time_until_due_ms(9_000),
        0,
        "an overdue poller must saturate at zero, not underflow"
    );
}

#[test]
fn m217_ac01_slow_call_coalesces_ticks_instead_of_overlapping() {
    let mut p = focused();
    // A request starts at t=0 and takes 7 seconds — 3.5 intervals.
    assert_eq!(p.begin(0), PollDecision::Fire);
    for now in (250..7_000).step_by(250) {
        assert_eq!(
            p.begin(now),
            PollDecision::Coalesced,
            "tick at {now}ms landed while a request was in flight and must be coalesced"
        );
        assert!(p.is_in_flight());
    }
    assert_eq!(
        p.fired_count(),
        1,
        "a 7s call must never be overlapped by a second request"
    );
    assert!(
        p.coalesced_count() > 20,
        "the dropped ticks must be counted"
    );
}

#[test]
fn m217_ac01_overrun_is_followed_by_exactly_one_catch_up_request() {
    let mut p = focused();
    p.begin(0);
    for now in (250..7_000).step_by(250) {
        p.begin(now);
    }
    p.finish(7_000);
    // The interval is measured from the *start* of the previous
    // request, so the poller is immediately due — but only once.
    assert_eq!(p.begin(7_000), PollDecision::Fire);
    p.finish(7_000);
    assert_eq!(
        p.fired_count(),
        2,
        "one overrun must produce exactly one follow-up request, not one per dropped tick"
    );
    assert_eq!(
        p.begin(7_500),
        PollDecision::NotDue,
        "the follow-up re-arms the interval"
    );
}

#[test]
fn m217_ac01_in_flight_flag_is_released_by_finish() {
    let mut p = focused();
    p.begin(0);
    assert!(p.is_in_flight());
    p.finish(100);
    assert!(!p.is_in_flight());
    assert_eq!(
        p.begin(100),
        PollDecision::NotDue,
        "finishing must not also make the poller due"
    );
}

#[test]
fn m217_ac01_longer_cadence_scales_the_interval() {
    let mut p = AutopilotPoller::with_refresh_secs(5);
    p.set_focused(true);
    round_trip(&mut p, 0);
    assert_eq!(p.begin(4_999), PollDecision::NotDue);
    assert_eq!(p.begin(5_000), PollDecision::Fire);
}
