//! M243: the Settings `ui.theme` picker.
//!
//! Covers the picker's model and its rendering: the 7-row expansion
//! (6 named palettes + `Default (mocha)`), the 6-role swatch drawn in
//! each palette, the keyboard highlight, and the live apply that
//! repaints the frame in the highlighted theme.
//!
//! The swatch is deliberately **6 roles**, not 9 — `focus_ring`,
//! `surface_1` and `surface_2` are background layers, not text colors,
//! and read as mud in a one-line strip. `SWATCH_ROLES` pins the six.

use std::collections::BTreeMap;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::Terminal;

use raul::theme;
use raul::tui::app::{App, Lane};
use raul::tui::mode::{SettingsFocus, SettingsState};
use raul::tui::modes::settings::schema::SettingsSchema;
use raul::tui::modes::settings::theme_picker::{self, ThemePicker};
use raul::tui::modes::settings::SETTINGS_KEYS;
use raul::tui::render;
use raul::tui::view_state;

// --- helpers ---------------------------------------------------------------

/// The `ui.theme` row's index in the flat key list (3rd entry: after
/// `ui.color` and `ui.icons`).
fn theme_idx() -> usize {
    SETTINGS_KEYS
        .iter()
        .position(|(_, k)| *k == theme_picker::THEME_KEY)
        .expect("ui.theme must be in SETTINGS_KEYS")
}

/// A minimal but real `mp config schema` payload, built through the
/// parser so the test exercises the same path the lane uses at
/// lane-open rather than hand-assembling the typed struct.
fn build_test_schema() -> SettingsSchema {
    let raw = serde_json::json!({
        "$schema_version": "1.0",
        "keys": [
            { "key": "ui.color", "type": "bool", "default": "true",
              "description": "ANSI color toggle." },
            { "key": "ui.icons", "type": "choice", "default": "unicode",
              "allowed": ["none", "ascii", "unicode"], "description": "Icon set." },
            { "key": "ui.theme", "type": "choice", "default": "mocha",
              "allowed": ["mocha", "macchiato", "frappe", "latte", "dracula", "alucard"],
              "description": "Theme palette name." },
            { "key": "ui.hide_done", "type": "bool", "default": "false",
              "description": "Hide done milestones." }
        ]
    });
    SettingsSchema::from_json(raw.to_string().as_bytes()).expect("schema parses")
}

fn settings_app(saved_theme: &str) -> App {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    let config = serde_json::json!({
        "ui": { "color": true, "icons": "unicode", "theme": saved_theme,
                "hide_done": false, "show_autopilot_tab": false },
    });
    app.settings = Some(SettingsState {
        config,
        schema: Some(build_test_schema()),
        selected_idx: theme_idx(),
        focus: SettingsFocus::Fields,
        edit: None,
        staged_edits: BTreeMap::new(),
        schema_warning: None,
        theme: ThemePicker::new(saved_theme),
    });
    app
}

/// Focus the `ui.theme` row and expand the picker.
fn app_with_open_picker(saved_theme: &str) -> App {
    let mut app = settings_app(saved_theme);
    let state = app.settings.as_mut().unwrap();
    state.selected_idx = theme_idx();
    state.theme.expand();
    app
}

fn render_full(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    buffer_text(terminal.backend().buffer())
}

fn buffer_text(buffer: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        other => panic!("expected an RGB color, got {other:?}"),
    }
}

/// Is `(r, g, b)` present as a foreground anywhere in the buffer?
fn has_fg(buffer: &Buffer, want: (u8, u8, u8)) -> bool {
    (0..buffer.area().height).any(|y| {
        (0..buffer.area().width).any(|x| buffer[(x, y)].fg == Color::Rgb(want.0, want.1, want.2))
    })
}

// --- model -----------------------------------------------------------------

#[test]
fn picker_expands_ui_theme_into_seven_rows() {
    let rows = theme_picker::rows();
    assert_eq!(rows.len(), 7, "6 named palettes + Default");
    let labels: Vec<String> = rows.iter().map(|r| r.label()).collect();
    assert_eq!(
        labels,
        vec![
            "latte",
            "frappe",
            "macchiato",
            "mocha",
            "dracula",
            "alucard",
            "Default (mocha)",
        ]
    );
    // No monochrome row.
    assert!(!labels.iter().any(|l| l == "monochrome"));
}

#[test]
fn picker_rows_carry_a_one_line_description() {
    for row in theme_picker::rows() {
        let d = row.description();
        assert!(!d.is_empty(), "{} has no description", row.label());
        assert!(
            !d.contains('\n'),
            "{} description must be one line",
            row.label()
        );
    }
    // Spot-check the new palette is described, not a placeholder.
    let alucard = theme_picker::row_for_name("alucard").unwrap();
    assert!(
        theme_picker::row_at(alucard)
            .description()
            .contains("Alucard"),
        "alucard row must be described"
    );
}

#[test]
fn swatch_is_six_roles() {
    assert_eq!(theme_picker::SWATCH_ROLES.len(), 6);
    let sw = theme_picker::swatch(&theme::ALUCARD);
    assert_eq!(sw.len(), 6);
    assert_eq!(sw[0], theme::ALUCARD.accent);
    assert_eq!(sw[1], theme::ALUCARD.success);
    assert_eq!(sw[2], theme::ALUCARD.warn);
    assert_eq!(sw[3], theme::ALUCARD.danger);
    assert_eq!(sw[4], theme::ALUCARD.dim);
    assert_eq!(sw[5], theme::ALUCARD.foreground);
}

#[test]
fn keyboard_highlight_moves_within_the_picker() {
    let mut app = app_with_open_picker("mocha");
    let start = app.settings.as_ref().unwrap().theme.cursor();
    assert_eq!(
        app.settings.as_ref().unwrap().theme.preview_name(),
        "mocha",
        "the picker opens on the saved theme"
    );

    app.move_down();
    assert_eq!(
        app.settings.as_ref().unwrap().theme.cursor(),
        start + 1,
        "Down moves the highlight one row"
    );

    app.move_up();
    app.move_up();
    assert_eq!(
        app.settings.as_ref().unwrap().theme.cursor(),
        start - 1,
        "Up moves the highlight back up"
    );

    // The flat key list must not move while the picker owns input.
    assert_eq!(
        app.settings.as_ref().unwrap().selected_idx,
        theme_idx(),
        "the picker owns Up/Down; the flat list must not scroll"
    );
}

#[test]
fn cursor_clamps_within_the_seven_rows() {
    let mut app = app_with_open_picker("latte");
    for _ in 0..20 {
        app.move_up();
    }
    assert_eq!(app.settings.as_ref().unwrap().theme.cursor(), 0);
    for _ in 0..20 {
        app.move_down();
    }
    assert_eq!(app.settings.as_ref().unwrap().theme.cursor(), 6);
}

// --- rendering -------------------------------------------------------------

#[test]
fn expanded_picker_renders_every_palette_row() {
    let app = app_with_open_picker("mocha");
    let out = render_full(&app, 120, 44);
    for name in [
        "latte",
        "frappe",
        "macchiato",
        "mocha",
        "dracula",
        "alucard",
    ] {
        assert!(out.contains(name), "picker row for {name} missing:\n{out}");
    }
    assert!(out.contains("Default (mocha)"), "reset row missing:\n{out}");
}

#[test]
fn collapsed_picker_renders_no_palette_rows() {
    let app = settings_app("mocha");
    let out = render_full(&app, 120, 44);
    // The flat list is dense; the picker must not leak into it.
    assert!(
        !out.contains("alucard"),
        "alucard row must only appear when the picker is expanded:\n{out}"
    );
    assert!(
        !out.contains("Default (mocha)"),
        "reset row must only appear when the picker is expanded:\n{out}"
    );
    assert!(
        out.contains("ui.theme"),
        "the ui.theme key row itself must still render:\n{out}"
    );
}

#[test]
fn swatch_draws_each_palette_in_its_own_colors() {
    // Alucard is the newest palette and its colors are unique to it, so
    // its swatch colors must be the ones on screen. The app's live
    // palette is mocha here — proving the swatch is drawn in the ROW's
    // palette, not the active one.
    let app = app_with_open_picker("mocha");
    let backend = TestBackend::new(120, 44);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();

    for role_color in [
        theme::ALUCARD.accent,
        theme::ALUCARD.success,
        theme::ALUCARD.warn,
        theme::ALUCARD.danger,
        theme::ALUCARD.dim,
        theme::ALUCARD.foreground,
    ] {
        assert!(
            has_fg(buffer, rgb(role_color)),
            "alucard swatch must paint its own {role_color:?} somewhere in the buffer"
        );
    }
}

#[test]
fn highlighted_row_carries_the_cursor_marker() {
    let app = app_with_open_picker("mocha");
    let out = render_full(&app, 120, 44);
    // Exactly one `▶` cursor in the picker, and the `ui.theme` parent
    // row shows the `▼` expand marker instead of competing for it.
    let cursors = out.matches('▶').count();
    assert_eq!(cursors, 1, "expected exactly one cursor row:\n{out}");
    assert!(
        out.contains('▼'),
        "the expanded ui.theme row must show the ▼ expand marker:\n{out}"
    );
}

#[test]
fn picker_renders_on_a_narrow_pane_without_panicking() {
    // The picker adds 7 rows to a 42-row list; tight panes must degrade
    // by scrolling, not by panicking. The list smooth-scrolls to keep
    // the *picker cursor* in view, so the `ui.theme` parent row scrolls
    // off on a short pane — the highlighted palette row is what must
    // survive.
    for (w, h) in [(80u16, 24u16), (60, 20), (40, 12)] {
        let app = app_with_open_picker("mocha");
        let out = render_full(&app, w, h);
        assert!(
            out.contains("mocha"),
            "the highlighted palette row must survive at {w}x{h}:\n{out}"
        );
        assert!(
            out.contains('█'),
            "the swatch must render at {w}x{h}:\n{out}"
        );
        // The parent row is in the window whenever there is room for it.
        if h >= 24 {
            assert!(
                out.contains("ui.theme"),
                "ui.theme row must survive at {w}x{h}:\n{out}"
            );
        }
    }
}

// --- mouse -----------------------------------------------------------------

/// Click at the center of the hit rect registered for picker row `n`
/// and return whether the highlight landed on that row.
fn click_picker_row(app: &mut App, width: u16, height: u16, n: usize) -> bool {
    let view = view_state::compute_view(app, ratatui::layout::Rect::new(0, 0, width, height));
    let id = format!("settings.theme.{n}");
    let hit = view
        .list_item_rects
        .iter()
        .find(|h| h.id == id)
        .unwrap_or_else(|| panic!("no hit rect for {id}"))
        .clone();
    let x = hit.rect.x + hit.rect.width / 2;
    let y = hit.rect.y;
    let consumed = raul::tui::mouse::handle_dispatch(app, &view, x, y, false);
    consumed && app.settings.as_ref().unwrap().theme.cursor() == n
}

#[test]
fn each_picker_row_registers_a_hit_rect() {
    let app = app_with_open_picker("mocha");
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 120, 44));
    let ids: Vec<&str> = view.list_item_rects.iter().map(|h| h.id.as_str()).collect();
    for n in 0..theme_picker::ROW_COUNT {
        assert!(
            ids.contains(&format!("settings.theme.{n}").as_str()),
            "row {n} must register a hit rect; got {ids:?}"
        );
    }
    assert_eq!(
        view.list_item_rects.len(),
        theme_picker::ROW_COUNT,
        "the Settings lane publishes picker rects only"
    );
}

#[test]
fn collapsed_picker_publishes_no_hit_rects() {
    let app = settings_app("mocha");
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 120, 44));
    assert!(
        view.list_item_rects.is_empty(),
        "a collapsed picker must not be clickable: {:?}",
        view.list_item_rects
    );
}

/// The hit rects must line up with the rows that are actually drawn —
/// a rect one row off would move the highlight to the wrong palette,
/// which is worse than no mouse support at all.
#[test]
fn hit_rects_line_up_with_the_rendered_rows() {
    let app = app_with_open_picker("mocha");
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 120, 44));
    let backend = TestBackend::new(120, 44);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let v = view_state::compute_view(&app, f.area());
            render::render(f, &app, &v);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();

    for hit in &view.list_item_rects {
        let n: usize = hit
            .id
            .strip_prefix("settings.theme.")
            .unwrap()
            .parse()
            .unwrap();
        let label = theme_picker::row_at(n).label();
        let line: String = (0..buffer.area().width)
            .map(|x| buffer[(x, hit.rect.y)].symbol())
            .collect();
        assert!(
            line.contains(&label),
            "hit rect for row {n} points at y={} which renders {line:?}, not {label:?}",
            hit.rect.y
        );
    }
}

#[test]
fn click_moves_the_highlight() {
    let mut app = app_with_open_picker("mocha");
    let start = app.settings.as_ref().unwrap().theme.cursor();
    assert_ne!(start, 0);
    assert!(
        click_picker_row(&mut app, 120, 44, 0),
        "clicking row 0 must move the highlight there"
    );
    assert_eq!(app.settings.as_ref().unwrap().theme.preview_name(), "latte");
}

#[test]
fn hover_moves_the_highlight() {
    let mut app = app_with_open_picker("mocha");
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 120, 44));
    let hit = view
        .list_item_rects
        .iter()
        .find(|h| h.id == "settings.theme.5")
        .expect("alucard row rect")
        .clone();
    // Hover arrives as a Moved event; the runner routes it through
    // the same dispatcher a click uses.
    let consumed =
        raul::tui::mouse::handle_dispatch(&mut app, &view, hit.rect.x + 2, hit.rect.y, false);
    assert!(consumed, "hover over a picker row must move the highlight");
    assert_eq!(
        app.settings.as_ref().unwrap().theme.preview_name(),
        "alucard"
    );
}

#[test]
fn click_off_the_picker_is_ignored() {
    let mut app = app_with_open_picker("mocha");
    let before = app.settings.as_ref().unwrap().theme.cursor();
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, 120, 44));
    // y=0 is the tab bar, well above the Settings list.
    let consumed = raul::tui::mouse::handle_dispatch(&mut app, &view, 10, 0, false);
    assert!(!consumed, "a click outside the picker must not be consumed");
    assert_eq!(
        app.settings.as_ref().unwrap().theme.cursor(),
        before,
        "the highlight must not move"
    );
}

// --- status preview (AC-05) ------------------------------------------------

#[test]
fn status_preview_lists_every_lifecycle_role() {
    assert_eq!(
        theme_picker::STATUS_PREVIEW.len(),
        6,
        "in-progress / done / ready / blocked / accent / dim"
    );
    let labels: Vec<&str> = theme_picker::STATUS_PREVIEW
        .iter()
        .map(|(l, _)| *l)
        .collect();
    assert_eq!(
        labels,
        vec!["in-progress", "done", "ready", "blocked", "accent", "dim"]
    );
}

#[test]
fn status_preview_row_renders_under_the_picker() {
    let app = app_with_open_picker("mocha");
    let out = render_full(&app, 140, 44);
    for label in ["in-progress", "done", "ready", "blocked", "accent", "dim"] {
        assert!(
            out.contains(label),
            "status preview chip {label:?} missing:\n{out}"
        );
    }
    // The preview sits below every palette row.
    let alucard = out.find("alucard").expect("alucard row renders");
    let blocked = out.find("blocked").expect("status preview renders");
    assert!(
        blocked > alucard,
        "the status preview must be the last row of the expansion"
    );
}

#[test]
fn status_preview_paints_in_the_highlighted_palette() {
    // Highlight alucard and check the preview repaints in ITS colors.
    let mut app = app_with_open_picker("mocha");
    app.settings
        .as_mut()
        .unwrap()
        .theme
        .set_cursor(theme_picker::row_for_name("alucard").expect("alucard row exists"));
    app.apply_theme_preview();

    let backend = TestBackend::new(140, 44);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let v = view_state::compute_view(&app, f.area());
            render::render(f, &app, &v);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();

    // Alucard's success #14710A and danger #CB3A2A are unique to it, so
    // finding both proves the preview (not mocha) is what got painted.
    assert!(
        has_fg(buffer, rgb(theme::ALUCARD.success)),
        "status preview must use the highlighted palette's success color"
    );
    assert!(
        has_fg(buffer, rgb(theme::ALUCARD.danger)),
        "status preview must use the highlighted palette's danger color"
    );
}

#[test]
fn status_preview_absent_when_the_picker_is_collapsed() {
    let app = settings_app("mocha");
    let out = render_full(&app, 140, 44);
    assert!(
        !out.contains("in-progress"),
        "the status preview belongs to the expansion only:\n{out}"
    );
}

// --- live apply (AC-06) ----------------------------------------------------

/// Route a key through the Settings handler the way the runner does:
/// `handle_key` → actions → `App::move_*`.
fn press(app: &mut App, code: ratatui::crossterm::event::KeyCode) {
    use ratatui::crossterm::event::{KeyEvent, KeyModifiers};
    use raul::tui::action::Action;
    let actions =
        raul::tui::modes::settings::handle_key(KeyEvent::new(code, KeyModifiers::NONE), app);
    for a in actions {
        match a {
            Action::Up => app.move_up(),
            Action::Down => app.move_down(),
            other => panic!("unexpected action {other:?} for {code:?}"),
        }
    }
}

fn palette_name(app: &App) -> &'static str {
    app.effective_palette().name
}

#[test]
fn arrows_live_apply_the_highlighted_palette() {
    raul::config::set_color_enabled(true);
    let mut app = app_with_open_picker("mocha");
    assert_eq!(palette_name(&app), "mocha");

    for _ in 0..5 {
        press(&mut app, ratatui::crossterm::event::KeyCode::Down);
    }
    // Five rows down from mocha is the reset row, which previews mocha.
    // Walk to alucard explicitly instead.
    let mut app = app_with_open_picker("mocha");
    let target = theme_picker::row_for_name("alucard").unwrap();
    let cur = app.settings.as_ref().unwrap().theme.cursor();
    for _ in 0..(target as i32 - cur as i32) {
        press(&mut app, ratatui::crossterm::event::KeyCode::Down);
    }
    assert_eq!(
        palette_name(&app),
        "alucard",
        "the highlight must set App::palette immediately"
    );
}

#[test]
fn vim_keys_live_apply_too() {
    raul::config::set_color_enabled(true);
    let mut app = app_with_open_picker("mocha");
    // `k` / `j` are the default aliases for up / down, so the picker
    // must honor them exactly as it honors the arrows. `theme::ALL` is
    // alphabetical, so one `j` from mocha lands on dracula.
    press(&mut app, ratatui::crossterm::event::KeyCode::Char('j'));
    assert_eq!(palette_name(&app), "dracula");
    press(&mut app, ratatui::crossterm::event::KeyCode::Char('k'));
    assert_eq!(palette_name(&app), "mocha");
}

#[test]
fn mouse_live_applies_the_highlighted_palette() {
    raul::config::set_color_enabled(true);
    let mut app = app_with_open_picker("mocha");
    let n = theme_picker::row_for_name("dracula").unwrap();
    assert!(click_picker_row(&mut app, 120, 44, n));
    assert_eq!(palette_name(&app), "dracula");
}

#[test]
fn the_next_frame_repaints_in_the_new_palette() {
    raul::config::set_color_enabled(true);
    let mut app = app_with_open_picker("mocha");
    let n = theme_picker::row_for_name("alucard").unwrap();
    assert!(click_picker_row(&mut app, 120, 44, n));

    // The very next render — no other state change — must already be
    // in the new palette.
    let backend = TestBackend::new(120, 44);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let v = view_state::compute_view(&app, f.area());
            render::render(f, &app, &v);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(
        has_fg(buffer, rgb(theme::ALUCARD.accent)),
        "the next frame must repaint in the highlighted palette"
    );
}

#[test]
fn closed_picker_does_not_live_apply() {
    raul::config::set_color_enabled(true);
    let mut app = settings_app("mocha");
    // Not expanded: the arrows navigate the flat key list and the
    // palette must not move.
    app.move_down();
    app.move_down();
    assert_eq!(palette_name(&app), "mocha");
    assert!(!app.settings.as_ref().unwrap().theme_picker_open());
}

#[test]
fn live_apply_is_a_noop_outside_settings() {
    raul::config::set_color_enabled(true);
    let mut app = app_with_open_picker("mocha");
    app.select_lane(Lane::Milestones);
    let before = palette_name(&app);
    app.apply_theme_preview();
    assert_eq!(
        palette_name(&app),
        before,
        "apply_theme_preview must not repaint when the picker is not the open editor"
    );
}
