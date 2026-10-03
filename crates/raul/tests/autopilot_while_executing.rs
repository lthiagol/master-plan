//! The While-Executing takeover and the read-only per-milestone peek.
//!
//! The takeover is the lane's second view: it replaces the split
//! entirely while `mp autopilot status` reports a live run. The tests
//! here pin the *transition* rules and the *shape* of the takeover,
//! because those are the parts that can silently regress — a takeover
//! that forgets to yield to Back, or a State tab that reappears mid-run,
//! both look fine in a screenshot and are wrong in use.
//!
//! Rendering is asserted through `TestBackend` where the assertion is
//! about what the operator can see, and through the state machine
//! where it is about a rule.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::action::{self, Action};
use raul::tui::app::{App, Lane};
use raul::tui::autopilot::setup::queued_milestones;
use raul::tui::autopilot::setup::TakeoverRow;
use raul::tui::autopilot::setup::{self, MilestonePeek, SidebarTab};
use raul::tui::render::autopilot_lane::lifecycle_position_bar;
use raul::tui::{render, view_state};

const W: u16 = 132;
const H: u16 = 36;

fn app_with_queue(ids: &[&str]) -> App {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    app.autopilot.setup.selected = ids.iter().map(|s| s.to_string()).collect();
    app
}

fn live_app(ids: &[&str]) -> App {
    let mut app = app_with_queue(ids);
    app.autopilot.note_run_live(true);
    app
}

fn screen(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).expect("terminal");
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..H)
        .map(|y| {
            (0..W)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── the transition ──────────────────────────────────────────────

/// AC-05: a live run promotes the split to the takeover.
#[test]
fn a_live_run_replaces_the_split_with_the_takeover() {
    let mut app = app_with_queue(&["240"]);
    assert!(!app.autopilot.takeover_active(), "idle shows the split");
    app.autopilot.note_run_live(true);
    assert!(app.autopilot.takeover_active());
}

/// AC-05: the takeover really does replace the split on screen — the
/// setup sections are gone, not drawn underneath.
#[test]
fn the_takeover_replaces_the_setup_sections() {
    let app = live_app(&["240"]);
    let screen = screen(&app);
    assert!(screen.contains("While executing"));
    assert!(screen.contains("Queued milestones"));
    for heading in [" Topology ", " Harness ", " Run mode "] {
        assert!(
            !screen.contains(heading),
            "{heading:?} must not be drawn under the takeover; got:\n{screen}"
        );
    }
}

/// AC-05: all five takeover bands are present — topology strip, one
/// row per queued milestone, activity tail, health strip, control row.
#[test]
fn the_takeover_renders_all_five_bands() {
    let app = live_app(&["240", "241"]);
    let screen = screen(&app);
    // 1. topology strip
    assert!(screen.contains("While executing"), "topology strip missing");
    // 2. one row per queued milestone
    assert!(screen.contains("Queued milestones"), "rows band missing");
    for id in ["240", "241"] {
        assert!(screen.contains(id), "row for {id} missing from:\n{screen}");
    }
    // 3. health strip
    assert!(screen.contains("health"), "health strip missing");
    // 4. activity tail
    assert!(
        screen.contains("(no activity yet)"),
        "activity tail missing"
    );
    // 5. control row — still reachable without a Back
    for label in ["[Pause]", "[Stop]", "[Resume]", "[Back]"] {
        assert!(screen.contains(label), "control {label} missing");
    }
}

/// AC-05: each row carries the id, the lifecycle, a cycle count, and
/// the lifecycle-position bar.
#[test]
fn each_row_carries_id_lifecycle_cycle_and_position_bar() {
    let app = live_app(&["240"]);
    let rows = queued_milestones(&app);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "240");
    assert!(!rows[0].lifecycle.is_empty());
    assert!(rows[0].cycle >= 1);
    let bar = lifecycle_position_bar(&rows[0].lifecycle);
    assert!(bar.starts_with('[') && bar.ends_with(']'));
    assert!(bar.contains('#'), "a live milestone should be past stage 0");
}

/// The bar tracks the canonical lifecycle order, so two milestones at
/// different stages are comparable at a glance.
#[test]
fn the_position_bar_advances_with_the_lifecycle() {
    let early = lifecycle_position_bar("approved");
    let later = lifecycle_position_bar("self-reviewed");
    let done = lifecycle_position_bar("complete");
    assert!(
        early.matches('#').count() < later.matches('#').count(),
        "self-reviewed should be further along than approved: {early} vs {later}"
    );
    assert!(
        later.matches('#').count() < done.matches('#').count(),
        "complete should be furthest along: {later} vs {done}"
    );
    // An unknown lifecycle renders an empty bar rather than panicking or
    // claiming a position.
    assert_eq!(lifecycle_position_bar("not-a-lifecycle"), "[.......]");
}

/// AC-05: the live queue view wins over the form's selection, because
/// the drive's own queue is the truth about what is running.
#[test]
fn the_live_queue_overrides_the_forms_selection() {
    let mut app = live_app(&["form-selection"]);
    app.autopilot.queue_view = Some(raul::tui::autopilot::QueueView {
        session_id: "alpha".to_string(),
        rows: vec![
            raul::tui::autopilot::QueueRow {
                milestone_id: "live-1".to_string(),
                title: "A".to_string(),
                lifecycle: "in-progress".to_string(),
                active: true,
            },
            raul::tui::autopilot::QueueRow {
                milestone_id: "live-2".to_string(),
                title: "B".to_string(),
                lifecycle: "approved".to_string(),
                active: false,
            },
        ],
        status: "active".to_string(),
    });
    let rows = queued_milestones(&app);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "live-1");
    assert!(rows[0].active, "the active row is the one being worked on");
    assert!(!rows[1].active);
    assert!(
        !rows.iter().any(|r: &TakeoverRow| r.id == "form-selection"),
        "the form's selection is a fallback, not an override"
    );
}

/// AC-05: a live run with an empty queue still shows the takeover (with
/// an empty rows band) rather than falling back to the split — the run
/// is live, and a layout that flickers back to the form mid-run is
/// worse than an empty list.
#[test]
fn a_live_run_with_nothing_queued_still_takes_over() {
    let app = live_app(&[]);
    assert!(app.autopilot.takeover_active());
    let screen = screen(&app);
    assert!(screen.contains("While executing"));
    assert!(screen.contains("(no queued milestones)"));
    assert!(!screen.contains(" Topology "));
}

// ─── Back from the takeover (AC-06) ──────────────────────────────

/// AC-06 / `state_tab_hidden`: Esc returns to the split while the run
/// continues, and the State tab is withheld.
#[test]
fn state_tab_is_hidden_after_backing_out_of_the_takeover() {
    let mut app = live_app(&["240"]);
    assert!(app.autopilot.takeover_active());
    // The State tab is withheld the moment a run goes live.
    assert!(app.autopilot.layout.state_tab_hidden);

    // Back.
    assert!(app.autopilot.dismiss_takeover());
    assert!(!app.autopilot.takeover_active());
    // The run keeps going.
    assert!(app.autopilot.run_live, "Back must not stop the run");
    // And the State tab stays hidden.
    assert!(app.autopilot.layout.state_tab_hidden);
    assert_eq!(app.autopilot.layout.sidebar_tab, SidebarTab::Progress);

    // Back is a no-op with no live run — Esc then falls through to its
    // other targets rather than swallowing the key.
    let mut idle = app_with_queue(&["240"]);
    assert!(!idle.autopilot.dismiss_takeover());
}

/// AC-06: Back is sticky. The next poll tick re-reports a live run, and
/// the split must not spring back to the takeover under the operator.
#[test]
fn back_is_sticky_across_poll_ticks() {
    let mut app = live_app(&["240"]);
    app.autopilot.dismiss_takeover();
    // The poller re-reports the same live run.
    app.autopilot.note_run_live(true);
    assert!(
        !app.autopilot.takeover_active(),
        "the takeover must not return"
    );
    // Only a run that ends and a new one that starts re-arms it.
    app.autopilot.note_run_live(false);
    app.autopilot.note_run_live(true);
    assert!(
        app.autopilot.takeover_active(),
        "a new run re-arms the takeover"
    );
}

/// AC-06: after Back, the split is on screen and the State tab is not
/// clickable.
#[test]
fn after_back_the_split_is_drawn_without_the_state_tab() {
    let mut app = live_app(&["240"]);
    app.autopilot.dismiss_takeover();
    let screen = screen(&app);
    assert!(screen.contains(" Topology "), "the split should be back");
    assert!(!screen.contains("While executing"));
    // The tab header row still names Progress and Activity.
    assert!(screen.contains("Progress"));
    assert!(screen.contains("Activity"));

    let regions = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H))
        .autopilot
        .expect("hit areas published");
    assert!(regions.tab_rect(SidebarTab::State).is_none());
}

/// AC-06: the split is read-only behind a live run. The form still
/// renders, but the takeover is one poll away and the spec puts
/// mid-run edits out of scope, so the lane does not present the form as
/// editable while a run owns the screen.
#[test]
fn the_split_behind_a_live_run_is_reachable_but_not_the_takeover() {
    let mut app = live_app(&["240"]);
    app.autopilot.dismiss_takeover();
    // The control row is still live — that is how the operator stops
    // the run they just backed out of.
    assert!(app.autopilot.run_live);
    let screen = screen(&app);
    assert!(screen.contains("[Stop]"));
}

// ─── the peek (AC-06) ────────────────────────────────────────────

/// AC-06: the peek is read-only and carries `intent.outcome` plus the
/// AC list.
#[test]
fn the_peek_carries_the_outcome_and_the_ac_list() {
    let payload = serde_json::json!({
        "milestone": {"id": "241", "title": "Autopilot split"},
        "intent": {"outcome": "Operators configure and watch a run on one screen."},
        "acceptance_criteria": [
            {"id": "AC-01", "status": "passed"},
            {"id": "AC-02", "status": "pending"},
        ],
    });
    let peek = MilestonePeek::from_show_json("241", &payload).expect("peek builds");
    assert_eq!(peek.milestone_id, "241");
    assert_eq!(peek.title, "Autopilot split");
    assert!(peek.outcome.contains("Operators configure"));
    assert_eq!(peek.acs.len(), 2);
    assert_eq!(peek.acs[0].id, "AC-01");
    assert_eq!(peek.acs[0].status, "passed");
    assert_eq!(peek.acs[1].status, "pending");
}

/// AC-06: a payload with no `intent.outcome` opens no modal. A
/// truncated response would otherwise render an empty peek that looks
/// like a milestone with nothing to say.
#[test]
fn a_payload_without_an_outcome_opens_no_peek() {
    assert!(MilestonePeek::from_show_json("241", &serde_json::json!({})).is_none());
    assert!(MilestonePeek::from_show_json("241", &serde_json::json!({"intent": {}})).is_none());
    assert!(
        MilestonePeek::from_show_json("241", &serde_json::json!({"intent": {"outcome": 42}}))
            .is_none()
    );
}

/// AC-06: a milestone with no ACs still opens — the AC list is empty,
/// which is a real state, not a failure.
#[test]
fn a_milestone_with_no_acs_still_opens_a_peek() {
    let peek = MilestonePeek::from_show_json(
        "241",
        &serde_json::json!({
            "intent": {"outcome": "something"},
            "acceptance_criteria": [],
        }),
    )
    .expect("peek builds");
    assert!(peek.acs.is_empty());
}

/// AC-06: clicking a takeover row requests the peek for that row.
#[test]
fn clicking_a_takeover_row_targets_that_milestone() {
    let mut app = live_app(&["240", "241"]);
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H));
    let regions = view.autopilot.clone().expect("hit areas");
    assert_eq!(regions.takeover_rows.len(), 2);
    let second = &regions.takeover_rows[1];
    assert_eq!(second.milestone_id, "241");
    raul::tui::mouse::handle_dispatch(&mut app, &view, second.rect.x + 1, second.rect.y, false);
    assert_eq!(app.autopilot.peek_target.as_deref(), Some("241"));
}

/// AC-06: the peek renders intent.outcome, the AC ids + statuses, and
/// the `mp reviews show` hint.
#[test]
fn the_peek_modal_renders_the_outcome_acs_and_hint() {
    let mut app = live_app(&["241"]);
    let payload = serde_json::json!({
        "milestone": {"id": "241", "title": "Autopilot split"},
        "intent": {"outcome": "One screen for configure and watch."},
        "acceptance_criteria": [
            {"id": "AC-01", "status": "passed"},
            {"id": "AC-02", "status": "pending"},
        ],
    });
    assert!(raul::tui::mouse::open_peek_from_payload(
        &mut app, "241", &payload
    ));
    let screen = screen(&app);
    assert!(screen.contains("241"));
    assert!(screen.contains("One screen for configure and watch."));
    assert!(screen.contains("AC-01"));
    assert!(screen.contains("passed"));
    assert!(screen.contains("AC-02"));
    assert!(screen.contains("mp reviews show 241"));
    assert!(screen.contains("read-only"));
}

/// AC-06: the peek is closed by Esc.
#[test]
fn esc_closes_the_peek() {
    let mut app = live_app(&["241"]);
    app.autopilot.open_peek(MilestonePeek {
        milestone_id: "241".to_string(),
        outcome: "x".to_string(),
        ..Default::default()
    });
    assert!(app.autopilot.peek.is_some());
    // Esc in the peek is `AutopilotClosePeek`, not Back — the peek is
    // the more modal of the two.
    action::apply_action(
        &mut app,
        &raul::mp_runner::MpRunner::new().expect("runner"),
        Action::AutopilotClosePeek,
    )
    .expect("apply");
    assert!(app.autopilot.peek.is_none());
}

/// AC-06: a run that is not live has no takeover rows, so there is
/// nothing to peek at from the split.
#[test]
fn the_split_publishes_no_takeover_rows() {
    let app = app_with_queue(&["240"]);
    let regions = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H))
        .autopilot
        .expect("hit areas");
    assert!(regions.takeover_rows.is_empty());
}

/// AC-06: the takeover's row band is elastic and the control row is
/// always the last band, so a queue longer than the terminal is clipped
/// rather than pushing the controls off screen.
#[test]
fn takeover_bands_keep_the_control_row_on_screen() {
    let area = ratatui::layout::Rect::new(0, 0, W, H);
    let (topology, telemetry, rows, activity, control) = setup::takeover_bands(area);
    assert_eq!(topology.y, 0);
    assert_eq!(telemetry.y, topology.y + topology.height);
    assert_eq!(rows.y, telemetry.y + telemetry.height);
    assert!(control.y + control.height <= area.y + area.height);
    // The activity tail is the *last row of* the rows band, so it ends
    // where the rows band ends — and never past the control row.
    assert_eq!(activity.y + activity.height, rows.y + rows.height);
    assert!(activity.y + activity.height <= control.y);
}

/// The bands stay inside the lane even when the terminal is too short
/// to hold them all — saturating, never wrapping into a panic.
#[test]
fn takeover_bands_degrade_on_a_short_terminal() {
    for height in [0u16, 1, 2, 3, 4, 5, 7] {
        let area = ratatui::layout::Rect::new(0, 0, W, height);
        let (topology, telemetry, rows, activity, control) = setup::takeover_bands(area);
        for rect in [topology, telemetry, rows, activity, control] {
            assert!(
                rect.y + rect.height <= area.y + area.height,
                "band {rect:?} escaped a {height}-row lane"
            );
        }
    }
}
