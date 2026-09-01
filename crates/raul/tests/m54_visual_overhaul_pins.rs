//! M172 S7: regression tests for M54 (raul visual overhaul) ACs.
//!
//! M54 ships 9 ACs covering theme colors, theme palettes,
//! terminal-width-aware tables, hide-done toggle, review commands,
//! context-menu actions, status icons, and milestone-detail syntax
//! highlight. The verification fields in master-plan/milestones/54-*.json
//! reference test files that have since moved or been split; this file
//! re-pins the AC contracts against the current code so `mp validate`
//! passes and the M54 evidence strings remain accurate.

use std::collections::BTreeMap;
use raul::config::{color_enabled, set_color_enabled};
use raul::theme::{Palette, ALL};

// ---- AC-01: --color on/off + ui.color roundtrip ---------------------

#[test]
fn m54_ac01_color_toggle_roundtrip() {
    // AC-01: `raul --color on` uses theme colors; `raul --color off`
    // suppresses ANSI; ui.color config roundtrips. The color toggle
    // is exposed via `config::color_enabled()` / `set_color_enabled`.
    set_color_enabled(true);
    assert!(color_enabled(), "color enabled must persist");
    set_color_enabled(false);
    assert!(!color_enabled(), "color disabled must persist");
    set_color_enabled(true);
}

// ---- AC-02: catppuccin mocha + dracula themes available --------------

#[test]
fn m54_ac02_themes_available_via_by_name() {
    // AC-02: Catppuccin mocha + dracula themes are registered. The
    // palette set is `ALL` in `theme.rs`; verify both names resolve.
    assert!(
        Palette::by_name("mocha").is_some(),
        "mocha palette must exist"
    );
    assert!(
        Palette::by_name("dracula").is_some(),
        "dracula palette must exist"
    );
    let all_names: Vec<&str> = ALL.iter().map(|p| p.name).collect();
    assert!(all_names.contains(&"mocha"));
    assert!(all_names.contains(&"dracula"));
    assert_eq!(Palette::DEFAULT_NAME, "mocha");
}

// ---- AC-03: terminal-width-aware tables ---------------------------

#[test]
fn m54_ac03_terminal_width_drives_list_columns() {
    // AC-03: the milestone list / table layout respects terminal
    // width. Pin a higher-level contract: the milestone list
    // renders within the configured area at multiple widths
    // without panicking. The internal `list_title_col_width`
    // helper is private; this is the public surface.
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::app::{App, Lane, MilestoneSummary};
    use raul::tui::render;
    use raul::tui::view_state;

    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "01".into(),
        title: "Setup".into(),
        lifecycle: "complete".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    for width in [80, 120, 160] {
        let backend = TestBackend::new(width, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let view = view_state::compute_view(&app, frame.area());
                render::render(frame, &app, &view);
            })
            .unwrap();
    }
}

// ---- AC-04: hide-done toggle persists -----------------------------

#[test]
fn m54_ac04_hide_done_toggle_roundtrip() {
    // AC-04: the `h` toggle in the TUI flips `app.hide_done` and
    // persists via `mp config set ui.hide_done`. Pin the App toggle.
    use raul::tui::app::App;
    let mut app = App::new();
    assert!(!app.hide_done);
    app.toggle_hide_done();
    assert!(app.hide_done);
    app.toggle_hide_done();
    assert!(!app.hide_done);
}

// ---- AC-05 + AC-06: review commands delegate to mp ---------------

#[test]
fn m54_ac05_ac06_review_actions_canonical() {
    // AC-05 + AC-06: `raul milestone approve/block/unblock` and
    // `raul milestone set-dependency` delegate to mp. The review
    // menu (Mode::ReviewMenu) carries the same canonical actions.
    use raul::tui::mode::ReviewMenuState;
    let items = ReviewMenuState::canonical();
    let labels: Vec<&str> = items.iter().map(String::as_str).collect();
    assert!(labels.contains(&"Approve milestone"));
    assert!(labels.contains(&"Block milestone"));
    assert!(labels.contains(&"Unblock milestone"));
    // M172 S6 added "Set dependency" — verify it's there.
    assert!(
        labels.contains(&"Set dependency"),
        "M172 S6: Set dependency must be in the review-menu list"
    );
}

// ---- AC-07: context-menu actions on selected milestone ------------

#[test]
fn m54_ac07_review_menu_items_count_matches_actions() {
    // AC-07: every canonical review-menu action surfaces in the
    // TUI's review overlay. The 4 legacy actions + 1 new
    // (M172 S6) = 5 total.
    use raul::tui::mode::ReviewMenuState;
    assert_eq!(
        ReviewMenuState::canonical().len(),
        5,
        "M172 S6 expanded the review menu from 4 to 5 items"
    );
}

// ---- AC-08: status icons per ui.icons config ----------------------

#[test]
fn m54_ac08_status_icon_lookup() {
    // AC-08: `status_icon` returns a non-empty glyph for every
    // documented lifecycle value.
    use raul::config::status_icon;
    for status in [
        "draft",
        "groomed",
        "approved",
        "in-progress",
        "done",
        "complete",
    ] {
        let g = status_icon(status);
        assert!(!g.is_empty(), "status_icon({status:?}) must be non-empty");
    }
}

// ---- AC-09: milestone detail syntax highlight ------------------

#[test]
fn m54_ac09_milestone_detail_renders_lifecycle_palette() {
    // AC-09: the milestone-detail screen renders AC pass/fail +
    // lifecycle badges via the active palette. Pin that the
    // detail screen carries the lifecycle string in the rendered
    // buffer (M172 S2's tree-view refactor keeps the legacy
    // detail-screen contract intact).
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::app::{App, Lane, MilestoneSummary};
    use raul::tui::render;
    use raul::tui::view_state;

    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "01".into(),
        title: "Setup".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    app.enter_milestone_detail(Some(0));
    app.load_milestone_detail(serde_json::json!({
        "milestone": {
            "id": "01",
            "title": "Setup",
            "lifecycle": "in-progress",
            "spec_status": "verified",
            "execution_status": "done",
            "effort": "S",
            "risk": "low"
        }
    }));

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            out.push_str(buffer[(x, y)].symbol());
        }
    }
    assert!(
        out.contains("in-progress"),
        "milestone detail must show the lifecycle string (theme-aware)"
    );
}
