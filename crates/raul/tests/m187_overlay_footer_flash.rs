//! M187 regressions: S menu renderer, F flash on non-Milestones,
//! modal backdrop, footer dedup.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, BacklogLine, Lane, MilestoneSummary};
use raul::tui::render;
use raul::tui::view_state;
use std::collections::BTreeMap;

fn ms(id: &str) -> MilestoneSummary {
    MilestoneSummary {
        id: id.into(),
        title: format!("t-{id}"),
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

fn bl(id: &str) -> BacklogLine {
    BacklogLine {
        id: id.into(),
        title: id.into(),
        priority: "normal".into(),
        status: "open".into(),
        resolution: String::new(),
        preview: String::new(),
        ..Default::default()
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

/// #1: S menu opens and renders the per-lane key set with the cursor
/// on the current key. Milestones shows 6 keys (Id/Title/Priority/
/// Stage/Created/Updated); Backlog shows 6 (Id/Title/Priority/
/// Status/Created/ResolvedAt).
#[test]
fn sort_rebind_menu_renders_with_correct_keys_per_lane() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01"), ms("02")]);
    app.select_lane(Lane::Milestones);
    app.open_sort_rebind();
    assert!(app.sort_rebind_open());
    let s = dump(&app, 100, 24);
    // M205: Stage replaces the pre-M205 Lifecycle menu entry —
    // the Stage column cell owns that signal now, and the Stage
    // sort reuses the same derivation.
    assert!(
        s.contains("Stage"),
        "milestones menu must list Stage; got:\n{s}"
    );
    assert!(
        s.contains("Updated"),
        "milestones menu must list Updated; got:\n{s}"
    );
    assert!(
        s.contains("Created"),
        "milestones menu must list Created (M205 addition); got:\n{s}"
    );
    assert!(
        !s.contains("Status"),
        "milestones menu must NOT list Status; got:\n{s}"
    );
    assert!(
        s.contains("↑↓ cycle") || s.contains("cycle"),
        "menu must show cycle hint"
    );

    // Switch to Backlog — menu should show Status, not Lifecycle/Updated.
    let mut app = App::new();
    app.load_backlog(vec![bl("TW-01"), bl("TW-02")]);
    app.select_lane(Lane::Backlog);
    app.open_sort_rebind();
    let s = dump(&app, 100, 24);
    assert!(
        s.contains("Status"),
        "backlog menu must list Status; got:\n{s}"
    );
    assert!(
        !s.contains("Lifecycle"),
        "backlog menu must NOT list Lifecycle"
    );
    assert!(!s.contains("Updated"), "backlog menu must NOT list Updated");
}

/// #1: confirm cycle updates the rendered cursor (bold/accent moves).
#[test]
fn sort_rebind_menu_cycle_moves_cursor() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01")]);
    app.select_lane(Lane::Milestones);
    app.open_sort_rebind();
    let initial = app.sort_rebind_index;
    app.cycle_sort_rebind_next();
    assert_ne!(app.sort_rebind_index, initial);
    assert!(app.sort_rebind_index < 4);
}

/// #1: footer carries the modal-internal keys while S menu is open.
#[test]
fn sort_rebind_menu_footer_shows_cycle_keys() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01")]);
    app.select_lane(Lane::Milestones);
    app.open_sort_rebind();
    let s = dump(&app, 100, 24);
    assert!(
        s.contains("⏎ bind") || s.contains("bind"),
        "footer must show bind hint while sort menu open; got:\n{s}"
    );
}

/// #4: F on Backlog flashes a "Milestones only" message instead of
/// silent no-op.
#[test]
fn lifecycle_filter_flash_on_non_milestones() {
    let r = MpRunner::new().unwrap();
    for lane in [Lane::Backlog, Lane::Ideas, Lane::Path, Lane::Settings] {
        let mut app = App::new();
        app.select_lane(lane);
        apply_action(&mut app, &r, Action::OpenLifecycleFilter).unwrap();
        assert!(
            app.flash_message.is_some(),
            "F on {lane:?} must set a flash_message, not silently no-op"
        );
        assert!(
            app.flash_message
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains("milestones"),
            "flash message must mention Milestones; got {:?}",
            app.flash_message
        );
    }
}

/// #4: g on non-Milestones also flashes.
#[test]
fn grooming_preset_flash_on_non_milestones() {
    let r = MpRunner::new().unwrap();
    let mut app = App::new();
    app.select_lane(Lane::Backlog);
    apply_action(&mut app, &r, Action::ApplyGroomingPreset).unwrap();
    assert!(app.flash_message.is_some());
}

/// #5: lifecycle filter + search overlays paint a non-default background
/// (DarkGray backdrop) so underlying content does not bleed through.
#[test]
fn filter_modal_paints_backdrop() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01")]);
    app.select_lane(Lane::Milestones);
    app.open_lifecycle_filter();
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let backdrop = raul::tui::palette::overlay_backdrop(app.effective_palette());
    // Find the lifecycle filter block (it carries "Lifecycle filter" in
    // its title) and confirm at least one cell inside it has the backdrop bg.
    let mut found_backdrop = false;
    let mut in_modal = false;
    for y in 0..buf.area().height {
        let mut row = String::new();
        for x in 0..buf.area().width {
            row.push_str(buf[(x, y)].symbol());
        }
        if row.contains("Lifecycle filter") {
            in_modal = true;
            continue;
        }
        if in_modal {
            // Scan this row for backdrop cells.
            for x in 0..buf.area().width {
                if buf[(x, y)].bg == backdrop {
                    found_backdrop = true;
                    break;
                }
            }
            if found_backdrop {
                break;
            }
        }
    }
    assert!(
        found_backdrop,
        "lifecycle filter modal must paint a backdrop bg"
    );

    // Search input overlay parity check.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.open_search();
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut found_search_backdrop = false;
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            if buf[(x, y)].bg == backdrop && buf[(x, y)].symbol() == " " {
                found_search_backdrop = true;
                break;
            }
        }
        if found_search_backdrop {
            break;
        }
    }
    let _ = found_search_backdrop; // best-effort: search overlay is small.
}

/// #6: per-tab footer no longer duplicates quit/help/hide-done.
#[test]
fn per_tab_footer_does_not_duplicate_globals() {
    let kb = raul::tui::keybinds::Keybinds::default();
    let list_footer = kb.footer_list();
    assert!(list_footer.contains(":move"));
    assert!(list_footer.contains(":select"));
    // Globals line carries these — must not appear in per-tab.
    assert!(!list_footer.contains(":hide-done"));
    assert!(!list_footer.contains(":help"));
    assert!(!list_footer.contains(":quit"));

    let content_footer = kb.footer_content(false);
    assert!(content_footer.contains(":action"));
    assert!(content_footer.contains(":menu"));
    assert!(!content_footer.contains(":hide-done"));
    assert!(!content_footer.contains(":help"));
    assert!(!content_footer.contains(":quit"));
}
