//! M185 AC-10: lowercase f vs capital F coexistence.
//! M204: capital F now opens the unified per-lane filter modal
//! (Milestones / Backlog / Ideas). The M185-era `OpenLifecycleFilter`
/// action remains in the enum (for the legacy single-dim modal
/// reachable via `Action::OpenLifecycleFilter` programmatically) but
/// no default keybinding resolves to it. This test pins the new
/// M204 contract: capital F → `Action::OpenFilter`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, ContentState, Lane};
use raul::tui::keybinds::Keybinds;
use raul::tui::mode::Mode;

#[test]
fn capital_f_opens_unified_filter_modal() {
    // M204: capital F resolves to the unified filter action
    // (was `OpenLifecycleFilter` in M185).
    let kb = Keybinds::default();
    let key = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::SHIFT);
    assert_eq!(
        kb.resolve(&key),
        Some(Action::OpenFilter),
        "capital F must resolve to OpenFilter (M204 unified modal); got {:?}",
        kb.resolve(&key)
    );
    // The `Action::OpenFilter` action opens the unified
    // `Mode::Filter` (not the legacy `Mode::LifecycleFilter`).
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let r = raul::mp_runner::MpRunner::new().expect("mp");
    apply_action(&mut app, &r, Action::OpenFilter).unwrap();
    assert!(matches!(app.active_mode, Mode::Filter(_)));
}

#[test]
fn lowercase_f_is_toggle_filter() {
    let kb = Keybinds::default();
    let key = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::empty());
    assert_eq!(kb.resolve(&key), Some(Action::ToggleFilter));
}

#[test]
fn toggle_filter_flips_open_only_in_annotation_thread() {
    let mut app = App::new();
    app.content = ContentState::AnnotationThread;
    assert!(!app.open_only);
    app.toggle_filter();
    assert!(app.open_only);
}
