//! Autopilot setup-region layout.
//!
//! The setup form is the lane's control surface: six stacked sections
//! (topology, harness per role, milestones, commit behavior, run mode,
//! Start + summary) plus the always-present control row. This file
//! pins the *rendered buffer* at 132x36 so a layout regression shows
//! up as a diff rather than as a vague "it looks different".
//!
//! The golden is generated from the same `render::render` path the real
//! TUI uses. Regenerate it with:
//!
//! ```text
//! MP_UPDATE_GOLDEN=1 cargo nextest run -p raul --test autopilot_setup_layout
//! ```
//!
//! Two things the golden deliberately does *not* pin: colors (the
//! TestBackend strips styling, so only the text grid is compared) and
//! anything read from the filesystem (the lane renders in-memory state
//! only — the poller owns every read).

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::app::{App, Lane};
use raul::tui::autopilot::setup::{self, RunMode};
use raul::tui::render;
use raul::tui::view_state;

/// The dimensions the golden is captured at. Wide enough for the
/// 40/60 split plus the sidebar's tab headers, short enough to be a
/// realistic laptop terminal.
const GOLDEN_W: u16 = 132;
const GOLDEN_H: u16 = 36;

const GOLDEN_PATH: &str = "tests/fixtures/autopilot_setup_render.txt";

/// A lane with a representative selection so the golden shows chips in
/// more than one state (selected + unselected) and a non-trivial
/// summary line.
fn app_with_setup() -> App {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    // The Milestones section renders the picker's drivable candidates,
    // so the fixture needs a populated picker — not just a selection.
    app.autopilot.picker.refresh_candidates(&serde_json::json!({
        "milestones": [
            {"id": "M239", "title": "Telemetry", "lifecycle": "approved"},
            {"id": "M240", "title": "Poll", "lifecycle": "in-progress"},
            {"id": "M241", "title": "Split", "lifecycle": "approved"},
            {"id": "M242", "title": "Picker", "lifecycle": "groomed"},
        ]
    }));
    let form = &mut app.autopilot.setup;
    form.set_topology("two-agent");
    form.harness_uniform = false;
    form.set_harness("runner", "cursor");
    // The picker strips the `M` prefix from ids (pre-existing), so the
    // selection uses the picker's own ids — a selection that does not
    // match a candidate would render as nothing selected.
    form.selected = vec!["240".into(), "241".into()];
    form.commit_after_execute = true;
    form.push_after_review = false;
    form.run_mode = RunMode::Normal;
    app.autopilot.session_count = 3;
    app
}

/// Render the Autopilot lane at the golden size and return the buffer
/// as one string per row.
fn render_rows(app: &App) -> Vec<String> {
    let backend = TestBackend::new(GOLDEN_W, GOLDEN_H);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..GOLDEN_H)
        .map(|y| {
            (0..GOLDEN_W)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

fn golden() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_PATH);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e}\nregenerate with MP_UPDATE_GOLDEN=1",
            path.display()
        )
    })
}

/// AC-01: the rendered buffer at 132x36 matches the committed golden.
#[test]
fn setup_layout_matches_committed_golden() {
    let app = app_with_setup();
    let rows = render_rows(&app);
    let actual = rows.join("\n") + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_PATH);

    if std::env::var("MP_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().expect("golden parent")).expect("mkdir");
        std::fs::write(&path, &actual).expect("write golden");
        return;
    }

    let expected = golden();
    assert_eq!(
        actual, expected,
        "setup region drifted from the golden at {GOLDEN_W}x{GOLDEN_H}; \
         regenerate with MP_UPDATE_GOLDEN=1 and review the diff"
    );
}

/// AC-01: the six sections appear in the spec's order, each with its
/// heading, followed by the control row. Asserted on the buffer text so
/// the order is pinned independently of the golden's exact spacing.
#[test]
fn setup_region_renders_six_sections_then_the_control_row() {
    let app = app_with_setup();
    let buffer = render_rows(&app).join("\n");

    let headings = [
        " Topology ",
        " Harness ",
        " Milestones ",
        " Commit ",
        " Run mode ",
        " Start ",
        " Controls ",
    ];
    let mut cursor = 0usize;
    for heading in headings {
        let at = buffer[cursor..]
            .find(heading)
            .unwrap_or_else(|| panic!("section {heading:?} missing or out of order in:\n{buffer}"));
        cursor += at + heading.len();
    }
}

/// AC-01: the section count is exactly six plus the control row — a
/// seventh band would mean the spec's "six stacked sections" grew.
#[test]
fn setup_region_has_exactly_six_sections_plus_controls() {
    assert_eq!(setup::SETUP_SECTIONS.len(), 7);
    // The first six are the spec's sections; the seventh is the strip.
    let spec_sections: Vec<_> = setup::SETUP_SECTIONS
        .iter()
        .filter(|s| **s != setup::SetupSection::ControlRow)
        .collect();
    assert_eq!(spec_sections.len(), 6);
    assert_eq!(
        setup::SetupSection::ControlRow.ordinal(),
        7,
        "the control row renders last, under Start"
    );
}

/// AC-01: the Start summary is `<n> milestones · <topology> · <run
/// mode>` with no time estimate.
#[test]
fn start_summary_names_count_topology_and_run_mode() {
    let app = app_with_setup();
    let buffer = render_rows(&app).join("\n");
    let summary = app.autopilot.setup.summary();
    assert_eq!(summary, "2 milestones · two-agent · normal");
    assert!(
        buffer.contains("2 milestones · two-agent · normal"),
        "summary line missing from the buffer; got:\n{buffer}"
    );
    // The milestone explicitly rules out a time estimate.
    assert!(!summary.contains("min"), "summary must not estimate time");
    assert!(!summary.contains("~"), "summary must not estimate time");
}

/// AC-01 / S2.1: the control row renders all four buttons and is dim
/// (inert) when no run is live.
#[test]
fn control_row_renders_all_four_buttons_inert_when_idle() {
    let app = app_with_setup();
    assert!(!app.autopilot.run_live, "fixture must be idle");
    let buffer = render_rows(&app).join("\n");
    for label in ["Pause", "Stop", "Resume", "Back"] {
        assert!(
            buffer.contains(&format!("[{label}]")),
            "control row must offer {label}; got:\n{buffer}"
        );
    }
}

/// AC-01 / S2.1: with a run live the same four buttons render, and
/// the buttons the run state makes applicable are the ones lit. The
/// TestBackend strips style, so this pins that the row is *present*
/// in both states and that the inert case is not rendered as an
/// empty or missing strip.
#[test]
fn control_row_survives_the_transition_to_a_live_run() {
    let mut app = app_with_setup();
    app.autopilot.note_run_live(true);
    assert!(app.autopilot.takeover_active());
    let takeover = render_rows(&app).join("\n");
    // The takeover replaces the split, so the setup sections are gone…
    assert!(
        !takeover.contains(" Topology "),
        "the takeover must replace the split, not draw both"
    );
    // …but the control row stays reachable without a Back.
    assert!(
        takeover.contains("[Pause]") && takeover.contains("[Back]"),
        "control row must remain on the takeover; got:\n{takeover}"
    );
}

/// AC-02: the left column honours the stored split percentage. The
/// layout math is asserted directly (the rendered pixel column would
/// only be observable through the border glyph).
#[test]
fn split_percentage_drives_the_left_column_width() {
    let mut app = app_with_setup();
    for pct in [25u32, 40, 55, 75] {
        app.autopilot.layout.set_split_pct(pct as i64);
        let expected = ((GOLDEN_W as f64) * (pct as f64 / 100.0)).round() as u16;
        assert_eq!(
            app.autopilot.layout.left_width(GOLDEN_W),
            expected,
            "split_pct {pct} should put the border at column {expected}"
        );
    }
}

/// AC-01: the golden is not accidentally empty — a test that compares
/// two empty strings passes forever.
#[test]
fn golden_is_not_trivially_empty() {
    let expected = golden();
    let non_blank = expected.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(
        non_blank >= 20,
        "golden should have substantial content; only {non_blank} non-blank rows"
    );
    assert!(
        expected.contains("Topology") && expected.contains("Controls"),
        "golden should name the sections"
    );
}
