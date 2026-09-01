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
    assert!(app.milestone_filter.contains("approved"));
    assert!(app.milestone_filter.contains("in-progress"));
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
    app.milestone_filter.insert("complete".into());
    let r = runner();
    apply_action(&mut app, &r, Action::OpenLifecycleFilter).unwrap();
    // Toggle draft on then cancel
    apply_action(&mut app, &r, Action::LifecycleFilterToggle).unwrap();
    let actions = modes::lifecycle_filter::handle_key(key(KeyCode::Esc));
    assert_eq!(actions, vec![Action::LifecycleFilterCancel]);
    apply_action(&mut app, &r, Action::LifecycleFilterCancel).unwrap();
    assert_eq!(
        app.milestone_filter.iter().collect::<Vec<_>>(),
        vec![&"complete".to_string()]
    );
}
