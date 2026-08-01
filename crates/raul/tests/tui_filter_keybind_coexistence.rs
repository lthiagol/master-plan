//! M185 AC-10: lowercase f vs capital F coexistence.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::app::{App, ContentState, Lane};
use raul::tui::keybinds::Keybinds;
use raul::tui::mode::Mode;

#[test]
fn capital_f_opens_lifecycle_filter_on_milestones() {
    let kb = Keybinds::default();
    let key = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::SHIFT);
    // shift_char('f') stores Char('f')+SHIFT; format may differ.
    assert_eq!(
        kb.resolve(&key),
        Some(Action::OpenLifecycleFilter),
        "capital F must resolve to OpenLifecycleFilter"
    );
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.open_lifecycle_filter();
    assert!(matches!(app.active_mode, Mode::LifecycleFilter(_)));
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
