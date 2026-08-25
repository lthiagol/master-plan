//! M185 AC-06: g applies Grooming preset on Milestones only.

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};
use raul::tui::progress::GROOMING_PRESET;

#[test]
fn grooming_preset_sets_three_lifecycles() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let r = MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::ApplyGroomingPreset).unwrap();
    for lc in GROOMING_PRESET {
        assert!(
            app.milestone_filter.contains(*lc),
            "missing {lc} in {:?}",
            app.milestone_filter
        );
    }
    assert_eq!(app.milestone_filter.len(), 3);
}

#[test]
fn grooming_preset_noop_on_other_lanes() {
    let mut app = App::new();
    let r = MpRunner::new().expect("mp");
    for lane in [
        Lane::Overview,
        Lane::Path,
        Lane::Backlog,
        Lane::Ideas,
        Lane::Watch,
        Lane::Settings,
    ] {
        app.select_lane(lane);
        app.milestone_filter.clear();
        apply_action(&mut app, &r, Action::ApplyGroomingPreset).unwrap();
        assert!(app.milestone_filter.is_empty(), "lane {lane:?} must no-op");
    }
}
