//! M138: footer + help overlay are generated from `Keybinds`, not hardcoded
//! in the renderer. These tests replace the pre-M138 source-scan assertions
//! that *required* hardcoded key legends in `render/` — the exact thing
//! AC-05 removes. They now assert the opposite: the renderer delegates to
//! `app.keybinds`, and no multi-key legend string is baked into `render/`.

use std::fs;
use std::path::PathBuf;

fn render_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("render")
}

#[test]
fn footer_is_generated_from_keybinds() {
    // chrome.rs::footer_for must delegate to the keybinds footer methods
    // rather than returning hardcoded legend strings. M167 removed the
    // tab-bar / content focus split, so chrome no longer calls
    // `kb.footer_tab_bar()`; only the content-pane footer builder is
    // dispatched today.
    let content = fs::read_to_string(render_dir().join("chrome.rs")).unwrap();
    assert!(
        content.contains("kb.footer_content("),
        "footer_for must build its content-pane text from app.keybinds"
    );

    // No hardcoded key-legend fragment should survive in the renderer.
    let forbidden = [
        "hl:lanes",
        "1-7:jump",
        "↑↓:inbox",
        "↑↓:move",
        "↑↓:scroll",
        "?:help",
    ];
    for pat in &forbidden {
        assert!(
            !content.contains(pat),
            "chrome.rs must not hardcode key legend '{pat}' (AC-05)"
        );
    }
}

#[test]
fn help_overlay_is_generated_from_keybinds() {
    let content = fs::read_to_string(render_dir().join("overlays.rs")).unwrap();

    assert!(
        content.contains("fn render_help_overlay"),
        "render_help_overlay function must exist"
    );
    assert!(
        content.contains("app.keybinds.help_entries()"),
        "help overlay must be generated from app.keybinds.help_entries()"
    );

    // The pre-M138 hardcoded key legends (a literal key glued to a label)
    // must be gone — the key portion now comes from `keybinds`.
    let forbidden = [
        "↑/k",
        "↓/j",
        "←/h",
        "→/l",
        "A      Create annotation",
        "r      Resolve selected annotation",
        "R      Reopen selected annotation",
        "p      Request approval",
        "m      Open review menu",
    ];
    for pat in &forbidden {
        assert!(
            !content.contains(pat),
            "overlays.rs must not hardcode key legend '{pat}' (AC-05)"
        );
    }
}
