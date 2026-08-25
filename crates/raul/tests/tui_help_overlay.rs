//! M199 AC-06: help overlay (?) groups entries under two
//! headings — `Per-lane` (active lane's keys, surfaced first
//! per Q-03) and `Global` (the six universal bindings). The
//! per-lane group is sourced from `Keybinds::footer_per_tab` so
//! the help overlay and the footer share a single source of
//! truth. No duplication: the per-lane group must not include
//! the six globals keys; the Global group must not include
//! lane-conditional items.
//!
//! AC-07: modal footers are unchanged. This test also pins
//! that the help overlay continues to be reachable from each
//! mode (the modals still surface a help-friendly footer or
//! the user can press `?` from anywhere).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, ContentState, Lane};
use raul::tui::keybinds::Keybinds;
use raul::tui::render;
use raul::tui::view_state;

fn render_full(app: &mut App, w: u16, h: u16) -> String {
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

fn open_help(app: &mut App) {
    // The `?` key opens help. We don't dispatch through the
    // modes handler here — `toggle_help` is the public surface.
    app.toggle_help();
}

#[test]
fn help_overlay_renders_per_lane_and_global_groups() {
    // M199 AC-06: the help overlay groups entries under
    // `Per-lane` and `Global` headings. On Milestones, the
    // per-lane group carries the lane-specific list keys
    // (filter, search, hide-done, sort, cycle, annotate).
    let mut app = App::new();
    app.load_milestones(vec![]);
    app.select_lane(Lane::Milestones);
    open_help(&mut app);
    let s = render_full(&mut app, 100, 40);

    // Both group headings must be present.
    assert!(
        s.contains("Per-lane"),
        "help overlay must show a `Per-lane` heading; got:\n{s}"
    );
    assert!(
        s.contains("Global"),
        "help overlay must show a `Global` heading; got:\n{s}"
    );
    // The per-lane heading should name the active lane so
    // the user knows which group they're looking at.
    assert!(
        s.contains("Milestones"),
        "help overlay should name the active lane in the per-lane heading; got:\n{s}"
    );
}

#[test]
fn per_lane_group_contains_lane_specific_keys_without_globals() {
    // M199 D-04: the per-lane group is the *delta* from the
    // global baseline. Globals (quit, help, refresh, move, go,
    // lanes) must not be duplicated in the per-lane group.
    let mut app = App::new();
    app.load_milestones(vec![]);
    app.select_lane(Lane::Milestones);
    open_help(&mut app);
    let s = render_full(&mut app, 100, 40);

    // Per-lane group should include the six lane-specific
    // list keys for Milestones.
    for needle in ["filter", "search", "hide-done", "sort", "cycle", "annotate"] {
        assert!(
            s.contains(needle),
            "help overlay per-lane group must include `{needle}`; got:\n{s}"
        );
    }
    // The `Quit` / `Help` / `Refresh` / `Move up` / `Move down` /
    // `Select / drill in` / `Previous lane` / `Next lane` labels
    // are Global group members — they appear once, in the
    // Global section, not duplicated in the per-lane section.
    // We can't directly check the absence in the per-lane
    // section from a flat string, so we instead verify that
    // the source of truth (footer_per_tab) does not include
    // them, which is what drives the per-lane group render.
    let per_tab = app
        .keybinds
        .footer_per_tab(Lane::Milestones, ContentState::List, false, false);
    for forbidden in ["quit", "help", "refresh", "move", "go", "lanes"] {
        assert!(
            !per_tab.to_lowercase().contains(forbidden),
            "footer_per_tab(Milestones, List) must not include global token `{forbidden}`; got={per_tab:?}"
        );
    }
}

#[test]
fn global_group_lists_six_universal_bindings() {
    // M199 AC-06 sub-pin: the Global group must list the
    // same six bindings as the footer globals line: quit,
    // help, lane-switch, move, go, refresh. The label of the
    // global entries is what the user sees in the overlay.
    let kb = Keybinds::default();
    let (_global, _per_lane) = kb.help_entries_grouped(Lane::Milestones, ContentState::List);
    let labels: Vec<&str> = _global.iter().map(|e| e.label).collect();
    for expected in [
        "Quit",
        "Help",
        "Refresh",
        "Move up",
        "Move down",
        "Select / drill in",
        "Previous lane",
        "Next lane",
    ] {
        assert!(
            labels.contains(&expected),
            "Global group must include `{expected}`; got labels={labels:?}"
        );
    }
}

#[test]
fn active_lane_group_first_per_q03() {
    // M199 Q-03: the active lane's group is shown first so
    // the most-relevant keys are at the top of the overlay.
    // We assert this by checking that `Per-lane` appears
    // before `Global` in the rendered help overlay.
    let mut app = App::new();
    app.load_milestones(vec![]);
    app.select_lane(Lane::Milestones);
    open_help(&mut app);
    let s = render_full(&mut app, 100, 40);
    let per_lane_idx = s.find("Per-lane").expect("Per-lane heading missing");
    let global_idx = s.find("Global").expect("Global heading missing");
    assert!(
        per_lane_idx < global_idx,
        "Per-lane must appear before Global in the rendered overlay (Q-03); per_lane_idx={per_lane_idx} global_idx={global_idx}"
    );
}

#[test]
fn path_lane_help_overlay_says_no_lane_specific_keys() {
    // M199: when the active lane has no per-tab keys
    // (Path), the per-lane group renders an empty-state line
    // so the user learns the empty state is normal.
    let mut app = App::new();
    app.select_lane(Lane::Path);
    open_help(&mut app);
    let s = render_full(&mut app, 100, 40);
    assert!(
        s.contains("Per-lane") && s.contains("Path"),
        "Path help must still show the per-lane group heading; got:\n{s}"
    );
    // The empty-state copy is a single placeholder line.
    assert!(
        s.contains("no lane-specific keys") || s.to_lowercase().contains("see global"),
        "Path help must show the empty-state placeholder; got:\n{s}"
    );
}

#[test]
fn settings_lane_help_overlay_surfaces_save_and_cancel() {
    // M199 review fix (F-04 + F-09): the Settings per-tab string
    // uses bracket markers (`[Save (s)] [Cancel (Esc)]`) which
    // `per_tab_help_entries` cannot parse. The help overlay's
    // Settings case was therefore empty (showed the
    // "no lane-specific keys" placeholder), which would mislead
    // the user pressing `?` on Settings. The fix: special-case
    // Settings in `help_entries_grouped` to emit `Save` and
    // `Cancel` directly. This test pins that behavior.
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    open_help(&mut app);
    let s = render_full(&mut app, 100, 40);
    assert!(
        s.contains("Save"),
        "Settings help must show `Save` in the per-lane section; got:\n{s}"
    );
    assert!(
        s.contains("Cancel"),
        "Settings help must show `Cancel` in the per-lane section; got:\n{s}"
    );
    assert!(
        s.contains('s'),
        "Settings help must surface the `s` key for Save; got:\n{s}"
    );
    // The empty-state placeholder must NOT appear for Settings
    // (Settings is not a read-mostly lane like Path).
    assert!(
        !s.contains("no lane-specific keys"),
        "Settings help must not show the empty-state placeholder; got:\n{s}"
    );
}

#[test]
fn help_overlay_groups_change_when_lane_changes() {
    // M199: switching the active lane must change the
    // per-lane group's keys. We assert the per-tab source
    // (and thus the overlay render) reflects the new lane.
    let mut app = App::new();
    app.load_milestones(vec![]);
    let per_tab_milestones =
        app.keybinds
            .footer_per_tab(Lane::Milestones, ContentState::List, false, false);
    let per_tab_backlog =
        app.keybinds
            .footer_per_tab(Lane::Backlog, ContentState::List, false, false);
    assert!(
        per_tab_milestones.contains("annotate") && per_tab_backlog.contains("annotate"),
        "both list lanes surface annotate; milestones={per_tab_milestones:?} backlog={per_tab_backlog:?}"
    );
    // Detail rows differ: MilestonesDetail has `[/]:section`
    // and `p/n:item`; BacklogDetail does not.
    let per_tab_ms_detail = app.keybinds.footer_per_tab(
        Lane::Milestones,
        ContentState::MilestoneDetail,
        false,
        false,
    );
    let per_tab_bl_detail =
        app.keybinds
            .footer_per_tab(Lane::Backlog, ContentState::BacklogDetail, false, false);
    assert!(
        per_tab_ms_detail.contains("section"),
        "Milestones detail should have a section token; got={per_tab_ms_detail:?}"
    );
    assert!(
        !per_tab_bl_detail.contains("section"),
        "Backlog detail should NOT have a section token; got={per_tab_bl_detail:?}"
    );
}

#[test]
fn help_entries_grouped_passes_active_content_state() {
    // M199 review fix: the help overlay must pass `app.content`
    // to `help_entries_grouped` so the per-lane group reflects
    // the active (lane, content_state) pair. Opening help from
    // `MilestoneDetail` should surface the detail-specific keys
    // (annotate/menu/section/item/approve), not the list keys
    // (filter/search/...).
    let kb = Keybinds::default();
    let (_global, per_lane_list) = kb.help_entries_grouped(Lane::Milestones, ContentState::List);
    let (_global, per_lane_detail) =
        kb.help_entries_grouped(Lane::Milestones, ContentState::MilestoneDetail);
    let list_labels: Vec<&str> = per_lane_list.iter().map(|(l, _)| l.as_str()).collect();
    let detail_labels: Vec<&str> = per_lane_detail.iter().map(|(l, _)| l.as_str()).collect();
    // List view surfaces the lane-conditional items; detail
    // view does not.
    assert!(
        list_labels.contains(&"filter") && list_labels.contains(&"search"),
        "list per-lane must include `filter` and `search`; got={list_labels:?}"
    );
    assert!(
        detail_labels.contains(&"section") && detail_labels.contains(&"item"),
        "detail per-lane must include `section` and `item`; got={detail_labels:?}"
    );
    // The two groups should be distinct.
    assert_ne!(
        list_labels, detail_labels,
        "list and detail per-lane groups must differ"
    );
}
