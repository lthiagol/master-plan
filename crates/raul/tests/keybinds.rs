//! M138: `Keybinds` load / resolve / help-generation tests.
//!
//! * AC-03 — load from the `[keybinds]` config section; missing or malformed
//!   entries fall back to the default binding.
//! * AC-04 — `resolve` returns the right `Action` for every default binding,
//!   forward (combo → action) and inverse (action → a known combo).
//! * AC-05 — help text is generated from the struct and reflects every
//!   default binding.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;

use raul::tui::action::Action;
use raul::tui::keybinds::Keybinds;

fn ev(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ev_mods(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

// ---------------------------------------------------------------------------
// AC-04 — resolve, forward (combo -> action)
// ---------------------------------------------------------------------------

#[test]
fn resolve_default_content_bindings_forward() {
    let kb = Keybinds::default();
    // Global-ish / content-canonical bindings resolved by `resolve`.
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), Some(Action::Quit));
    assert_eq!(kb.resolve(&ev(KeyCode::Char('Q'))), Some(Action::Quit));
    assert_eq!(kb.resolve(&ev(KeyCode::Esc)), Some(Action::Esc));
    // M167: Tab no longer toggles a focus state — resolve returns None
    // (the contextual handler in modes/normal.rs maps Tab to NextLane
    // via the `next_lane` slot).
    assert_eq!(kb.resolve(&ev(KeyCode::Tab)), None);
    assert_eq!(kb.resolve(&ev(KeyCode::Up)), Some(Action::Up));
    assert_eq!(kb.resolve(&ev(KeyCode::Char('k'))), Some(Action::Up));
    assert_eq!(kb.resolve(&ev(KeyCode::Down)), Some(Action::Down));
    assert_eq!(kb.resolve(&ev(KeyCode::Char('j'))), Some(Action::Down));
    assert_eq!(kb.resolve(&ev(KeyCode::PageUp)), Some(Action::PageUp));
    assert_eq!(kb.resolve(&ev(KeyCode::PageDown)), Some(Action::PageDown));
    assert_eq!(kb.resolve(&ev(KeyCode::Enter)), Some(Action::Enter));
    assert_eq!(kb.resolve(&ev(KeyCode::Char('?'))), Some(Action::OpenHelp));
    assert_eq!(
        kb.resolve(&ev(KeyCode::Char('f'))),
        Some(Action::ToggleFilter)
    );
    assert_eq!(
        kb.resolve(&ev(KeyCode::Char('h'))),
        Some(Action::ToggleHideDone)
    );
    assert_eq!(
        kb.resolve(&ev_mods(KeyCode::Char('a'), KeyModifiers::SHIFT)),
        Some(Action::CreateAnnotation)
    );
    assert_eq!(
        kb.resolve(&ev(KeyCode::Char('r'))),
        Some(Action::ResolveAnnotation)
    );
    assert_eq!(
        kb.resolve(&ev_mods(KeyCode::Char('r'), KeyModifiers::SHIFT)),
        Some(Action::ReopenAnnotation)
    );
    assert_eq!(
        kb.resolve(&ev(KeyCode::Char('p'))),
        Some(Action::ToggleApproval)
    );
    assert_eq!(
        kb.resolve(&ev(KeyCode::Char('m'))),
        Some(Action::OpenReviewMenu)
    );
}

#[test]
fn resolve_uppercase_letters_via_shift() {
    let kb = Keybinds::default();
    // `A` delivered as Char('A') with no modifiers must still resolve to the
    // shift+a binding (crossterm delivers uppercase both ways).
    assert_eq!(
        kb.resolve(&ev(KeyCode::Char('A'))),
        Some(Action::CreateAnnotation)
    );
    assert_eq!(
        kb.resolve(&ev(KeyCode::Char('R'))),
        Some(Action::ReopenAnnotation)
    );
}

#[test]
fn resolve_unbound_key_is_none() {
    let kb = Keybinds::default();
    assert_eq!(kb.resolve(&ev(KeyCode::Char('z'))), None);
    assert_eq!(kb.resolve(&ev(KeyCode::F(9))), None);
}

// ---------------------------------------------------------------------------
// AC-04 — inverse (action -> a known combo). Every default binding round-trips
// through `resolve`: taking the action's first bound combo and feeding it back
// yields the same action.
// ---------------------------------------------------------------------------

#[test]
fn every_default_binding_round_trips() {
    let kb = Keybinds::default();
    type FieldRow<'a> = (&'a str, &'a Vec<(KeyCode, KeyModifiers)>, Action);
    let fields: Vec<FieldRow> = vec![
        ("quit", &kb.quit, Action::Quit),
        ("up", &kb.up, Action::Up),
        ("down", &kb.down, Action::Down),
        ("page_up", &kb.page_up, Action::PageUp),
        ("page_down", &kb.page_down, Action::PageDown),
        ("enter", &kb.enter, Action::Enter),
        ("escape", &kb.escape, Action::Esc),
        ("help", &kb.help, Action::OpenHelp),
        ("filter", &kb.filter, Action::ToggleFilter),
        ("hide_done", &kb.hide_done, Action::ToggleHideDone),
        (
            "create_annotation",
            &kb.create_annotation,
            Action::CreateAnnotation,
        ),
        ("resolve", &kb.resolve, Action::ResolveAnnotation),
        ("reopen", &kb.reopen, Action::ReopenAnnotation),
        ("approve", &kb.approve, Action::ToggleApproval),
        ("review_menu", &kb.review_menu, Action::OpenReviewMenu),
        // M167: detail-section navigation (NextSection/PrevSection
        // /NextItem/PrevItem) is consumed contextually by the modes/normal
        // handler rather than via `resolve` — same shape as
        // `previous_lane` / `next_lane` / `focus_content` above. They
        // round-trip through the profile TOML serializer; round-trip
        // through `resolve` is intentionally not asserted for them.
    ];
    for (name, combos, expected) in fields {
        assert!(!combos.is_empty(), "{name} must have a default binding");
        for combo in combos {
            let key = KeyEvent::new(combo.0, combo.1);
            assert_eq!(
                kb.resolve(&key),
                Some(expected.clone()),
                "combo {combo:?} for {name} must resolve to {expected:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// AC-03 — load from config JSON; fall back to default on missing / malformed
// ---------------------------------------------------------------------------

#[test]
fn load_from_config_applies_override() {
    let cfg = json!({ "config": { "keybinds": { "quit": "x" } } });
    let kb = Keybinds::load_from_config(&cfg);
    // Override took effect.
    assert_eq!(kb.resolve(&ev(KeyCode::Char('x'))), Some(Action::Quit));
    // The old default no longer resolves to Quit (it was replaced, not added).
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), None);
}

#[test]
fn load_from_config_missing_entries_fall_back_to_default() {
    let cfg = json!({ "config": { "keybinds": { "quit": "x" } } });
    let kb = Keybinds::load_from_config(&cfg);
    // `up` was not overridden -> default Up / k still resolve.
    assert_eq!(kb.resolve(&ev(KeyCode::Up)), Some(Action::Up));
    assert_eq!(kb.resolve(&ev(KeyCode::Char('k'))), Some(Action::Up));
}

#[test]
fn load_from_config_missing_section_is_all_defaults() {
    let cfg = json!({ "config": { "ui": { "color": true } } });
    let kb = Keybinds::load_from_config(&cfg);
    assert_eq!(kb, Keybinds::default());
}

#[test]
fn load_from_config_malformed_combo_falls_back() {
    let cfg = json!({ "config": { "keybinds": { "quit": "not-a-key" } } });
    let kb = Keybinds::load_from_config(&cfg);
    // Malformed value -> keep the default binding, do not crash.
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), Some(Action::Quit));
}

#[test]
fn load_from_config_wrong_type_falls_back() {
    // A numeric value (not a string) must not crash; default is kept.
    let cfg = json!({ "config": { "keybinds": { "quit": 42 } } });
    let kb = Keybinds::load_from_config(&cfg);
    assert_eq!(kb.resolve(&ev(KeyCode::Char('q'))), Some(Action::Quit));
}

// ---------------------------------------------------------------------------
// AC-05 — help text is generated from the struct and reflects every binding
// ---------------------------------------------------------------------------

#[test]
fn help_entries_cover_every_action_with_keys() {
    let kb = Keybinds::default();
    let entries = kb.help_entries();
    // M185: +2 help rows (lifecycle_filter, grooming_preset) → 26.
    // M186: +2 help rows (search, cycle_sort) → 28.
    assert_eq!(entries.len(), 28, "help must list every keybindable action");
    for entry in &entries {
        assert!(
            !entry.keys.is_empty(),
            "help entry {:?} must show at least one key",
            entry.label
        );
        assert!(!entry.keys_display().is_empty());
    }
}

#[test]
fn help_reflects_overridden_binding() {
    let cfg = json!({ "config": { "keybinds": { "quit": "ctrl+c" } } });
    let kb = Keybinds::load_from_config(&cfg);
    let quit = kb
        .help_entries()
        .into_iter()
        .find(|e| e.label == "Quit")
        .expect("Quit entry present");
    assert_eq!(quit.keys, vec!["Ctrl+c".to_string()]);
}

#[test]
fn footers_are_generated_and_nonempty() {
    let kb = Keybinds::default();
    // M198: `footer_tab_bar` takes the show_autopilot_tab flag so
    // the "1-N:jump" range matches the visible lane list. With
    // Watch visible the legend is "1-7"; with Watch hidden it
    // drops to "1-6". Both variants must end in ":quit".
    assert!(kb.footer_tab_bar(true).contains(":quit"));
    assert!(kb.footer_tab_bar(false).contains(":quit"));
    // The jump range tracks the visible list, not the full
    // registry — pinning the contract here so a future
    // lane-add does not silently drift the legend.
    assert!(kb.footer_tab_bar(true).contains("1-7:jump"));
    assert!(kb.footer_tab_bar(false).contains("1-6:jump"));
    assert!(kb.footer_overview().contains(":help"));
    // M187: footer_list was trimmed — quit/help/hide-done now live only
    // on the globals line. The lane-specific actions remain.
    assert!(kb.footer_list().contains(":move"));
    assert!(kb.footer_list().contains(":select"));
    assert!(kb.footer_list().contains(":back"));
    assert!(!kb.footer_list().contains(":hide-done"));
    assert!(!kb.footer_list().contains(":quit"));
    assert!(kb.footer_content(true).contains("(open only)"));
    assert!(kb.footer_content(false).contains("(all)"));
}
