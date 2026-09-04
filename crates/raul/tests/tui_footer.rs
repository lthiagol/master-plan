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
    // chrome.rs::footer_for must delegate to the keybinds footer
    // methods rather than returning hardcoded legend strings. M199
    // consolidated the four pre-M199 footer methods
    // (`footer_overview` / `footer_list` / `footer_content` /
    // `footer_settings`) into a single per-(lane, content_state)
    // table: `Keybinds::footer_per_tab`. The renderer now routes
    // every non-modal footer through that single source of truth.
    // M217: the composition moved one level down —
    // `chrome::footer_for` delegates to
    // `view_state::footer_per_tab_text`, which is the single
    // source of truth for both the footer text and the footer
    // height. The `footer_per_tab` call it wraps now lives in
    // view_state.rs, so the delegation is asserted across both
    // files.
    let content = fs::read_to_string(render_dir().join("chrome.rs")).unwrap();
    assert!(
        content.contains("footer_per_tab_text(app)"),
        "footer_for must build its per-tab text from view_state::footer_per_tab_text()"
    );
    let view_state_src =
        fs::read_to_string(render_dir().parent().unwrap().join("view_state.rs")).unwrap();
    assert!(
        view_state_src.contains("footer_per_tab("),
        "footer_per_tab_text must build its per-tab text from app.keybinds.footer_per_tab()"
    );

    // No hardcoded key-legend fragment should survive in the renderer.
    // (`:?` is mentioned in M199's design-decision comments, so we
    // don't pin the substring `?:help` — only the hardcoded
    // rendering surfaces.)
    let forbidden = [
        "hl:lanes",
        "1-7:jump",
        "↑↓:inbox",
        "↑↓:move",
        "↑↓:scroll",
        "footer_overview()",
        "footer_list()",
        "footer_content(",
    ];
    for pat in &forbidden {
        assert!(
            !content.contains(pat),
            "chrome.rs must not hardcode key legend '{pat}' (post-M199)"
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
    // M199: the help overlay routes through
    // `app.keybinds.help_entries_grouped(active_lane)` so the
    // overlay and the footer share a single source of truth.
    assert!(
        content.contains("help_entries_grouped("),
        "help overlay must be generated from app.keybinds.help_entries_grouped()"
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
