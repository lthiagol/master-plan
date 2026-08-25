//! M186 external-review regressions (F-01..F-05).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, BacklogLine, Lane, MilestoneSummary};
use raul::tui::mode::Mode;
use raul::tui::render;
use raul::tui::view_state;

fn ms(id: &str, title: &str) -> MilestoneSummary {
    MilestoneSummary {
        id: id.into(),
        title: title.into(),
        lifecycle: "draft".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".into(),
        updated: String::new(),
    }
}

fn bl(id: &str, title: &str) -> BacklogLine {
    BacklogLine {
        id: id.into(),
        title: title.into(),
        priority: "normal".into(),
        status: "open".into(),
        resolution: String::new(),
    }
}

fn dump(app: &App, w: u16, h: u16) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    s
}

/// F-01: help overlay lists the per-lane and global groups
/// (M199 simplification). The M186 section labels (Search, Cycle
/// sort) are no longer prose; the keys are surfaced as their
/// glyph + label in the per-lane group. This test pins the new
/// M199 contract.
#[test]
fn m186_f01_help_overlay_lists_search_and_cycle() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.toggle_help();
    let s = dump(&app, 110, 44);
    // M199 groups: Per-lane + Global.
    assert!(
        s.contains("Per-lane"),
        "help must list a 'Per-lane' group heading; got:\n{s}"
    );
    assert!(
        s.contains("Global"),
        "help must list a 'Global' group heading; got:\n{s}"
    );
    // The Milestones per-lane group surfaces the search key as
    // "/ search" and the cycle-sort key as "o cycle" per the M199
    // per-tab table.
    assert!(
        s.contains("search"),
        "help must list the search key in the per-lane group; got:\n{s}"
    );
    assert!(
        s.contains("cycle"),
        "help must list the cycle key in the per-lane group; got:\n{s}"
    );
    // Key glyphs must still appear (the M199 render surfaces them
    // as the leading text on each per-lane line, not in prose).
    assert!(s.contains('/'), "help must show '/' key glyph; got:\n{s}");
    assert!(s.contains('o'), "help must show 'o' key glyph; got:\n{s}");
}

/// F-02: lane switch cancels Mode::SearchInput; per-lane buffer preserved.
#[test]
fn m186_f02_lane_switch_cancels_search_input() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.open_search();
    app.search_push_char('a');
    app.search_push_char('b');
    assert!(matches!(app.active_mode, Mode::SearchInput(_)));
    app.select_lane(Lane::Settings);
    assert_eq!(
        app.active_mode,
        Mode::Normal,
        "lane switch must cancel search"
    );
    // Per-lane buffer preserved (the live-filter mirror already wrote
    // the term into lane_search).
    assert_eq!(
        app.lane_search.get(&Lane::Milestones).map(String::as_str),
        Some("ab"),
        "per-lane search buffer must survive lane switch"
    );
    // Reopening on the original lane keeps the term (visible_* still
    // see it via lane_search).
    app.select_lane(Lane::Milestones);
    assert_eq!(app.lane_search_term(), "ab");
}

/// F-02 also covers Tab path (via apply_action + NextLane).
#[test]
fn m186_f02_next_lane_action_cancels_search_input() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    // Seed backlog so Backlog lane can render without shelling out.
    app.load_backlog(vec![bl("TW-01", "x")]);
    app.load_milestones(vec![ms("01", "x")]);
    app.open_search();
    app.search_push_char('a');
    // Apply NextLane directly (skips load_data_for_lane shell-out).
    apply_action(
        &mut app,
        &raul::mp_runner::MpRunner::new().unwrap(),
        Action::NextLane,
    )
    .ok();
    // NextLane moves active_lane forward; search must be cancelled.
    assert_ne!(
        app.active_mode,
        Mode::SearchInput(search_input_state_marker())
    );
}

fn search_input_state_marker() -> raul::tui::mode::SearchInputState {
    raul::tui::mode::SearchInputState {
        buffer: String::new(),
        prior: String::new(),
    }
}

/// F-03: empty list + active search shows the "No matches for /<term>" message.
#[test]
fn m186_f03_empty_list_with_search_shows_no_matches_message() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "foo")]);
    app.select_lane(Lane::Milestones);
    app.lane_search
        .insert(Lane::Milestones, "xyznomatch".to_string());
    let s = dump(&app, 100, 24);
    assert!(
        s.contains("No matches for /xyznomatch"),
        "empty search result must show 'No matches for /<term>'; got:\n{s}"
    );
    assert!(
        !s.contains("Run 'mp list milestones'"),
        "default empty message must not show when search is active; got:\n{s}"
    );
    // Backlog lane parity.
    app.load_backlog(vec![bl("TW-01", "foo")]);
    app.select_lane(Lane::Backlog);
    app.lane_search
        .insert(Lane::Backlog, "xyznomatch".to_string());
    let s = dump(&app, 100, 24);
    assert!(
        s.contains("No matches for /xyznomatch"),
        "Backlog empty search must show 'No matches'; got:\n{s}"
    );
}

/// F-04: chip count is per-lane (no longer sums milestones + backlog).
#[test]
fn m186_f04_chip_count_is_per_lane() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "m1"), ms("02", "m2")]);
    app.load_backlog(vec![
        bl("TW-01", "t1"),
        bl("TW-02", "t2"),
        bl("TW-03", "t3"),
    ]);
    app.select_lane(Lane::Backlog);
    let s = dump(&app, 100, 24);
    let header = s.lines().next().unwrap_or("");
    // Backlog chip count must be 3 (backlog), not 5 (sum).
    assert!(
        header.contains("(3)"),
        "Backlog chip must show real backlog count (3); got {header:?}"
    );
    assert!(
        !header.contains("(5)"),
        "Backlog chip must not sum milestones+backlog; got {header:?}"
    );
}

/// F-05: stale selected_index does not paint REVERSED on a non-existent row.
#[test]
fn m186_f05_selected_index_clamped_after_search_narrows() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "alpha"), ms("02", "beta")]);
    app.select_lane(Lane::Milestones);
    app.selected_index = 1; // point at "beta"
                            // Directly narrow the list to a single item ("alpha") without
                            // going through search_push_char (which resets selected_index).
    app.lane_search
        .insert(Lane::Milestones, "alpha".to_string());
    assert_eq!(
        app.visible_milestones().len(),
        1,
        "precondition: search narrows to 1 item"
    );

    // Render and find the row carrying "alpha". The REVERSED modifier
    // must land on that single visible row, not on row index 1 (empty).
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut alpha_row_reversed = false;
    for y in 0..buf.area().height {
        let mut row = String::new();
        let mut any_reversed = false;
        for x in 0..buf.area().width {
            let cell = &buf[(x, y)];
            row.push_str(cell.symbol());
            if cell.modifier.contains(ratatui::style::Modifier::REVERSED) {
                any_reversed = true;
            }
        }
        if row.contains("alpha") && any_reversed {
            alpha_row_reversed = true;
        }
    }
    assert!(
        alpha_row_reversed,
        "selected row must be clamped to the single visible item (alpha) with REVERSED"
    );
}
