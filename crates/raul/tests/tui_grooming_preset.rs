//! M185 AC-06: g applies Grooming preset on Milestones only.
//! M204: the preset writes through the new `lane_filters` model
//! (lifecycle dimension on the Milestones lane).

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
    let lf = app.lifecycle_filter_set();
    for lc in GROOMING_PRESET {
        assert!(lf.contains(*lc), "missing {lc} in {lf:?}");
    }
    assert_eq!(lf.len(), 3);
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
        Lane::Autopilot,
        Lane::Settings,
    ] {
        app.select_lane(lane);
        app.set_lifecycle_filter(Default::default());
        apply_action(&mut app, &r, Action::ApplyGroomingPreset).unwrap();
        assert!(
            app.lifecycle_filter_set().is_empty(),
            "lane {lane:?} must no-op"
        );
    }
}
