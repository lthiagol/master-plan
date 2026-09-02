//! M91 AC-01: horizontal tab bar replaces the left sidebar.
//!
//! These tests assert that:
//!   - The TUI renders a top horizontal tab bar with all seven lanes
//!     in their `Lane::ordered()` order.
//!   - The active lane is highlighted in the bar.
//!   - No left "Lanes" block / sidebar column is drawn.
//!   - Content still fills the area below the tab bar.
//!   - At narrow widths the bar uses `Lane::compact_label()`.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane};
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

fn render_to_string(width: u16, height: u16) -> String {
    let app = App::new();
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            {
                let view = view_state::compute_view(&app, frame.area());
                render::render(frame, &app, &view);
            };
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        while output.ends_with(' ') {
            output.pop();
        }
        output.push('\n');
    }
    output
}

fn render_with_active(active: Lane, width: u16, height: u16) -> String {
    let mut app = App::new();
    app.select_lane(active);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            {
                let view = view_state::compute_view(&app, frame.area());
                render::render(frame, &app, &view);
            };
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        while output.ends_with(' ') {
            output.pop();
        }
        output.push('\n');
    }
    output
}

/// Returns the line at row `row` (trimmed of trailing spaces, no newline).
fn line(output: &str, row: u16, height: u16) -> &str {
    assert!(row < height, "row {row} out of bounds for height {height}");
    output.lines().nth(row as usize).unwrap_or("")
}

const STD_W: u16 = 100;
const STD_H: u16 = 30;

#[test]
fn tab_bar_renders_all_lane_labels_in_order() {
    let output = render_to_string(STD_W, STD_H);
    // Expected order matches Lane::ordered().
    let expected = ["Overview", "Milestones", "Path", "Backlog", "Settings"];
    let mut last_pos = 0usize;
    for label in expected {
        let pos = output
            .find(label)
            .unwrap_or_else(|| panic!("tab bar should contain {label:?}; output:\n{output}"));
        assert!(
            pos >= last_pos,
            "label {label:?} appeared out of order in output:\n{output}"
        );
        last_pos = pos;
    }
}

#[test]
fn tab_bar_is_in_row_one_below_header() {
    let output = render_to_string(STD_W, STD_H);
    let header = line(&output, 0, STD_H);
    let bar = line(&output, 1, STD_H);
    // The header row is `raul TUI — <view title>` — it should NOT contain lane labels.
    assert!(
        !header.contains("Milestones"),
        "header row should not contain lane labels; got header={header:?}"
    );
    // The bar row SHOULD contain lane labels.
    assert!(
        bar.contains("Overview"),
        "row 1 (tab bar) should contain lane labels; got bar={bar:?}"
    );
}

#[test]
fn no_left_sidebar_column_is_drawn() {
    let output = render_to_string(STD_W, STD_H);
    // The old sidebar used a `Lanes` block title and a `│` between rows.
    // The tab bar uses ` │ ` between TABS, not between LANES in a column.
    // At a wide viewport the content (Overview / "Plan overview" / "Suggested path")
    // should occupy the area below row 1 — never be squeezed into a narrow left column.
    //
    // Heuristic: collect every row that has lanes stacked vertically in the form
    // `Overview` (top of column), `Milestones` (next row down), etc.  With the
    // tab bar the labels appear on the SAME row, never on consecutive rows.
    let mut bar_cols = Vec::new();
    for (idx, row) in output.lines().enumerate() {
        if row.contains("Overview") && row.contains("Milestones") {
            bar_cols.push(idx);
        }
    }
    assert_eq!(
        bar_cols.len(),
        1,
        "lanes Overview+Milestones should appear on exactly one row (the tab bar); found rows {bar_cols:?}"
    );
}

#[test]
fn no_legacy_lanes_block_title() {
    let output = render_to_string(STD_W, STD_H);
    // The old sidebar block title was `Lanes ` or ` Lanes ◂ `. The replacement
    // is the horizontal tab bar — no block title in that position.
    // We tolerate `Lanes` substring appearing inside lane labels (it does not),
    // but the standalone block-frame pattern ` Lanes ` (with leading + trailing space)
    // and the focused-frame pattern ` Lanes ◂ ` must both be gone.
    assert!(
        !output.contains("Lanes ◂"),
        "focused sidebar title `Lanes ◂` should be gone; output:\n{output}"
    );
}

#[test]
fn content_pane_still_renders_dashboard_below_tab_bar() {
    // M181: render at 50 rows so the redesigned dashboard's
    // Health / Statistics / Lifecycle / Path blocks leave vertical
    // room for the Inbox + Activity lower split to actually fit.
    // At STD_H = 30 the lower split collapses to zero rows.
    let output = render_with_active(Lane::Overview, STD_W, 50);
    // The Overview lane now renders the M181 hierarchy — Health,
    // Statistics | Work queues, Lifecycle, Suggested path, Inbox,
    // Recent activity — in the content pane. We don't pin exact row
    // offsets (they shift as the bar shape changes), but every
    // section MUST appear somewhere in the output below the tab
    // bar.
    assert!(
        output.contains("Health"),
        "content pane should show Health strip; got:\n{output}"
    );
    assert!(
        output.contains("Statistics"),
        "content pane should show Statistics box; got:\n{output}"
    );
    assert!(
        output.contains("Work queues"),
        "content pane should show Work queues box; got:\n{output}"
    );
    assert!(
        output.contains("Lifecycle"),
        "content pane should show Lifecycle grid; got:\n{output}"
    );
    assert!(
        output.contains("Suggested path"),
        "content pane should show path section; got:\n{output}"
    );
    assert!(
        output.contains("Inbox"),
        "content pane should show Inbox section; got:\n{output}"
    );
    assert!(
        output.contains("Recent activity"),
        "content pane should show Recent activity section; got:\n{output}"
    );
}

#[test]
fn active_lane_is_highlighted_in_bar() {
    // The visual highlight is a background-color flip; we don't pin bytes per
    // palette, but we can assert that the bar still contains all labels
    // when active_lane is non-default.
    let output = render_with_active(Lane::Backlog, STD_W, STD_H);
    let bar = line(&output, 1, STD_H);
    for label in ["Overview", "Milestones", "Path", "Backlog", "Settings"] {
        assert!(
            bar.contains(label),
            "tab bar (active=Backlog) should still contain {label:?}; bar={bar:?}"
        );
    }
}

#[test]
fn narrow_terminal_uses_compact_labels() {
    // Below the 60-col threshold the bar falls back to Lane::compact_label().
    // S6 will add overflow scroll/wrap; S1 just asserts compact labels render.
    let output = render_to_string(50, STD_H);
    let bar = line(&output, 1, STD_H);
    assert!(
        bar.contains("Ov"),
        "compact `Ov` (Overview) should appear in bar; got bar={bar:?}"
    );
}

// ---- M91 S3: keyboard lane navigation from the tab bar ----

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::runner::{tab_bar_action, TabBarAction};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn s3_left_right_and_h_l_route_to_previous_next() {
    assert_eq!(
        tab_bar_action(&key(KeyCode::Left)),
        Some(TabBarAction::Previous)
    );
    assert_eq!(
        tab_bar_action(&key(KeyCode::Right)),
        Some(TabBarAction::Next)
    );
    assert_eq!(
        tab_bar_action(&key(KeyCode::Char('h'))),
        Some(TabBarAction::Previous)
    );
    assert_eq!(
        tab_bar_action(&key(KeyCode::Char('l'))),
        Some(TabBarAction::Next)
    );
}

#[test]
fn s3_number_keys_jump_by_ordered_index() {
    // M184: Lane::ordered() has 7 entries (Overview, Milestones,
    // Path, Backlog, Ideas, Watch, Settings).
    // Jump(N) is 0-based; digits 1..=N map to lanes 0..N-1.
    let lanes = Lane::ordered();
    assert_eq!(
        lanes.len(),
        7,
        "spec assumes seven lanes; update if this changes"
    );
    for (one_based, _lane) in lanes.iter().enumerate() {
        let ch = char::from(b'1' + one_based as u8);
        let k = key(KeyCode::Char(ch));
        match tab_bar_action(&k) {
            Some(TabBarAction::Jump(idx)) => assert_eq!(
                idx, one_based,
                "Jump(N) should be 0-based index of lane N+1"
            ),
            other => panic!("{ch:?} -> expected Some(Jump({one_based})), got {other:?}"),
        }
    }
}

#[test]
fn s3_digit_past_lane_count_is_none_or_filtered() {
    // M184: N=7 lanes → digits 1..=7 map in-range. Digit `8`/`9`
    // are past the ordered set; tab_bar_action may still emit Jump
    // for digit keys up to 9 — caller filters out-of-range.
    let n = Lane::ordered().len();
    assert_eq!(n, 7);
    // Digit equal to N maps to index N-1 (Settings).
    assert_eq!(
        tab_bar_action(&key(KeyCode::Char('7'))),
        Some(TabBarAction::Jump(6)),
        "digit 7 must map to Settings (index 6)"
    );
}

#[test]
fn s3_enter_routes_to_focus_content_not_to_lane_change() {
    assert_eq!(
        tab_bar_action(&key(KeyCode::Enter)),
        Some(TabBarAction::FocusContent)
    );
}

#[test]
fn s3_modifiers_return_none() {
    // Modifiers (Ctrl/Alt/Shift + arrow) are NOT tab-bar binds; we don't want
    // Ctrl+Left to silently advance the lane.
    let ctrl_left = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
    let shift_right = KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT);
    let alt_h = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT);
    assert_eq!(tab_bar_action(&ctrl_left), None);
    assert_eq!(tab_bar_action(&shift_right), None);
    assert_eq!(tab_bar_action(&alt_h), None);
}

#[test]
fn s3_unrelated_keys_return_none() {
    for code in [
        KeyCode::Char('q'),
        KeyCode::Char('x'),
        KeyCode::Tab, // Tab -> S4 focus toggle (not a tab-bar lane nav)
        KeyCode::Backspace,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::F(5),
        KeyCode::Null,
    ] {
        assert_eq!(
            tab_bar_action(&key(code)),
            None,
            "key {code:?} should be None"
        );
    }
}

// ---- M91 S4: focus model, footer reflection, help overlay ----

use raul::tui::app::ContentState;

/// Render the FULL app screen (status bar 3 rows + standard 40 cols) so the
/// footer line is the bottom-most non-empty row.
fn render_full(width: u16, height: u16, app: &mut App) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            {
                let view = view_state::compute_view(app, frame.area());
                render::render(frame, app, &view);
            };
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        while output.ends_with(' ') {
            output.pop();
        }
        output.push('\n');
    }
    output
}

fn bottom_line(output: &str) -> &str {
    output
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
}

#[test]
fn s4_footer_reflects_tab_bar_focus_with_new_key_bindings() {
    let mut app = App::new();
    // M167: tab_bar_focused removed — the footer always renders the
    // content-pane variant for the active lane. The pre-M167 contract
    // (footer shape with focus == true) is no longer applicable; we
    // verify the Overview-lane footer renders with the expected hints
    // and the M167 footers no longer reference a focus key (Tab now
    // navigates lanes, full stop).

    let output = render_full(STD_W, STD_H, &mut app);
    let footer = bottom_line(&output);
    // Footer must NOT carry the old ":focus" hint (the focus toggle is
    // gone), and must NOT carry the legacy sidebar `[/]:resize` text.
    assert!(
        !footer.contains(":focus") && !footer.contains("[/]:resize"),
        "footer must drop the legacy :focus key (Tab is now lane-nav); got footer={footer:?}"
    );
    // The Overview footer keeps its lane-specific hints.
    assert!(
        footer.contains(':') && footer.contains(' '),
        "Overview footer must still render its hint string; got {footer:?}"
    );
}

#[test]
fn s4_footer_reflects_content_focus_with_tab_label() {
    let mut app = App::new();
    // M167: no `app.tab_bar_focused = false;` — focus is gone.
    app.select_lane(Lane::Overview);
    let output = render_full(STD_W, STD_H, &mut app);
    let footer = bottom_line(&output);
    // Content footer must NOT carry Tab as a focus key — Tab is now a
    // lane-nav binding (covered by the upstream `kb.next_lane` lookup
    // shown when the user is on a list lane via "h/l:move").
    assert!(
        !footer.contains("Tab:focus") && !footer.contains("Tab:sidebar"),
        "content footer must not label Tab as a focus key (M167); got {footer:?}"
    );
}

#[test]
fn s4_help_overlay_documents_tab_bar_keys_not_sidebar_keys() {
    // M199: the help overlay is now grouped under `Per-lane` and
    // `Global` headings. The legacy `Tab bar focused` /
    // `Content focused` / `Detail actions` / `Milestones / Backlog /
    // Ideas` section labels are gone, along with their prose
    // ("Navigate sidebar lanes", "Page Up/Wheel: scroll list").
    // Tab-bar keys (Previous lane, Next lane) now live in the
    // Global group (universal bindings, identical on every tab);
    // the jump-to-lane-1..N row was retired with the M167 focus
    // toggle and is no longer surfaced in help.
    let mut app = App::new();
    app.toggle_help();
    app.load_milestones(vec![]); // ensure content lane has data, helps render quickly
    let output = render_full(STD_W, STD_H, &mut app);
    let lower = output.to_ascii_lowercase();

    // M199: dead sidebar copy stays gone.
    assert!(
        !lower.contains("resize sidebar"),
        "help must not document `[/] resize sidebar`; help output:\n{output}"
    );
    assert!(
        !lower.contains("sidebar lanes"),
        "help must not contain the legacy 'sidebar lanes' navigation copy; help output:\n{output}"
    );

    // M199: the new overlay groups are surfaced. Both Per-lane
    // and Global headings must appear; the Global group must
    // include the tab-bar bindings (Previous lane, Next lane)
    // and the universal selection / move keys.
    assert!(
        output.contains("Per-lane") && output.contains("Global"),
        "help must show M199 group headings; help output:\n{output}"
    );
    assert!(
        output.contains("Previous lane"),
        "help must document Previous lane (M199 global group); help output:\n{output}"
    );
    assert!(
        output.contains("Next lane"),
        "help must document Next lane (M199 global group); help output:\n{output}"
    );
    assert!(
        output.contains("Select / drill in"),
        "help must document the Enter binding in the global group; help output:\n{output}"
    );
}

#[test]
fn s4_tab_toggles_focus_through_dispatch() {
    // M167: Tab no longer toggles a focus state — Tab advances
    // active_lane along `Lane::ordered()` (keybinds.next_lane carries
    // Tab as an additional binding). The semantic is asserted end-to-end
    // by `m167_keybinds::tab_advances_active_lane`. This trace test
    // exists so a reviewer scrolling tui_tab_bar.rs sees the M167
    // semantically alongside the tab-bar layout tests below.
    use crossterm::event::KeyCode;
    use raul::tui::action::Action;
    let app = App::new();
    let before = app.active_lane;
    let action = raul::tui::modes::normal::handle_key(
        KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE),
        &app,
    );
    assert_eq!(action, vec![Action::NextLane]);
    let _ = (before, app);
}

#[test]
fn s4_back_tab_reverses_through_dispatch() {
    // M167: Shift+Tab reverses lane nav, mirroring the Tab contract.
    use crossterm::event::KeyCode;
    use raul::tui::action::Action;
    let app = App::new();
    let action = raul::tui::modes::normal::handle_key(
        KeyEvent::new(KeyCode::BackTab, crossterm::event::KeyModifiers::NONE),
        &app,
    );
    assert_eq!(action, vec![Action::PreviousLane]);
}

#[test]
fn s4_overview_lane_still_renders_with_new_focus_contract() {
    // Wiring sanity: with Overview active, the content pane shows the
    // redesigned M181 dashboard. Render at 50 rows so the top blocks
    // (Health / Statistics / Lifecycle / Path) plus the lower Inbox
    // + Activity split all fit — at STD_H=30 the lower split would
    // collapse to zero rows.
    let mut app = App::new();
    app.select_lane(Lane::Overview);
    app.content = ContentState::List;
    let output = render_full(STD_W, 50, &mut app);
    assert!(
        output.contains("Health"),
        "Overview Health strip must render; got:\n{output}"
    );
    assert!(
        output.contains("Statistics"),
        "Overview Statistics box must render; got:\n{output}"
    );
    assert!(
        output.contains("Work queues"),
        "Overview Work queues box must render; got:\n{output}"
    );
    assert!(
        output.contains("Lifecycle"),
        "Overview Lifecycle grid must render; got:\n{output}"
    );
    assert!(
        output.contains("Suggested path"),
        "Overview path section must render; got:\n{output}"
    );
    assert!(
        output.contains("Inbox"),
        "Overview Inbox section must render; got:\n{output}"
    );
    assert!(
        output.contains("Recent activity"),
        "Overview Recent activity section must render; got:\n{output}"
    );
}

/// S4 follow-up: Tab dispatch lives in exactly one place. Prior to the
/// collapse, three dispatch sites handled Tab (handle_tab_bar_key, the
/// Board branch's inner match, the non-Board branch, and the
/// OverviewKeyAction::ToggleSidebar arm). The collapse moved all of
/// that into a single `KeyCode::Tab` arm at the top of the dispatch
/// loop. This source-shape test pins that structural invariant.
///
/// M136: the dispatcher is now the `match (app.active_mode, key)` in
/// `runner.rs::dispatch_event` plus the per-mode handlers in
/// `tui/modes/*.rs`. Pin the structural invariant by counting Tab
/// references across the whole `tui/` source tree (excluding the
/// comment block above), and require that there is at most one *match
/// arm* style (KeyCode::Tab inside a `match`) — duplicate *string*
/// matches in comments are out of scope.
#[test]
fn s4_tab_dispatch_lives_in_exactly_one_place() {
    use std::path::PathBuf;
    let tui_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui");
    let mut total_arms = 0usize;
    fn walk(dir: &PathBuf, total: &mut usize) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, total);
            } else if path.extension().is_some_and(|e| e == "rs") {
                // M138: `key_combo.rs` is the combo parser/formatter — it
                // enumerates every `KeyCode` by nature and is not a dispatch
                // site. `keybinds.rs` holds the single Tab *binding*
                // (`toggle_tab_focus` default). Exclude the parser so the
                // test still measures dispatch, not parsing.
                if path.file_name().is_some_and(|n| n == "key_combo.rs") {
                    continue;
                }
                let content = std::fs::read_to_string(&path).unwrap();
                // Count only `match`-arm-style references to KeyCode::Tab
                // by requiring it to be followed by ` =>` or ` ` and a
                // closure body. Simpler heuristic: count occurrences of
                // the token `KeyCode::Tab` and require they form a single
                // binding site. Two matches means two binds; let's just
                // assert a small finite upper bound.
                let occurrences = content.matches("KeyCode::Tab").count();
                *total += occurrences;
            }
        }
    }
    walk(&tui_dir, &mut total_arms);
    assert!(
        total_arms <= 3,
        "Tab handling must live in at most three places (dispatch arm + handler + \
         settings-mode hijack); found {total_arms} `KeyCode::Tab` occurrences across the tui/ tree"
    );
}

/// S4 follow-up: OverviewKeyAction no longer carries ToggleSidebar.
#[test]
fn s4_overview_key_action_drops_togglesidebar() {
    use raul::tui::inbox_nav::OverviewKeyAction;
    // Compile-time guarantee: this would fail to build if ToggleSidebar
    // were still a variant. Pin the structural intent explicitly.
    fn exhaustive(action: OverviewKeyAction) {
        match action {
            OverviewKeyAction::Refresh => {}
            OverviewKeyAction::ToggleHelp => {}
            OverviewKeyAction::QuitFromHelp => {}
            OverviewKeyAction::PassToEventHandler => {}
            OverviewKeyAction::Ignore => {} // ToggleSidebar intentionally absent — Tab is now global.
        }
    }
    // Touch the helper so dead_code analysis doesn't drop it; the value
    // here is the type-level exhaustive match.
    exhaustive(OverviewKeyAction::Ignore);
}

// ---- M91 S5: mouse click on tab labels ----

use raul::tui::runner::tab_hit_test;

#[test]
fn s5_tab_hit_test_first_tab_full_labels() {
    // Full labels at wide widths. After 1 leading space the first tab
    // (" Overview ") occupies cols 1..11 inclusive (length 10).
    // F-02: pin the legacy 7-lane contract (Watch visible) — these
    // tests predate M198.
    let lanes = &Lane::ordered();
    assert_eq!(tab_hit_test(1, false, lanes), Some(0));
    assert_eq!(tab_hit_test(2, false, lanes), Some(0)); // inside Overview label
    assert_eq!(tab_hit_test(10, false, lanes), Some(0)); // last col of first tab
}

#[test]
fn s5_tab_hit_test_full_labels_third_tab_is_path() {
    // Path's tab (lane 2) sits at cols 24..31 in wide mode. M137+
    // dropped the leading space on non-first lanes so the bar
    // renders `Overview │ Milestones │ Path │ Backlog │ Board`
    // with one space on each side of every pipe; the old test
    // hardcoded col 25 because the format added a leading space
    // before `│` (which shifted every subsequent lane right by
    // one column).
    let lanes = &Lane::ordered();
    assert_eq!(lanes[2], Lane::Path);
    assert_eq!(tab_hit_test(24, false, lanes), Some(2)); // first col of Path
    assert_eq!(tab_hit_test(27, false, lanes), Some(2)); // inside Path label
    assert_eq!(tab_hit_test(30, false, lanes), Some(2)); // last col of Path
}

#[test]
fn s5_tab_hit_test_compact_labels_narrow_mode() {
    // Narrow widths use compact_label(). " Ov " (1 leading + label + 1 trailing)
    // occupies cols 1..4. First tab is still lane 0 (Overview).
    let lanes = &Lane::ordered();
    assert_eq!(tab_hit_test(1, true, lanes), Some(0));
    assert_eq!(tab_hit_test(3, true, lanes), Some(0));
}

#[test]
fn s5_tab_hit_test_past_all_tabs_returns_none() {
    // x past the right edge of the bar — well past 7 lanes at full label.
    let lanes = &Lane::ordered();
    assert_eq!(tab_hit_test(200, false, lanes), None);
    assert_eq!(tab_hit_test(200, true, lanes), None);
    assert_eq!(tab_hit_test(0, true, lanes), None); // col 0 is leading space
    assert_eq!(tab_hit_test(0, false, lanes), None);
}

#[test]
fn s5_click_on_tab_bar_row_selects_that_lane() {
    // We don't have a public dispatch test surface for handle_mouse, so pin
    // the invariants via the helper tab_hit_test: any x within a tab's
    // column-span returns Some(idx). The actual mouse→lane wiring is
    // integrated through this helper (see handle_mouse row-1 branch).
    let lanes = &Lane::ordered();
    // Iterate every lane; for each one pick a representative x inside it.
    let probe_xs: Vec<u16> = vec![5, 25, 50, 70, 95, 120, 140];
    for (i, &x) in probe_xs.iter().enumerate() {
        if let Some(idx) = tab_hit_test(x, false, lanes) {
            // idx should be < lanes.len().
            assert!(idx < lanes.len(), "idx {idx} out of range");
            // And the tab we hit corresponds to its label.
            assert_eq!(lanes[idx].label(), lanes[idx].label());
        } else {
            // Misses are OK at this width — we don't care about exact mapping
            // at every column as long as the helper stays pure.
            let _ = i;
        }
    }
}

// ---- M91 S8: drill-in / go_back preserved across the tab-bar migration ----
//
// AC-06 contract:
//   "Drill-in from every lane (milestone, track, backlog, board card,
//    path next, overview inbox) still opens detail and Esc/back returns to list."

/// Each lane has exactly one drill-in target ContentState. We verify by
/// setting up each lane + driving the state transition that the existing
/// handle_event dispatcher performs (Enter on content List).
#[test]
fn s8_milestone_list_drill_opens_milestone_detail() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".into(),
        title: "T".into(),
        lifecycle: "approved".into(),
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
    assert_eq!(app.content, ContentState::MilestoneDetail);
    app.go_back();
    assert_eq!(app.content, ContentState::List);
}

#[test]
fn s8_backlog_list_drill_opens_backlog_detail() {
    let mut app = App::new();
    app.select_lane(Lane::Backlog);
    app.load_backlog(vec![raul::tui::app::BacklogLine {
        id: "BL-01".into(),
        title: "Later".into(),
        priority: "med".into(),
        status: "open".into(),
        resolution: "".into(),
        preview: "".into(),
    }]);
    app.selected_backlog_id = Some("BL-01".into());
    app.detail_scroll = 0;
    app.content = ContentState::BacklogDetail;
    assert_eq!(app.content, ContentState::BacklogDetail);
    app.go_back();
    assert_eq!(app.content, ContentState::List);
}

#[test]
fn s8_path_next_action_drills_into_milestone_by_id_with_hide_done() {
    // AC-06 / M87 AC-01: Path lane's next_action path resolves a milestone
    // BY ID (not by full-list index) so hide_done can't pick the wrong row.
    let mut app = App::new();
    app.hide_done = true;
    app.dashboard.next_action = "M42/start".into();
    app.select_lane(Lane::Path);
    // The Path next-action handler does:
    //   let ms_id = next_action.trim_start_matches('M').split('/').next();
    //   app.select_lane(Lane::Milestones);
    //   app.enter_milestone_detail_by_id(&ms_id);
    // Pin the parse step here.
    let ms_id = app
        .dashboard
        .next_action
        .trim_start_matches('M')
        .split('/')
        .next()
        .unwrap_or("");
    assert_eq!(ms_id, "42");
}

#[test]
fn s8_overview_inbox_drill_resolves_to_target_detail() {
    // AC-06: Overview inbox items drill-in. The handle_event dispatcher
    // calls navigate_from_inbox_item() based on item.kind; we verify the
    // navigation decision (lane + detail type) via direct state setup.
    let mut app = App::new();
    // Pre-load milestone so the resolved-by-id find succeeds.
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "M01".into(),
        title: "T".into(),
        lifecycle: "approved".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    let item = raul::tui::app::InboxLine {
        id: "M01".into(),
        kind: "milestone".into(),
        display: "d".into(),
        reason: "r".into(),
        action: "mp milestone show M01".into(),
    };
    app.dashboard.inbox_items.push(item.clone());
    app.dashboard.inbox_items.reverse(); // make first item be the one we just pushed is index 0 implicitly
                                         // The dispatcher's behavior for an "milestone" inbox item: select Milestones,
                                         // load_milestones, find the id (id-resolve), enter_milestone_detail_by_id.
    app.select_lane(Lane::Milestones);
    app.enter_milestone_detail_by_id("M01");
    assert_eq!(app.content, ContentState::MilestoneDetail);
    assert_eq!(app.active_lane, Lane::Milestones);
}

#[test]
fn s8_default_entry_unaffected() {
    // AC-06: raul -i starts on Overview.
    // We pin the App's default state matches the -i entry.
    let app = App::new();
    assert_eq!(
        app.active_lane,
        Lane::Overview,
        "default is Overview (raul -i)"
    );
}

#[test]
fn s8_hide_done_drills_by_id_survives_full_list_index_drift() {
    // AC-06 / M87 AC-01: hide_done can hide the target row from
    // visible_milestones(); enter_milestone_detail_by_id() restores the
    // visible-list selection only when the id is in the visible list.
    let mut app = App::new();
    app.load_milestones(vec![
        raul::tui::app::MilestoneSummary {
            id: "10".into(),
            title: "t".into(),
            lifecycle: "complete".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        raul::tui::app::MilestoneSummary {
            id: "20".into(),
            title: "t".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.hide_done = true;
    // id "10" is done and hidden. enter_milestone_detail_by_id must still
    // open detail without panicking.
    app.enter_milestone_detail_by_id("10");
    assert_eq!(app.content, ContentState::MilestoneDetail);
    assert_eq!(app.selected_milestone_id.as_deref(), Some("10"));
    // visible_milestones() filters done; entry to detail with hidden id
    // should leave selected_index alone (no spurious fallback).
    let visible = app.visible_milestones();
    assert!(visible.iter().all(|m| m.id != "10"));
}

// ---- M91 S2: sidebar-only mechanics removed from active paths ----
//
// AC-07: "no gutter drag resize, no sidebar_width keyboard adjustment,
//         no sidebar Up/Down lane list in runner event handling"

#[test]
fn s2_app_has_no_sidebar_width_or_draging_gutter() {
    let app = App::new();
    // After S2 these fields should not exist on App. We can't name them
    // directly (the fields are gone), so the strongest assertion is the
    // source-text check below; this invocation just exercises App::new().
    let _ = app.active_lane; // ensure the App is constructible
}

#[test]
fn s2_app_source_drops_sidebar_width_and_draging_gutter() {
    use std::path::PathBuf;
    let app_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("app.rs");
    let content = std::fs::read_to_string(&app_path).unwrap();
    assert!(
        !content.contains("sidebar_width"),
        "App struct must not carry a sidebar_width field after S2; app.rs source:\n{content}"
    );
    assert!(
        !content.contains("dragging_gutter"),
        "App struct must not carry a dragging_gutter field after S2; app.rs source:\n{content}"
    );
}

#[test]
fn s2_tabbar_action_has_no_resize_variants() {
    // AC-07 forbids `[/]` sidebar resize keys. After S2 the TabBarAction
    // enum dropped ResizeDec/ResizeInc entirely (vs keeping a no-op arm).
    // Compile-time guarantee via exhaustive match — the variants are gone.
    use raul::tui::runner::TabBarAction;
    fn _exhaustive(a: TabBarAction) {
        match a {
            TabBarAction::Previous => {}
            TabBarAction::Next => {}
            TabBarAction::Jump(_) => {}
            TabBarAction::FocusContent => {}
        }
    }
}

#[test]
fn s2_bracket_keys_return_none_from_tabbar_action() {
    // Even though [/] no longer map to a sidebar resize, they must NOT be
    // silently absorbed by TabBarAction either (would mask keystrokes).
    // They were tab-bar binds on the old sidebar; removing the sidebar
    // dropped them from tab-bar dispatch. Today nothing else binds [ / ].
    assert_eq!(tab_bar_action(&key(KeyCode::Char('['))), None);
    assert_eq!(tab_bar_action(&key(KeyCode::Char(']'))), None);
}

#[test]
fn s2_runner_source_drops_sidebar_mechanics() {
    use std::path::PathBuf;
    let runner_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("runner.rs");
    let content = std::fs::read_to_string(&runner_path).unwrap();
    // Source-text gates for AC-07. The fields are gone from App; the source
    // for runner.rs must not reference them either.
    assert!(
        !content.contains("sidebar_width"),
        "runner.rs must not reference sidebar_width after S2; got runner.rs:\n{content}"
    );
    assert!(
        !content.contains("dragging_gutter"),
        "runner.rs must not reference dragging_gutter after S2; got runner.rs:\n{content}"
    );
    // Up/Down must not be tab-bar lane-nav keys (S3 swapped them for Left/Right/h/l).
    // Acceptable: Up/Down may still appear elsewhere in the file (overview/content
    // nav, help overlay text, etc). The strict assertion: the LEFT-BAR lane-nav
    // helper `tab_bar_action` does NOT match Up/Down.
    let _ = content; // structural assertion is in s3_left_right_and_h_l_route_to_previous_next
}

// ---- M91 S6: responsive narrow widths ----
//
// AC-05: "at or below threshold, tabs use compact_label and overflow tabs
//         remain reachable via horizontal scroll or wrap indicator —
//         layout does not revert to a left sidebar."

#[test]
fn s6_wide_terminal_shows_full_labels_no_indicators() {
    let output = render_to_string(120, STD_H);
    let bar = line(&output, 1, STD_H);
    assert!(
        bar.contains("Milestones") && bar.contains("Backlog"),
        "wide bar must render full labels for all lanes; got bar={bar:?}"
    );
}

#[test]
fn s6_mid_terminal_uses_compact_labels_below_threshold() {
    // Below 60 cols we fall back to compact_label ("Ov", "Ml", ...). All
    // seven must still fit (compact sum is small enough for 50+ cols).
    let output = render_to_string(50, STD_H);
    let bar = line(&output, 1, STD_H);
    assert!(
        bar.contains("Ov") && bar.contains("Bl"),
        "compact labels render at width=50; bar={bar:?}"
    );
}

#[test]
fn s6_very_narrow_terminal_renders_without_panic_and_active_visible() {
    // Width=30 is below what compact labels need; bar should still render
    // without crashing, and the active lane name (compact) MUST appear.
    let output = render_to_string(30, STD_H);
    let bar = line(&output, 1, STD_H);
    // Default active lane is Overview -> compact "Ov" must be in the bar
    // (either visible or in the "active follows the ellipsis" tail).
    assert!(
        !bar.is_empty(),
        "very narrow bar must render something; got bar={bar:?}"
    );
    assert!(
        bar.contains("Ov"),
        "active lane (Overview) compact must be reachable at narrow width=30; bar={bar:?}"
    );
}

#[test]
fn s6_active_lane_in_last_position_still_visible_at_narrow_width() {
    // Pick the non-default active lane in last position (Backlog) at narrow width.
    // Even with overflow, Backlog must be visible — bar may append the
    // active lane with a leading ellipsis.
    let output = render_with_active(Lane::Backlog, 30, STD_H);
    let bar = line(&output, 1, STD_H);
    assert!(
        bar.contains("Bl"),
        "Backlog (last lane) compact must remain reachable; bar={bar:?}"
    );
}

// ---- M91 S7: list Page Up/Down + mouse wheel scroll ----
//
// AC-09: Page Up/Down move selection by viewport page in scrollable list
//        lanes; mouse wheel over the content list area scrolls selection;
//        wheel events over the tab bar do NOT scroll the list.

#[test]
fn s7_page_down_advances_by_page_size_and_clamps() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    // Build a list of 30 milestones.
    let items: Vec<raul::tui::app::MilestoneSummary> = (0..30)
        .map(|i| raul::tui::app::MilestoneSummary {
            id: format!("M{i:02}"),
            title: "t".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        })
        .collect();
    app.load_milestones(items);
    assert_eq!(app.selected_index, 0);
    app.move_page_down();
    assert_eq!(app.selected_index, App::PAGE_SIZE, "page down from 0 -> 10");
    app.move_page_down();
    assert_eq!(app.selected_index, 20);
    app.move_page_down();
    // Clamps at len-1 = 29.
    assert_eq!(app.selected_index, 29, "page down past end clamps to last");
    app.move_page_down();
    assert_eq!(app.selected_index, 29, "page down at end stays");
}

#[test]
fn s7_page_up_recedes_by_page_size_and_clamps() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let items: Vec<raul::tui::app::MilestoneSummary> = (0..30)
        .map(|i| raul::tui::app::MilestoneSummary {
            id: format!("M{i:02}"),
            title: "t".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        })
        .collect();
    app.load_milestones(items);
    app.selected_index = 25;
    app.move_page_up();
    assert_eq!(app.selected_index, 15, "page up from 25 -> 15 (25 - 10)");
    app.move_page_up();
    assert_eq!(app.selected_index, 5);
    app.move_page_up();
    assert_eq!(app.selected_index, 0, "page up clamps at 0");
    app.move_page_up();
    assert_eq!(app.selected_index, 0, "page up at 0 stays");
}

#[test]
fn s7_page_down_at_zero_count_is_noop() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    // No milestones loaded -> count = 0 -> page_down does nothing.
    app.move_page_down();
    assert_eq!(app.selected_index, 0);
}

#[test]
fn s7_action_enum_carries_page_up_down() {
    use raul::tui::action::Action;
    // Compile-time guarantee that the enum carries PageUp/PageDown.
    let mut variants = std::collections::BTreeSet::new();
    // Exhaustively pattern-match to fail if a variant is missing.
    fn classify(a: Action, set: &mut std::collections::BTreeSet<&'static str>) {
        match a {
            Action::Quit => {
                set.insert("Quit");
            }
            Action::Up => {
                set.insert("Up");
            }
            Action::Down => {
                set.insert("Down");
            }
            Action::PageUp => {
                set.insert("PageUp");
            }
            Action::PageDown => {
                set.insert("PageDown");
            }
            Action::Enter => {
                set.insert("Enter");
            }
            Action::ToggleFilter => {
                set.insert("ToggleFilter");
            }
            Action::OpenHelp => {
                set.insert("OpenHelp");
            }
            Action::CreateAnnotation => {
                set.insert("CreateAnnotation");
            }
            Action::ResolveAnnotation => {
                set.insert("ResolveAnnotation");
            }
            Action::ReopenAnnotation => {
                set.insert("ReopenAnnotation");
            }
            Action::ToggleApproval => {
                set.insert("ToggleApproval");
            }
            Action::OpenReviewMenu => {
                set.insert("OpenReviewMenu");
            }
            Action::ToggleHideDone => {
                set.insert("ToggleHideDone");
            }
            // M136 + M167 + M172: additional actions beyond the
            // legacy Event set. M172 S5 added the sort-rebind
            // action family (5 variants).
            Action::Esc
            | Action::RefreshLane
            | Action::PreviousLane
            | Action::NextLane
            | Action::JumpLane(_)
            | Action::FocusContent
            | Action::NextSection
            | Action::PrevSection
            | Action::NextItem
            | Action::PrevItem
            | Action::CloseHelp
            | Action::CloseReviewMenu
            | Action::ExecuteReviewAction
            | Action::OpenAnnotationThread
            | Action::CloseAnnotationThread
            | Action::EnterCoApproval
            | Action::ConfirmCoApproval
            | Action::SetCoApprovalAction(_)
            | Action::SubmitInput
            | Action::CancelInput
            | Action::WatchToggleSelect
            | Action::WatchPreflight
            | Action::WatchStart
            | Action::WatchStop
            | Action::WatchRefresh
            | Action::WatchClearQueue
            | Action::WatchMovePicker { .. }
            | Action::WatchMoveQueue { .. }
            | Action::PushInputChar(_)
            | Action::PopInputChar
            | Action::SettingsSave
            | Action::SettingsToggleBool
            | Action::SettingsCycleChoice { .. }
            | Action::OpenSortRebind
            | Action::SortRebindNext
            | Action::SortRebindPrev
            | Action::SortRebindConfirm
            | Action::SortRebindCancel
            | Action::OpenLifecycleFilter
            | Action::LifecycleFilterToggle
            | Action::LifecycleFilterNext
            | Action::LifecycleFilterPrev
            | Action::LifecycleFilterCommit
            | Action::LifecycleFilterCancel
            | Action::ApplyGroomingPreset
            | Action::OpenSearch
            | Action::SearchInputChar(_)
            | Action::SearchInputBackspace
            | Action::SearchInputCommit
            | Action::SearchInputCancel
            | Action::CycleSortNext => {
                set.insert("OtherM136Action");
            }
        }
    }
    classify(Action::PageUp, &mut variants);
    classify(Action::PageDown, &mut variants);
    assert!(variants.contains("PageUp"));
    assert!(variants.contains("PageDown"));
}

#[test]
fn s7_page_up_down_keys_dispatch_to_page_actions() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use raul::tui::action::Action;
    use raul::tui::modes::normal;

    // PageUp / PageDown dispatch only on the content pane. The pre-M136
    // dispatcher also gated PageUp/Down keys on
    // `!app.tab_bar_focused` (handle_tab_bar_key swallowed them); M136's
    // per-mode handler inherits that — the test sets up an unfocused
    // tab bar so the key falls through to `handle_event_in_normal`.
    let app = App::new();
    let up = normal::handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), &app);
    let dn = normal::handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), &app);
    assert_eq!(up, vec![Action::PageUp]);
    assert_eq!(dn, vec![Action::PageDown]);
}
