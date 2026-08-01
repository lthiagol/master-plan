use raul::tui::app::App;
use raul::tui::mode::{Mode, ReviewMenuState};

fn review_menu_state(app: &App) -> &ReviewMenuState {
    match &app.active_mode {
        Mode::ReviewMenu(s) => s,
        other => panic!("expected Mode::ReviewMenu, got {other:?}"),
    }
}

#[test]
fn review_menu_items_populated() {
    let mut app = App::new();
    app.open_review_menu();
    // M172 S6: the menu grew a "Set dependency" item — 5 total
    // (Approve / Block / Unblock / Request grooming / Set dependency).
    assert_eq!(review_menu_state(&app).items.len(), 5);
    assert!(matches!(app.active_mode, Mode::ReviewMenu(_)));
    assert_eq!(review_menu_state(&app).selected, 0);
}

#[test]
fn review_menu_can_be_closed() {
    let mut app = App::new();
    app.open_review_menu();
    app.close_review_menu();
    assert!(!matches!(app.active_mode, Mode::ReviewMenu(_)));
}

#[test]
fn review_menu_move_up_down() {
    let mut app = App::new();
    app.open_review_menu();
    app.move_down();
    assert_eq!(review_menu_state(&app).selected, 1);
    app.move_up();
    assert_eq!(review_menu_state(&app).selected, 0);
}

#[test]
fn review_menu_selected_action() {
    let mut app = App::new();
    app.open_review_menu();
    let action = app.selected_review_action();
    assert_eq!(action, Some("Approve milestone"));
    app.move_down();
    let action = app.selected_review_action();
    assert_eq!(action, Some("Block milestone"));
}
