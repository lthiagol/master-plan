//! M183 S3: loading a config/profile that references an unknown
//! keybind name (e.g. a Tweaks-era `TWEAK_OPEN`) must not panic —
//! the key is skipped with a diagnostic and defaults remain usable.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::App;
use raul::tui::keybinds::Keybinds;
use raul::tui::render;
use raul::tui::view_state;

#[test]
fn unknown_keybind_name_is_diagnostic_not_panic() {
    let profile = r#"
TWEAK_OPEN = "t"
quit = "q"
"#;
    let (diags, kb) = Keybinds::load_from_profile_toml(profile);
    assert!(
        diags
            .iter()
            .any(|d| { d.field == "TWEAK_OPEN" && d.message.contains("unknown keybind action") }),
        "expected unknown-keybind diagnostic for TWEAK_OPEN; got {diags:?}"
    );
    // Defaults still usable — quit still bound (profile override kept).
    assert!(
        !kb.quit.is_empty(),
        "quit binding must survive unknown keys"
    );
    assert_eq!(
        kb.help,
        Keybinds::default().help,
        "unrelated defaults must remain intact"
    );
}

#[test]
fn unknown_keybind_does_not_panic_on_render() {
    let profile = "TWEAK_OPEN = \"t\"\n";
    let (diags, kb) = Keybinds::load_from_profile_toml(profile);
    assert!(!diags.is_empty());

    let mut app = App::new();
    app.keybinds = kb;

    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    // If we got here without panic, S3 is satisfied.
    let buf = terminal.backend().buffer();
    assert!(buf.area().height >= 2);
}
