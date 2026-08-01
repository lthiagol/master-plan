use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Read every TUI source file into a single string. M136 spread the
/// review-menu code across `app.rs`, `tui/mod.rs`, `tui/mode.rs`,
/// `tui/modes/review_menu.rs`, and `tui/runner_helpers.rs`; the static
/// checks below look at the union so any one place moving the surface
/// doesn't break the gate test.
fn tui_source() -> String {
    let tui_dir = workspace_root()
        .join("crates")
        .join("raul")
        .join("src")
        .join("tui");
    let mut s = String::new();
    if tui_dir.is_dir() {
        fn walk(dir: &std::path::Path, out: &mut String) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push_str(&std::fs::read_to_string(&path).unwrap());
                    out.push('\n');
                }
            }
        }
        walk(&tui_dir, &mut s);
    }
    s
}

// Static analysis: verify ReviewMenu view exists in the app module
#[test]
fn review_menu_view_exists_in_app() {
    let combined = tui_source();
    // M136: review-menu state lives inside `Mode::ReviewMenu(_)` rather
    // than on a `show_review_menu: bool` field. The methods below stay;
    // only the *field* check changed shape.
    assert!(
        combined.contains("Mode::ReviewMenu"),
        "review menu state should live in `Mode::ReviewMenu`"
    );
    assert!(
        combined.contains("fn open_review_menu"),
        "open_review_menu method should exist"
    );
    assert!(
        combined.contains("fn close_review_menu"),
        "close_review_menu method should exist"
    );
    assert!(
        combined.contains("fn selected_review_action"),
        "selected_review_action should exist"
    );
}

// Static analysis: verify review menu keybinding exists. Pre-M136 the
// gate read `event.rs` (which had `ReviewMenuOpen`); M136 moved keying
// into `tui/modes/normal.rs` and the action into `tui/action.rs`. Look
// for the action variant and the key binding wherever they now live.
#[test]
fn review_menu_keybinding_exists() {
    let combined = tui_source();
    assert!(
        combined.contains("OpenReviewMenu"),
        "OpenReviewMenu action variant should exist"
    );
    assert!(
        combined.contains("KeyCode::Char('m')"),
        "'m' keybinding for review menu should exist"
    );
}

// Static analysis: verify review menu handler exists. M136 moved the
// dispatcher into `tui/modes/review_menu.rs` and the `mp` shell-out into
// `tui/runner_helpers.rs`; the contract checks below look across the
// whole `tui/` tree.
#[test]
fn review_menu_handler_exists() {
    let combined = tui_source();
    assert!(
        combined.contains("ReviewMenu"),
        "ReviewMenu handling should exist in some tui module"
    );
    assert!(
        combined.contains("execute_review_action"),
        "execute_review_action should exist"
    );
    assert!(
        combined.contains("run_raw_allow_failure"),
        "execute_review_action must use run_raw_allow_failure to surface mp errors (F-13)"
    );
    assert!(
        combined.contains("parse_mp_ok_response"),
        "execute_review_action must parse mp responses without swallowing failures"
    );
    assert!(
        combined.contains("flash_message"),
        "review action failures should set flash_message for TUI display"
    );
}

// Static analysis: verify review menu render exists
#[test]
fn review_menu_render_exists() {
    let combined = tui_source();
    assert!(
        combined.contains("render_review_menu_overlay"),
        "review menu render function should exist"
    );
    assert!(
        combined.contains("Review Actions"),
        "popup title should exist"
    );
}

// Static analysis: verify review action types
#[test]
fn review_actions_include_expected_commands() {
    let combined = tui_source();
    assert!(
        combined.contains("Approve milestone"),
        "Approve action should exist"
    );
    assert!(
        combined.contains("Block milestone"),
        "Block action should exist"
    );
    assert!(
        combined.contains("Unblock milestone"),
        "Unblock action should exist"
    );
    assert!(
        combined.contains("Request grooming"),
        "Request grooming action should exist"
    );
}
