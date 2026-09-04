//! M199 AC-02: on every lane, the per-tab footer line matches the
//! per-(lane, content_state) table from `Keybinds::footer_per_tab`.
//! The M199 redesign flipped the layout (globals on top, per-tab on
//! bottom) and consolidated the four pre-M199 footer methods into a
//! single per-(lane, content_state) table.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, ContentState, Lane};
use raul::tui::render;
use raul::tui::view_state;

fn render_footer_rows(app: &App) -> (String, String) {
    // M199: globals is on the top row of `footer_area`, per-tab is
    // on the bottom row. For lanes with an empty per-tab string
    // (Path, Watch), `footer_area.height == 1` and the per-tab
    // tuple slot is an empty string (the renderer doesn't paint a
    // per-tab row at all).
    let backend = TestBackend::new(140, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let view = view_state::compute_view(app, *buf.area());
    let footer_y = view.footer_area.y;
    let footer_h = view.footer_area.height;
    let mut globals = String::new();
    let mut per_tab = String::new();
    for x in 0..buf.area().width {
        globals.push_str(buf[(x, footer_y)].symbol());
        if footer_h >= 2 {
            per_tab.push_str(buf[(x, footer_y + 1)].symbol());
        }
    }
    (globals, per_tab)
}

fn expected_per_tab(app: &App) -> String {
    let settings_staged = app.settings.as_ref().is_some_and(|s| s.has_staged_edits());
    app.keybinds
        .footer_per_tab(app.active_lane, app.content, app.open_only, settings_staged)
}

#[test]
fn per_tab_line_matches_footer_per_tab_on_all_lanes() {
    // M199: every (lane, content_state) pair produces a per-tab
    // string from the new table. This test walks the seven ordered
    // lanes (M184) and asserts the per-tab row matches the
    // expected string.
    let lanes = [
        Lane::Overview,
        Lane::Milestones,
        Lane::Path,
        Lane::Backlog,
        Lane::Ideas,
        Lane::Autopilot,
        Lane::Settings,
    ];

    for lane in lanes {
        let mut app = App::new();
        // Settings needs an open settings state for the Save/Cancel footer.
        if lane == Lane::Settings {
            app.settings = Some(raul::tui::mode::SettingsState::new(serde_json::json!({})));
        }
        app.select_lane(lane);
        let (globals, per_tab) = render_footer_rows(&app);
        let expected = expected_per_tab(&app);
        // The per-tab row must contain the lane-specific tokens
        // (e.g. `F:filter`, `/:search` for Milestones) and must
        // not contain any of the six globals (so the duplication
        // guard in D-04 holds).
        let tokens: Vec<&str> = expected
            .split_whitespace()
            .filter(|t| t.contains(':') && !t.starts_with('['))
            .collect();
        // Lanes with an empty per-tab string (Path, Watch) skip
        // the per-tab row entirely.
        // M217: the Autopilot lane has no per-tab *keybind* glyphs
        // but does carry the auto-refresh indicator, so its footer
        // is now 2 rows with `poll: …` on the per-tab line. The
        // height is derived from `view_state::footer_per_tab_text`
        // (glyphs + indicators), which is what the renderer paints.
        if lane == Lane::Autopilot {
            assert!(
                per_tab.contains("poll:"),
                "lane Autopilot: per-tab row must carry the poll indicator; got={per_tab:?}"
            );
            continue;
        }
        if expected.is_empty() {
            assert!(
                per_tab.trim().is_empty(),
                "lane {lane:?}: per-tab row should be empty (footer is 1-row); got={per_tab:?}"
            );
            continue;
        }
        // Settings uses bracket markers (`[Save (s)]`, `[Cancel (Esc)]`)
        // rather than colon-separated glyphs. Verify the markers
        // are present.
        if lane == Lane::Settings {
            assert!(
                per_tab.contains("[Save (s)]") && per_tab.contains("[Cancel (Esc)]"),
                "lane Settings: per-tab line must list Save/Cancel markers; got={per_tab:?}"
            );
            continue;
        }
        assert!(
            !tokens.is_empty(),
            "expected per-tab tokens for {lane:?}; raw={expected:?}"
        );
        for tok in tokens {
            assert!(
                per_tab.contains(tok),
                "lane {lane:?}: per-tab line missing {tok:?}; got={per_tab:?} expected={expected:?}"
            );
        }
        // Globals tokens must NOT appear in the per-tab row.
        for tok in [":quit", ":help", ":refresh", ":go", ":move", ":lanes"] {
            assert!(
                !per_tab.contains(tok),
                "lane {lane:?}: per-tab line must not duplicate globals token {tok:?}; got={per_tab:?}"
            );
        }
        // And the globals row must contain the six universal
        // tokens — the M199 design promise.
        for tok in [":quit", ":help", ":refresh", ":go", ":move", ":lanes"] {
            assert!(
                globals.contains(tok),
                "lane {lane:?}: globals line must contain {tok:?}; got={globals:?}"
            );
        }
    }
}

#[test]
fn footer_per_tab_returns_expected_strings_for_every_pair() {
    // M199 AC-02 sub-pin: walk every (lane, content_state) pair
    // (not just the seven List-state lanes) and assert the
    // per-(lane, content_state) table returns a non-empty string
    // for every reachable pair. Path, Watch, and Ideas/<non-List>
    // intentionally return the empty string (D-07).
    let app = App::new();
    let mut reached = 0usize;
    for lane in [
        Lane::Overview,
        Lane::Milestones,
        Lane::Path,
        Lane::Backlog,
        Lane::Ideas,
        Lane::Autopilot,
        Lane::Settings,
    ] {
        for content in [
            ContentState::List,
            ContentState::MilestoneDetail,
            ContentState::BacklogDetail,
            ContentState::AnnotationThread,
            ContentState::CoApproval,
        ] {
            let s = app.keybinds.footer_per_tab(lane, content, false, false);
            match (lane, content) {
                (Lane::Path, _)
                | (Lane::Autopilot, _)
                | (Lane::Ideas, ContentState::MilestoneDetail)
                | (Lane::Ideas, ContentState::BacklogDetail)
                | (Lane::Ideas, ContentState::AnnotationThread)
                | (Lane::Ideas, ContentState::CoApproval) => {
                    assert!(
                        s.is_empty(),
                        "({lane:?}, {content:?}) should be empty; got={s:?}"
                    );
                }
                _ => {
                    // Reachable pairs always produce a populated
                    // string — the per-(lane, content_state) table
                    // is exhaustive. The string either carries a
                    // ":"-separated `glyph:label` token, or starts
                    // with `[` (Settings's Save/Cancel markers).
                    assert!(
                        s.contains(':') || s.trim_start().starts_with('['),
                        "({lane:?}, {content:?}) should produce a non-empty per-tab string; got={s:?}"
                    );
                }
            }
            reached += 1;
        }
    }
    assert_eq!(reached, 7 * 5, "exhausted every pair");
}
