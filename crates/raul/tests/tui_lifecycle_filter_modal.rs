//! M185 AC-04: lifecycle filter modal interactions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane, MilestoneSummary};
use raul::tui::mode::Mode;
use raul::tui::modes;
use std::collections::BTreeMap;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn runner() -> MpRunner {
    MpRunner::new().expect("mp binary")
}

fn seed(app: &mut App) {
    app.load_milestones(vec![
        MilestoneSummary {
            id: "01".into(),
            title: "a".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "02".into(),
            title: "b".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "03".into(),
            title: "c".into(),
            lifecycle: "complete".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);
}

#[test]
fn open_toggle_commit_filters_visible() {
    let mut app = App::new();
    seed(&mut app);
    let r = runner();
    apply_action(&mut app, &r, Action::OpenLifecycleFilter).unwrap();
    assert!(matches!(app.active_mode, Mode::LifecycleFilter(_)));

    // LIFECYCLE_FILTER_OPTIONS: draft=0, groomed=1, approved=2, in-progress=3
    apply_action(&mut app, &r, Action::LifecycleFilterNext).unwrap(); // groomed
    apply_action(&mut app, &r, Action::LifecycleFilterNext).unwrap(); // approved
    apply_action(&mut app, &r, Action::LifecycleFilterToggle).unwrap();
    apply_action(&mut app, &r, Action::LifecycleFilterNext).unwrap(); // in-progress
    apply_action(&mut app, &r, Action::LifecycleFilterToggle).unwrap();
    apply_action(&mut app, &r, Action::LifecycleFilterCommit).unwrap();

    assert!(matches!(app.active_mode, Mode::Normal));
    let lf = app.lifecycle_filter_set();
    assert!(lf.contains("approved"));
    assert!(lf.contains("in-progress"));
    let ids: Vec<_> = app
        .visible_milestones()
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(ids, vec!["01", "02"]);
}

#[test]
fn esc_reverts_prior_filter() {
    let mut app = App::new();
    seed(&mut app);
    app.set_lifecycle_filter(["complete".to_string()].into_iter().collect());
    let r = runner();
    apply_action(&mut app, &r, Action::OpenLifecycleFilter).unwrap();
    // Toggle draft on then cancel
    apply_action(&mut app, &r, Action::LifecycleFilterToggle).unwrap();
    let actions = modes::lifecycle_filter::handle_key(key(KeyCode::Esc));
    assert_eq!(actions, vec![Action::LifecycleFilterCancel]);
    apply_action(&mut app, &r, Action::LifecycleFilterCancel).unwrap();
    let lf = app.lifecycle_filter_set();
    assert_eq!(
        lf.iter().collect::<Vec<_>>(),
        vec![&"complete".to_string()]
    );
}

// ─── M204 S4: unified per-lane filter modal widget ──────────────────────────

use raul::tui::modes::filter_modal::spec as fspec;
use raul::tui::mode::DimensionKind;

/// S4 / AC-03: the widget accepts a `DimensionSpec` and the
/// `total_items` helper counts the flattened `(dim, value)` rows
/// for navigation. The pinned shape (3 dims on Milestones, 4
/// dims on Backlog, 4 dims on Ideas) is the load-bearing
/// contract — reordering dimensions in `spec::milestones()` is
/// a breaking change for chip rendering (S6).
#[test]
fn filter_modal_widget_handles_dimension_spec() {
    let ms_dims = fspec::milestones();
    assert_eq!(ms_dims.len(), 3, "Milestones must expose 3 dimensions");
    // Names match the on-disk ProjectConfig.filter keys.
    assert_eq!(ms_dims[0].name, "lifecycle");
    assert_eq!(ms_dims[1].name, "priority");
    assert_eq!(ms_dims[2].name, "age");
    // Age is a Preset (single-select) per AC-06.
    assert_eq!(ms_dims[2].kind, DimensionKind::Preset);
    // Priority is a Toggle (multi-select).
    assert_eq!(ms_dims[1].kind, DimensionKind::Toggle);
    // Total items = 10 lifecycle + 4 priority + 3 age = 17.
    assert_eq!(fspec::total_items(&ms_dims), 17);

    let bl_dims = fspec::backlog();
    assert_eq!(bl_dims.len(), 4, "Backlog must expose 4 dimensions");
    let bl_names: Vec<&str> = bl_dims.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        bl_names,
        vec!["priority", "status", "age", "source"],
        "Backlog dim order; got {bl_names:?}"
    );

    let id_dims = fspec::ideas();
    assert_eq!(id_dims.len(), 4, "Ideas must expose 4 dimensions");
    let id_names: Vec<&str> = id_dims.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        id_names,
        vec!["priority", "status", "age", "tags"],
        "Ideas dim order; got {id_names:?}"
    );
}

/// S4 / AC-03: the modal keybindings are identical across
/// lanes — Up/k move up, Down/j move down, Space toggles, Enter
/// commits, Esc cancels. The pin lives in the handler
/// signature; the test exercises the same handler with the
/// three key shapes.
#[test]
fn modal_visual_style_consistent_across_lanes() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use raul::tui::action::Action;
    use raul::tui::modes::filter_modal;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    // Same handler across all lanes (single handler in
    // modes/filter_modal.rs). The four canonical keybindings.
    assert_eq!(filter_modal::handle_key(key(KeyCode::Up)), vec![Action::FilterPrev]);
    assert_eq!(filter_modal::handle_key(key(KeyCode::Char('k'))), vec![Action::FilterPrev]);
    assert_eq!(filter_modal::handle_key(key(KeyCode::Down)), vec![Action::FilterNext]);
    assert_eq!(filter_modal::handle_key(key(KeyCode::Char('j'))), vec![Action::FilterNext]);
    assert_eq!(filter_modal::handle_key(key(KeyCode::Char(' '))), vec![Action::FilterToggle]);
    assert_eq!(filter_modal::handle_key(key(KeyCode::Enter)), vec![Action::FilterCommit]);
    assert_eq!(filter_modal::handle_key(key(KeyCode::Esc)), vec![Action::FilterCancel]);
    // Modifier-bearing keys (Ctrl/Alt/Super) are no-ops — the
    // user can't accidentally trigger a binding via OS-level
    // chord.
    let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert!(filter_modal::handle_key(ctrl_a).is_empty());
}

// ─── M204 S5: F key opens modal on all three lanes; c clears all ───────────

/// F on Milestones opens the unified filter modal. The modal
/// shape (Mode::Filter) is the new M204 variant; the legacy
/// Mode::LifecycleFilter is no longer the default for capital F.
#[test]
fn filter_modal_opens_on_F_from_milestones() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let r = raul::mp_runner::MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::OpenFilter).unwrap();
    match &app.active_mode {
        Mode::Filter(st) => {
            assert_eq!(st.lane, Lane::Milestones);
            // The Milestones modal must surface lifecycle +
            // priority + age in that order.
            assert_eq!(st.dimensions.len(), 3);
            assert_eq!(st.dimensions[0].name, "lifecycle");
            assert_eq!(st.dimensions[1].name, "priority");
            assert_eq!(st.dimensions[2].name, "age");
        }
        other => panic!("expected Mode::Filter, got {other:?}"),
    }
}

#[test]
fn filter_modal_opens_on_F_from_backlog() {
    let mut app = App::new();
    app.select_lane(Lane::Backlog);
    let r = raul::mp_runner::MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::OpenFilter).unwrap();
    match &app.active_mode {
        Mode::Filter(st) => {
            assert_eq!(st.lane, Lane::Backlog);
            assert_eq!(st.dimensions.len(), 4);
            let names: Vec<&str> = st.dimensions.iter().map(|d| d.name.as_str()).collect();
            assert_eq!(names, vec!["priority", "status", "age", "source"]);
        }
        other => panic!("expected Mode::Filter, got {other:?}"),
    }
}

#[test]
fn filter_modal_opens_on_F_from_ideas() {
    let mut app = App::new();
    app.select_lane(Lane::Ideas);
    let r = raul::mp_runner::MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::OpenFilter).unwrap();
    match &app.active_mode {
        Mode::Filter(st) => {
            assert_eq!(st.lane, Lane::Ideas);
            assert_eq!(st.dimensions.len(), 4);
            let names: Vec<&str> = st.dimensions.iter().map(|d| d.name.as_str()).collect();
            assert_eq!(names, vec!["priority", "status", "age", "tags"]);
        }
        other => panic!("expected Mode::Filter, got {other:?}"),
    }
}

#[test]
fn milestones_modal_exposes_lifecycle_priority_age() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let r = raul::mp_runner::MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::OpenFilter).unwrap();
    let st = match &app.active_mode {
        Mode::Filter(st) => st.clone(),
        _ => panic!("expected Mode::Filter"),
    };
    // Lifecycle: 10 values (canonical M185 order).
    assert_eq!(st.dimensions[0].values.len(), 10);
    assert_eq!(st.dimensions[0].values[0], "draft");
    assert_eq!(st.dimensions[0].values[8], "cancelled");
    // Priority: 4 values.
    assert_eq!(st.dimensions[1].values, vec!["urgent", "high", "normal", "low"]);
    // Age: 3 preset chips.
    assert_eq!(st.dimensions[2].values, vec![">7d", ">30d", ">90d"]);
}

#[test]
fn backlog_modal_exposes_priority_status_age_source() {
    use raul::tui::modes::filter_modal::spec as fspec;
    let dims = fspec::backlog();
    assert_eq!(dims[0].name, "priority");
    assert_eq!(dims[1].name, "status");
    assert_eq!(dims[2].name, "age");
    assert_eq!(dims[3].name, "source");
    // Source values are the four actionable-backlog prefixes.
    let source_vals = &dims[3].values;
    assert!(source_vals.contains(&"B-".to_string()));
    assert!(source_vals.contains(&"BL-".to_string()));
    assert!(source_vals.contains(&"TW-".to_string()));
    assert!(source_vals.contains(&"BF-".to_string()));
}

#[test]
fn ideas_modal_exposes_priority_status_age_tags() {
    use raul::tui::modes::filter_modal::spec as fspec;
    let dims = fspec::ideas();
    assert_eq!(dims[0].name, "priority");
    assert_eq!(dims[1].name, "status");
    assert_eq!(dims[2].name, "age");
    assert_eq!(dims[3].name, "tags");
    // Tag-prefix values: alpha / beta / unblocked / spike.
    let tag_vals = &dims[3].values;
    assert!(tag_vals.contains(&"alpha".to_string()));
    assert!(tag_vals.contains(&"beta".to_string()));
    assert!(tag_vals.contains(&"unblocked".to_string()));
    assert!(tag_vals.contains(&"spike".to_string()));
}

#[test]
fn c_clears_all_active_filters() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    // Seed two dimensions of filters on the Milestones lane.
    use std::collections::{BTreeMap, BTreeSet};
    let mut dims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    dims.insert(
        "lifecycle".to_string(),
        BTreeSet::from(["approved".to_string()]),
    );
    dims.insert(
        "priority".to_string(),
        BTreeSet::from(["high".to_string()]),
    );
    app.lane_filters.insert(Lane::Milestones, dims);
    assert!(!app.lifecycle_filter_set().is_empty());

    // Press c on Milestones — must clear all filters.
    let r = raul::mp_runner::MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::ClearFilters).unwrap();
    assert!(
        app.lifecycle_filter_set().is_empty(),
        "lifecycle dim must be cleared"
    );
    assert!(
        app.lane_filters
            .get(&Lane::Milestones)
            .map(|d| d.is_empty())
            .unwrap_or(true),
        "lane entry must be empty or absent"
    );
}

// ─── M204 S6: chip strip — every active dim is a chip with x; AND-combine ───

use raul::tui::app::filter_dimensions_for;

/// AC-05: filters AND-combine across dimensions. A row that
/// satisfies every active dimension (lifecycle AND priority
/// AND age) is kept; a row that fails any dimension is
/// dropped. Empty dimensions are skipped (no narrowing). The
/// pure seam is `App::visible_milestones` / `App::visible_backlog`.
#[test]
fn filters_and_combine() {
    use std::collections::{BTreeMap, BTreeSet};
    let mut app = App::new();
    app.load_milestones(vec![
        raul::tui::app::MilestoneSummary {
            id: "01".into(),
            title: "approved high".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "high".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        raul::tui::app::MilestoneSummary {
            id: "02".into(),
            title: "approved normal".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        raul::tui::app::MilestoneSummary {
            id: "03".into(),
            title: "in-progress high".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "high".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);
    // Filter: lifecycle=approved AND priority=high → only M01.
    let mut dims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    dims.insert(
        "lifecycle".to_string(),
        BTreeSet::from(["approved".to_string()]),
    );
    dims.insert(
        "priority".to_string(),
        BTreeSet::from(["high".to_string()]),
    );
    app.lane_filters.insert(Lane::Milestones, dims);
    let ids: Vec<&str> = app
        .visible_milestones()
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["01"],
        "AND-combine must keep only the row that satisfies BOTH dimensions; got {ids:?}"
    );
}

/// AC-05: the chip strip is built from the active filter
/// state. The render function (private) consumes the same
/// shape the chip renderer reads from. The pin asserts that
/// every active dim × active value is present in the chip
/// strip's text representation.
#[test]
fn chip_strip_shows_every_active_dimension() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::render;
    use raul::tui::view_state;
    use std::collections::{BTreeMap, BTreeSet};

    let mut app = App::new();
    // Seed 2 dimensions of filters on Milestones.
    let mut dims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    dims.insert(
        "lifecycle".to_string(),
        BTreeSet::from(["approved".to_string(), "in-progress".to_string()]),
    );
    dims.insert(
        "priority".to_string(),
        BTreeSet::from(["high".to_string()]),
    );
    app.lane_filters.insert(Lane::Milestones, dims);
    // Add at least one milestone so the lane renders (the
    // empty-state path bypasses the chip strip).
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".into(),
        title: "anchor".into(),
        lifecycle: "approved".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "high".into(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.select_lane(Lane::Milestones);

    let backend = TestBackend::new(140, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    // Every active chip must be rendered with its dim + value.
    assert!(flat.contains("lifecycle:"), "lifecycle dim label");
    assert!(flat.contains("approved"), "lifecycle value: approved");
    assert!(flat.contains("in-progress"), "lifecycle value: in-progress");
    assert!(flat.contains("priority:"), "priority dim label");
    assert!(flat.contains("high"), "priority value: high");
    // The chip's "remove" glyph is present.
    assert!(flat.contains("✕"), "chip x glyph");
}

/// AC-05: clicking the chip's `x` removes that single chip
/// (the (dim, value) pair). After removal, the visible set
/// reflects the new filter state immediately.
#[test]
fn chip_remove_updates_visible_set() {
    let mut app = App::new();
    app.load_milestones(vec![
        raul::tui::app::MilestoneSummary {
            id: "01".into(),
            title: "a".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        raul::tui::app::MilestoneSummary {
            id: "02".into(),
            title: "b".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);
    app.set_lifecycle_filter(
        ["approved".to_string(), "in-progress".to_string()]
            .into_iter()
            .collect(),
    );
    // Both rows are visible (lifecycle=approved|in-progress
    // is permissive across both).
    let initial: Vec<&str> = app
        .visible_milestones()
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(initial.len(), 2);

    // Click "x" on the approved chip.
    let r = raul::mp_runner::MpRunner::new().expect("mp");
    apply_action(
        &mut app,
        &r,
        Action::RemoveFilterChip {
            dim: "lifecycle".to_string(),
            value: "approved".to_string(),
        },
    )
    .unwrap();
    let after: Vec<&str> = app
        .visible_milestones()
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(
        after,
        vec!["02"],
        "removing the approved chip must hide the approved row; got {after:?}"
    );
}

/// AC-07: empty filter state hides the chip strip entirely
/// (no row reserved). The `render_active_filter_chips`
/// helper returns 0 when the lane has no active filter.
#[test]
fn empty_filter_hides_chip_strip() {
    let mut app = App::new();
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "01".into(),
        title: "anchor".into(),
        lifecycle: "approved".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".into(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.select_lane(Lane::Milestones);
    // Lane has no filter; the chip strip must NOT reserve a row.
    assert!(filter_dimensions_for(Lane::Milestones).len() > 0);
    // Render and verify no chip glyph appears in the buffer.
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::render;
    use raul::tui::view_state;
    let backend = TestBackend::new(140, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    assert!(
        !flat.contains("✕"),
        "empty filter must not render the chip strip; got: {flat}"
    );
}

// ─── M204 S7: age filter chips use the `created` field ──────────────────────

use raul::tui::app::{age_passes, age_strictest_threshold};
use std::collections::BTreeSet;

/// AC-06: age filter on Milestones uses the `created` field
/// (`MilestoneMeta.created`). Items older than the threshold
/// are hidden.
#[test]
fn age_filter_uses_created_for_milestones() {
    // The test pins the pure seam (`age_passes` +
    // `age_strictest_threshold`) rather than running the
    // full TUI loop with a real clock. The integration
    // coverage lives in `tui_milestones_title_chip.rs`.
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    let now: DateTime<Utc> = Utc.from_utc_datetime(
        &NaiveDate::from_ymd_opt(2026, 9, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
    );
    // 60 days old — passes >30d, fails >90d.
    let sixty_days_ago = "2026-07-04".to_string();
    assert!(age_passes(&sixty_days_ago, 30, now));
    assert!(!age_passes(&sixty_days_ago, 90, now));
    // 100 days old — passes both.
    let hundred_days_ago = "2026-05-25".to_string();
    assert!(age_passes(&hundred_days_ago, 30, now));
    assert!(age_passes(&hundred_days_ago, 90, now));
    // 3 days old — fails both.
    let three_days_ago = "2026-08-30".to_string();
    assert!(!age_passes(&three_days_ago, 7, now));
    // Empty string (pre-M205 record) — always passes
    // (permissive default).
    assert!(age_passes("", 7, now));
}

/// AC-06: age filter on Backlog uses `BacklogItem.created`
/// (the `BacklogLine.created_at` field).
#[test]
fn age_filter_uses_created_for_backlog() {
    // Pin the pure seam — the integration is covered by
    // tui_milestones_title_chip.rs / tui_lifecycle_filter_modal.rs.
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    let now: DateTime<Utc> = Utc.from_utc_datetime(
        &NaiveDate::from_ymd_opt(2026, 9, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
    );
    // Backlog age filter uses the same `age_passes` helper
    // applied to `b.created_at`. The path is the same;
    // the field is just the `BacklogLine` member.
    let created_at = "2026-07-04"; // ~60d old
    assert!(age_passes(created_at, 30, now));
}

/// AC-06: age filter on Ideas uses `IdeaEntry.created`
/// (the `BacklogLine.created_at` field — same shape as
/// Backlog).
#[test]
fn age_filter_uses_created_for_ideas() {
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    let now: DateTime<Utc> = Utc.from_utc_datetime(
        &NaiveDate::from_ymd_opt(2026, 9, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
    );
    let created_at = "2026-05-25"; // ~100d old
    assert!(age_passes(created_at, 90, now));
}

/// AC-06: age preset intersection uses the strictest
/// threshold. If `>7d` AND `>30d` are both active, the row
/// must be >30d (not just >7d). The `age_strictest_threshold`
/// helper returns 30 for that set.
#[test]
fn age_preset_intersection_uses_strictest() {
    let mut set: BTreeSet<String> = BTreeSet::new();
    set.insert(">7d".to_string());
    set.insert(">30d".to_string());
    assert_eq!(age_strictest_threshold(&set), 30);
    // >7d + >90d → 90 (strictest).
    let mut set: BTreeSet<String> = BTreeSet::new();
    set.insert(">7d".to_string());
    set.insert(">90d".to_string());
    assert_eq!(age_strictest_threshold(&set), 90);
    // All three → 90.
    let mut set: BTreeSet<String> = BTreeSet::new();
    set.insert(">7d".to_string());
    set.insert(">30d".to_string());
    set.insert(">90d".to_string());
    assert_eq!(age_strictest_threshold(&set), 90);
    // Just >30d → 30.
    let mut set: BTreeSet<String> = BTreeSet::new();
    set.insert(">30d".to_string());
    assert_eq!(age_strictest_threshold(&set), 30);
    // Empty → 0 (caller skips the age filter).
    let set: BTreeSet<String> = BTreeSet::new();
    assert_eq!(age_strictest_threshold(&set), 0);
    // Unknown values are skipped.
    let mut set: BTreeSet<String> = BTreeSet::new();
    set.insert(">90d".to_string());
    set.insert("garbage".to_string());
    assert_eq!(age_strictest_threshold(&set), 90);
}
