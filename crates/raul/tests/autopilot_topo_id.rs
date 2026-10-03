//! The derived, read-only topology id.
//!
//! `<topology>-<NNN>` where `NNN` is one more than the number of
//! sessions in `mp autopilot session list`, zero-padded to three
//! digits. Two properties matter and are easy to lose:
//!
//! 1. It is **derived**, not stored. Nothing about the id is persisted,
//!    so it cannot go stale in a config file — it is recomputed from
//!    the live session count every time it is drawn.
//! 2. It is **read-only**. It is a display label; there is no edit
//!    affordance and no key that writes it.

use raul::tui::app::{App, Lane};
use raul::tui::autopilot::setup::topology_id;

fn app() -> App {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    // The override panel is where the id is shown; open it so the test
    // exercises the real context.
    app.autopilot.open_panel();
    app
}

/// AC-07: the id is `<topology>-<NNN>`, NNN zero-padded to three.
#[test]
fn the_topology_id_is_topology_dash_zero_padded_count() {
    assert_eq!(topology_id("one-agent", 0), "one-agent-001");
    assert_eq!(topology_id("two-agent", 0), "two-agent-001");
    assert_eq!(topology_id("three-agent", 0), "three-agent-001");
    assert_eq!(topology_id("three-agent", 3), "three-agent-004");
    assert_eq!(topology_id("three-agent", 9), "three-agent-010");
}

/// AC-07: NNN is `1 + session count`, so a project with no sessions
/// still gets a first run a distinct id.
#[test]
fn the_count_is_one_more_than_the_session_list_length() {
    for sessions in [0usize, 1, 2, 41, 998] {
        let id = topology_id("three-agent", sessions);
        let suffix = id.rsplit('-').next().expect("id has a numeric suffix");
        assert_eq!(
            suffix.parse::<usize>().expect("suffix is numeric"),
            sessions + 1,
            "suffix should be 1 + {sessions}"
        );
    }
}

/// AC-07: the example from the spec.
#[test]
fn the_spec_example_holds() {
    // three-agent with three existing sessions → three-agent-004.
    assert_eq!(topology_id("three-agent", 3), "three-agent-004");
}

/// The suffix grows past three digits rather than being truncated or
/// wrapped — a run 1000 sessions in is still a distinct run.
#[test]
fn the_suffix_grows_past_three_digits() {
    assert_eq!(topology_id("three-agent", 998), "three-agent-999");
    assert_eq!(topology_id("three-agent", 999), "three-agent-1000");
    assert_eq!(topology_id("three-agent", 9999), "three-agent-10000");
}

/// AC-07: the id follows the topology chip. Changing the topology
/// changes the id's prefix, because the id describes the run the
/// operator is about to start.
#[test]
fn the_id_follows_the_selected_topology() {
    let mut app = app();
    app.autopilot.session_count = 3;
    assert_eq!(app.autopilot.topology_id(), "three-agent-004");
    app.autopilot.setup.set_topology("two-agent");
    assert_eq!(app.autopilot.topology_id(), "two-agent-004");
    app.autopilot.setup.set_topology("one-agent");
    assert_eq!(app.autopilot.topology_id(), "one-agent-004");
}

/// AC-07 / "nothing is persisted": the id is a function of state, so
/// changing the state changes the id. There is no stored copy that
/// could disagree with what is displayed.
#[test]
fn the_id_is_derived_from_state_on_every_read() {
    let mut app = app();
    let first = app.autopilot.topology_id();
    // Nothing changed → same id.
    assert_eq!(app.autopilot.topology_id(), first);
    // A new session appears → the id advances.
    app.autopilot.session_count += 1;
    assert_ne!(app.autopilot.topology_id(), first);
    // The topology changes → the id changes.
    let before_topology = app.autopilot.topology_id();
    app.autopilot.setup.set_topology("one-agent");
    assert_ne!(app.autopilot.topology_id(), before_topology);
}

/// AC-07: an unknown topology string still produces a well-formed id
/// rather than panicking — the form validates the chip, but a
/// hand-edited config should not crash the panel.
#[test]
fn an_unknown_topology_still_yields_a_well_formed_id() {
    let id = topology_id("four-agent", 1);
    assert_eq!(id, "four-agent-002");
    assert_eq!(id.rsplit('-').next().expect("suffix"), "002");
}

/// AC-07: the id is read-only. Nothing in the lane's action surface
/// writes it — the only things the form persists are the topology, the
/// per-role harness, and the two commit toggles.
#[test]
fn no_action_writes_the_topology_id() {
    let mut app = app();
    app.autopilot.session_count = 5;
    let before = app.autopilot.topology_id();

    // Toggling the panel, the setup form, and the run mode leaves the
    // derived id alone.
    app.autopilot.toggle_panel();
    app.autopilot.setup.run_mode = raul::tui::autopilot::setup::RunMode::Detached;
    app.autopilot.setup.commit_after_execute = true;
    assert_eq!(app.autopilot.topology_id(), before);

    // The field does not exist on the form: there is nowhere to put it.
    let form = &app.autopilot.setup;
    let serialized = serde_json::to_string(form).expect("setup form serializes");
    assert!(
        !serialized.contains("001"),
        "the setup form must not carry a run id: {serialized}"
    );
}

/// The id is rendered in the override panel, labelled as derived and
/// read-only so the operator does not try to edit it.
#[test]
fn the_override_panel_shows_the_id_as_derived_and_read_only() {
    let app = {
        let mut app = app();
        app.autopilot.session_count = 3;
        app
    };
    let screen = ratatui::backend::TestBackend::new(132, 36);
    let mut terminal = ratatui::Terminal::new(screen).expect("terminal");
    terminal
        .draw(|frame| {
            let view = raul::tui::view_state::compute_view(&app, frame.area());
            raul::tui::render::render(frame, &app, &view);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let text: String = (0..36)
        .flat_map(|y| (0..132).map(move |x| (x, y)))
        .map(|(x, y)| buffer[(x, y)].symbol().to_string())
        .collect();
    assert!(text.contains("three-agent-004"), "id missing:\n{text}");
    assert!(
        text.contains("derived, read-only"),
        "the id must be labelled read-only:\n{text}"
    );
}
