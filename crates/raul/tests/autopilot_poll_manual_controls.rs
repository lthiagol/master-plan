//! M217 / AC-03 — manual refresh + poll toggle.
//!
//! Two operator controls, and the footer that makes the second
//! one legible:
//!
//! * **`r` — refresh now.** Immediate, and it *resets* the poll
//!   timer so the automatic poll does not fire again a few
//!   milliseconds later against the same data.
//! * **`Ctrl-p` — auto-refresh on/off.** Display-only. The state
//!   is rendered in the lane footer, because a paused poll and a
//!   stalled drive look identical on a frozen screen otherwise.
//!
//! Keybind note: the spec names plain `p` for the toggle, but
//! M216 ships `p` as *pause the session* on this lane. The toggle
//! takes `Ctrl-p` rather than silently rebinding a shipped
//! destructive control; `toggle_poll = "p"` in `keybinds.toml`
//! restores the spec's letter for users who want it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::app::{App, Lane};
use raul::tui::keybinds::Keybinds;
use raul::tui::poll::{AutopilotPoller, PollDecision};

fn focused() -> AutopilotPoller {
    let mut p = AutopilotPoller::new();
    p.set_focused(true);
    p
}

fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

// ─── manual refresh resets the timer ────────────────────────────

#[test]
fn m217_ac03_manual_refresh_is_admitted_immediately() {
    let mut p = focused();
    assert!(p.manual_refresh(0), "an operator refresh is never queued");
    p.finish(0);
    assert_eq!(p.fired_count(), 1);
}

#[test]
fn m217_ac03_manual_refresh_resets_the_poll_timer() {
    let mut p = focused();
    // Automatic poll at t=0.
    assert_eq!(p.begin(0), PollDecision::Fire);
    p.finish(0);
    // Operator hits `r` at t=1900 — 100ms before the auto poll.
    assert!(p.manual_refresh(1_900));
    p.finish(1_900);
    assert_eq!(
        p.begin(2_000),
        PollDecision::NotDue,
        "the auto poll must not fire 100ms after a manual refresh of the same data"
    );
    assert_eq!(p.begin(3_899), PollDecision::NotDue);
    assert_eq!(
        p.begin(3_900),
        PollDecision::Fire,
        "the next auto poll is one full interval after the manual refresh"
    );
}

#[test]
fn m217_ac03_manual_refresh_works_while_auto_refresh_is_off() {
    let mut p = focused();
    p.toggle_enabled();
    assert!(!p.is_enabled());
    assert_eq!(p.decide(0), PollDecision::Disabled);
    assert!(
        p.manual_refresh(0),
        "turning auto-refresh off must not disable the manual path"
    );
    p.finish(0);
    assert_eq!(p.fired_count(), 1);
}

#[test]
fn m217_ac03_manual_refresh_respects_single_flight() {
    let mut p = focused();
    p.begin(0);
    assert!(
        !p.manual_refresh(10),
        "mashing `r` during a slow call must not spawn an overlapping request"
    );
    assert_eq!(p.fired_count(), 1);
    assert_eq!(p.coalesced_count(), 1);
}

// ─── the on/off toggle ──────────────────────────────────────────

#[test]
fn m217_ac03_toggle_flips_and_reports_the_new_state() {
    let mut p = focused();
    assert!(p.is_enabled(), "auto-refresh is on by default");
    assert!(!p.toggle_enabled());
    assert!(!p.is_enabled());
    assert!(p.toggle_enabled());
    assert!(p.is_enabled());
}

#[test]
fn m217_ac03_disabled_poller_never_fires_automatically() {
    let mut p = focused();
    p.toggle_enabled();
    for now in (0..30_000).step_by(250) {
        assert_eq!(p.begin(now), PollDecision::Disabled);
    }
    assert_eq!(p.fired_count(), 0);
}

#[test]
fn m217_ac03_re_enabling_arms_one_immediate_poll() {
    let mut p = focused();
    p.begin(0);
    p.finish(0);
    p.toggle_enabled(); // off
    for now in (250..30_000).step_by(250) {
        p.begin(now);
    }
    p.toggle_enabled(); // on
    assert_eq!(
        p.begin(30_000),
        PollDecision::Fire,
        "re-enabling should refresh right away"
    );
    p.finish(30_000);
    assert_eq!(p.fired_count(), 2, "…once, not once per interval spent off");
}

#[test]
fn m217_ac03_set_enabled_is_equivalent_to_the_toggle() {
    let mut p = focused();
    p.set_enabled(true);
    assert!(p.is_enabled(), "setting the current value is a no-op");
    p.set_enabled(false);
    assert!(!p.is_enabled());
    p.set_enabled(false);
    assert!(!p.is_enabled());
    p.set_enabled(true);
    assert!(p.is_enabled());
}

// ─── footer visibility ──────────────────────────────────────────

#[test]
fn m217_ac03_footer_label_shows_the_toggle_state() {
    let mut p = focused();
    let on = p.footer_label();
    assert!(
        on.contains("poll:"),
        "footer must name the poll: got {on:?}"
    );
    assert!(
        on.contains("2s"),
        "the on-state must show the cadence: got {on:?}"
    );
    p.toggle_enabled();
    let off = p.footer_label();
    assert_eq!(off, "poll: off");
    assert_ne!(on, off, "the two states must be visually distinguishable");
}

#[test]
fn m217_ac03_lane_footer_carries_the_poll_state() {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    let footer_on = raul::tui::view_state::footer_per_tab_text(&app);
    assert!(
        footer_on.contains("poll:"),
        "the Autopilot footer must surface the poll state; got {footer_on:?}"
    );
    app.autopilot_poller.toggle_enabled();
    let footer_off = raul::tui::view_state::footer_per_tab_text(&app);
    assert!(
        footer_off.contains("poll: off"),
        "a paused poll must be visible in the footer, not silent; got {footer_off:?}"
    );
    assert_ne!(footer_on, footer_off);
}

#[test]
fn m217_ac03_other_lanes_do_not_carry_the_poll_indicator() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let footer = raul::tui::view_state::footer_per_tab_text(&app);
    assert!(
        !footer.contains("poll:"),
        "the poll indicator belongs to the Autopilot lane only; got {footer:?}"
    );
}

// ─── production key path ────────────────────────────────────────

#[test]
fn m217_ac03_default_toggle_binding_is_ctrl_p() {
    let kb = Keybinds::default();
    let combos = &kb.lane_autopilot.toggle_poll;
    assert_eq!(combos.len(), 1);
    assert!(
        raul::tui::keybinds::any_matches(combos, &key(KeyCode::Char('p'), KeyModifiers::CONTROL)),
        "toggle_poll must default to Ctrl-p"
    );
    assert!(
        !raul::tui::keybinds::any_matches(combos, &key(KeyCode::Char('p'), KeyModifiers::NONE)),
        "plain `p` stays bound to M216's pause"
    );
    assert!(
        raul::tui::keybinds::any_matches(
            &kb.lane_autopilot.pause,
            &key(KeyCode::Char('p'), KeyModifiers::NONE)
        ),
        "M216's pause binding must be untouched by this milestone"
    );
}

#[test]
fn m217_ac03_toggle_key_dispatches_the_toggle_action_on_the_lane() {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    let actions =
        raul::tui::modes::normal::handle_key(key(KeyCode::Char('p'), KeyModifiers::CONTROL), &app);
    assert!(
        actions.contains(&Action::AutopilotTogglePoll),
        "expected AutopilotTogglePoll; got {actions:?}"
    );
}

#[test]
fn m217_ac03_refresh_key_still_dispatches_the_refresh_action() {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    let actions =
        raul::tui::modes::normal::handle_key(key(KeyCode::Char('r'), KeyModifiers::NONE), &app);
    assert!(
        actions.contains(&Action::AutopilotRefresh),
        "expected AutopilotRefresh; got {actions:?}"
    );
}
