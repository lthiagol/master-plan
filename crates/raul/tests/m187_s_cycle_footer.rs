//! M187 regressions: S-key dispatch fix, column-order cycle, footer flip.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, BacklogLine, Lane, SortKey};
use raul::tui::modes;
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

fn ms(id: &str, title: &str) -> raul::tui::app::MilestoneSummary {
    raul::tui::app::MilestoneSummary {
        id: id.into(),
        title: title.into(),
        lifecycle: "draft".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".into(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }
}

fn bl(id: &str, title: &str) -> BacklogLine {
    BacklogLine {
        id: id.into(),
        title: title.into(),
        priority: "normal".into(),
        status: "open".into(),
        resolution: String::new(),
        preview: String::new(),
        ..Default::default()
    }
}

/// #3: pressing S now reaches open_sort_rebind() via the modes::normal
/// dispatcher. Pre-fix the resolved action was dropped on the floor.
#[test]
fn s_keypress_opens_sort_rebind_modal() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "a"), ms("02", "b")]);
    app.select_lane(Lane::Milestones);
    let r = MpRunner::new().unwrap();
    assert!(!app.sort_rebind_open());
    // Three terminal encodings of "Shift+s" — all must work.
    for key in [
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::SHIFT),
        KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT),
        KeyEvent::new(KeyCode::Char('S'), KeyModifiers::empty()),
    ] {
        let actions = modes::normal::handle_key(key, &app);
        assert!(
            actions.iter().any(|a| matches!(a, Action::OpenSortRebind)),
            "S keypress ({key:?}) must dispatch OpenSortRebind; got {actions:?}"
        );
    }
    apply_action(&mut app, &r, Action::OpenSortRebind).unwrap();
    assert!(app.sort_rebind_open(), "OpenSortRebind must open the menu");
}

/// #4: milestones cycle = Id → Title → Priority → Stage → Created → Updated → Id
/// (column order; 6 stops in M205).
#[test]
fn milestones_cycle_sort_follows_column_order() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "a"), ms("02", "b")]);
    app.select_lane(Lane::Milestones);
    let r = MpRunner::new().unwrap();
    let expected = [
        SortKey::Title,
        SortKey::Priority,
        SortKey::Stage,
        SortKey::Created,
        SortKey::Updated,
        SortKey::Id,
    ];
    for want in expected {
        apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
        assert_eq!(
            app.lane_sort_key(Lane::Milestones),
            want,
            "cycle landed on wrong key"
        );
    }
}

/// #4: backlog cycle = Id → Title → Priority → Status → Created →
/// ResolvedAt → Id (M205 extended to 6 stops to match the
/// M182 S3 / M187 cycle, plus M205's Created + ResolvedAt additions).
#[test]
fn backlog_cycle_sort_follows_column_order() {
    let mut app = App::new();
    app.load_backlog(vec![bl("TW-01", "a"), bl("TW-02", "b")]);
    app.select_lane(Lane::Backlog);
    let r = MpRunner::new().unwrap();
    let expected = [
        SortKey::Title,
        SortKey::Priority,
        SortKey::Status,
        SortKey::Created,
        SortKey::ResolvedAt,
        SortKey::Id,
    ];
    for want in expected {
        apply_action(&mut app, &r, Action::CycleSortNext).unwrap();
        assert_eq!(
            app.lane_sort_key(Lane::Backlog),
            want,
            "cycle landed on wrong key"
        );
    }
}

/// #4: Title sort is alphabetical, case-insensitive.
#[test]
fn title_sort_is_alphabetical_case_insensitive() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "gamma"), ms("02", "Alpha"), ms("03", "beta")]);
    app.select_lane(Lane::Milestones);
    app.lane_sort_key.insert(Lane::Milestones, SortKey::Title);
    let titles: Vec<&str> = app
        .visible_milestones()
        .iter()
        .map(|m| m.title.as_str())
        .collect();
    assert_eq!(titles, vec!["Alpha", "beta", "gamma"]);
}

/// #1 + #2: footer layout — M199 flip puts globals on top, per-tab
/// on bottom, both centered. The per-tab row is sourced from
/// `Keybinds::footer_per_tab`, so it now carries the lane-specific
/// filter / search / hide-done / sort / cycle / annotate tokens
/// (the lane-conditional items that the M187 globals row claimed
/// but didn't actually fire on every tab).
#[test]
fn footer_flips_lines_and_centers() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "a")]);
    app.select_lane(Lane::Milestones);
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let h = buf.area().height;
    let mut globals_row = String::new();
    let mut per_tab_row = String::new();
    for x in 0..buf.area().width {
        globals_row.push_str(buf[(x, h - 2)].symbol());
        per_tab_row.push_str(buf[(x, h - 1)].symbol());
    }
    // M199: globals on top (h-2), per-tab on bottom (h-1).
    assert!(
        globals_row.contains(":quit") && globals_row.contains(":help"),
        "globals (top) must carry quit/help; got {globals_row:?}"
    );
    assert!(
        per_tab_row.contains(":filter") && per_tab_row.contains(":search"),
        "per-tab (bottom) must carry lane-specific list keys (filter/search); got {per_tab_row:?}"
    );

    // Centering: globals line must not start at column 0 (some leading
    // space precedes the first key glyph).
    let trimmed_start = globals_row.trim_start();
    let lead = globals_row.len() - trimmed_start.len();
    assert!(
        lead >= 4,
        "globals must be centered (leading pad ≥ 4); got lead={lead}, row={globals_row:?}"
    );
}

/// #1: flash message also centers.
#[test]
fn flash_message_is_centered() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.set_flash_message("Saved");
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let h = buf.area().height;
    let mut row = String::new();
    for x in 0..buf.area().width {
        row.push_str(buf[(x, h - 2)].symbol());
    }
    let trimmed = row.trim_start();
    let lead = row.len() - trimmed.len();
    assert!(
        lead >= 4,
        "flash must be centered; got lead={lead}, row={row:?}"
    );
}
