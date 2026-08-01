use raul::tui::app::App;

// M91 S2 removed sidebar_width / dragging_gutter from App. The three tests
// that asserted those fields are gone; `tab_bar_focused_defaults_true` stays
// M167: tab_bar_focused removed; tab bar is always visual chrome (no focus toggle).
// is still App state.
//
// S5 / S9 will replace these with the new contract tests:
//   * S5: click on a tab label selects that lane and highlights the active tab.
//   * S6/S9: narrow-width compact_label overflow + horizontal scroll hit targets.

#[test]
fn tab_bar_focused_defaults_true() {
    // M167: tab_bar_focused removed; the field was never App-public after
    // this milestone. This test name is preserved for trace; the contract
    // (tab bar is always visual chrome) is enforced via the keybind surface
    // (Tab / Shift+Tab → next_lane / previous_lane) instead.
    let _app = App::new();
}

#[test]
fn app_new_is_constructible_post_s2() {
    // Smoke test: ensure App::new() still works without the deleted fields.
    let _app = App::new();
}
