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
