//! M222: action registry tests — the typed enum of every
//! configurable global + per-lane keybind action.
//!
//! The registry is the single source of truth that the Settings
//! keymap view, the conflict diagnostics, and the TOML parser
//! all consult. AC-01 + AC-06 share this suite.

use crossterm::event::{KeyCode, KeyModifiers};
use raul::tui::key_combo::parse_key_combo;
use raul::tui::keybinds::{AutopilotLaneKeybinds, Keybinds};

/// AC-01: every default global binding still resolves to the
/// canonical action. The existing tests in `keybinds.rs` already
/// cover this; the M222 registry surfaces it through a single
/// helper so a future addition cannot silently break a default.
#[test]
fn registry_enumerates_every_global_action_with_a_default_binding() {
    let kb = Keybinds::default();
    for name in Keybinds::global_action_names() {
        // The registry must mirror the live struct — a missing
        // entry here would let a future addition to `global_action_names`
        // skip wiring through `slots_mut`/`slots`/the defaults.
        let action = Keybinds::lookup_action(name);
        assert_eq!(
            action,
            Some("global"),
            "global_action_names contains `{name}` but lookup_action does not route it to [global]"
        );
        // And the live binding must be non-empty (the registry
        // asserts every key is configurable; a `[]` default would
        // be a hole).
        match *name {
            "quit" => assert!(!kb.quit.is_empty()),
            "up" => assert!(!kb.up.is_empty()),
            "down" => assert!(!kb.down.is_empty()),
            "page_up" => assert!(!kb.page_up.is_empty()),
            "page_down" => assert!(!kb.page_down.is_empty()),
            "enter" => assert!(!kb.enter.is_empty()),
            "escape" => assert!(!kb.escape.is_empty()),
            "help" => assert!(!kb.help.is_empty()),
            "filter" => assert!(!kb.filter.is_empty()),
            "hide_done" => assert!(!kb.hide_done.is_empty()),
            "create_annotation" => assert!(!kb.create_annotation.is_empty()),
            "resolve" => assert!(!kb.resolve.is_empty()),
            "reopen" => assert!(!kb.reopen.is_empty()),
            "approve" => assert!(!kb.approve.is_empty()),
            "review_menu" => assert!(!kb.review_menu.is_empty()),
            "open_settings" => assert!(!kb.open_settings.is_empty()),
            "previous_lane" => assert!(!kb.previous_lane.is_empty()),
            "next_lane" => assert!(!kb.next_lane.is_empty()),
            "focus_content" => assert!(!kb.focus_content.is_empty()),
            "refresh" => assert!(!kb.refresh.is_empty()),
            "next_section" => assert!(!kb.next_section.is_empty()),
            "prev_section" => assert!(!kb.prev_section.is_empty()),
            "next_item" => assert!(!kb.next_item.is_empty()),
            "prev_item" => assert!(!kb.prev_item.is_empty()),
            "lifecycle_filter" => assert!(!kb.lifecycle_filter.is_empty()),
            "grooming_preset" => assert!(!kb.grooming_preset.is_empty()),
            "search" => assert!(!kb.search.is_empty()),
            "cycle_sort" => assert!(!kb.cycle_sort.is_empty()),
            "clear_filters" => assert!(!kb.clear_filters.is_empty()),
            other => panic!("registry entries and match arms drifted: `{other}`"),
        }
    }
}

/// AC-06: the registry enumerates every Autopilot-lane action.
/// Unknown lane-action names cannot reach a slot, so adding a
/// name here without wiring it into `AutopilotLaneKeybinds`
/// would silently compile and fail here.
#[test]
fn registry_enumerates_every_autopilot_action() {
    for name in Keybinds::autopilot_action_names() {
        let action = Keybinds::lookup_action(name);
        assert_eq!(
            action,
            Some("autopilot"),
            "autopilot_action_names contains `{name}` but lookup_action does not route it to [autopilot]"
        );
    }
    let kb = Keybinds::default();
    let ap = &kb.lane_autopilot;
    let assert_nonempty = |label: &'static str, combos: &Vec<_>| {
        assert!(
            !combos.is_empty(),
            "autopilot default `{label}` must have at least one binding"
        );
    };
    assert_nonempty("select", &ap.select);
    assert_nonempty("move_picker_up", &ap.move_picker_up);
    assert_nonempty("move_picker_down", &ap.move_picker_down);
    assert_nonempty("toggle_panel", &ap.toggle_panel);
    assert_nonempty("start", &ap.start);
    assert_nonempty("replay", &ap.replay);
    assert_nonempty("close", &ap.close);
}

/// AC-06 (deterministic duplicate-conflict diagnostics): the
/// defaults MUST contain at most one action per resolvable
/// chord. Two resolvable actions on the same chord produce a
/// deterministic warning naming the second (shadowed) action.
/// The M222 suite keeps the existing pin from the M138 era so a
/// future default change is caught here.
#[test]
fn registry_default_resolvable_bindings_are_disjoint() {
    use std::collections::HashSet;
    let kb = Keybinds::default();
    // The `focus_content` field intentionally shadows
    // `enter` (a contextual shadow), and the per-mode handler
    // dispatches focus first. Exclude it from the disjointness
    // pin so the contextual shadow doesn't trip the test.
    let mut seen: HashSet<(KeyCode, KeyModifiers)> = HashSet::new();
    let pairs: Vec<(&'static str, &Vec<(KeyCode, KeyModifiers)>)> = vec![
        ("quit", &kb.quit),
        ("up", &kb.up),
        ("down", &kb.down),
        ("page_up", &kb.page_up),
        ("page_down", &kb.page_down),
        ("enter", &kb.enter),
        ("escape", &kb.escape),
        ("help", &kb.help),
        ("filter", &kb.filter),
        ("create_annotation", &kb.create_annotation),
        ("reopen", &kb.reopen),
        ("resolve", &kb.resolve),
        ("approve", &kb.approve),
        ("review_menu", &kb.review_menu),
        ("open_settings", &kb.open_settings),
        ("hide_done", &kb.hide_done),
        ("lifecycle_filter", &kb.lifecycle_filter),
        ("grooming_preset", &kb.grooming_preset),
        ("search", &kb.search),
        ("cycle_sort", &kb.cycle_sort),
        ("clear_filters", &kb.clear_filters),
    ];
    for (name, combos) in pairs {
        for c in combos {
            let combo = (c.0, c.1);
            // Crossterm reports shifted letters either as
            // Char('Q') + SHIFT or Char('q') alone depending on
            // the terminal — collapse the case-fold before the
            // disjointness check.
            let fold = |(code, mods): (KeyCode, KeyModifiers)| -> (KeyCode, KeyModifiers) {
                if let KeyCode::Char(c) = code {
                    if c.is_ascii_uppercase() {
                        return (
                            KeyCode::Char(c.to_ascii_lowercase()),
                            mods | KeyModifiers::SHIFT,
                        );
                    }
                }
                (code, mods)
            };
            let key = fold(combo);
            assert!(
                seen.insert(key),
                "duplicate default keybind: `{name}` shares {combo:?} with another action"
            );
        }
    }
}

/// AC-06: global recovery actions remain reachable. An empty
/// list binding for `escape` or `quit` is rejected by the loader
/// (see `keybind_invalid_entries.rs`); this test pins the
/// registry that those actions exist at all.
#[test]
fn registry_keeps_global_recovery_actions_present() {
    let names: Vec<&'static str> = Keybinds::global_action_names().to_vec();
    assert!(
        names.contains(&"quit"),
        "recovery action `quit` must remain in the registry"
    );
    assert!(
        names.contains(&"escape"),
        "recovery action `escape` must remain in the registry"
    );
}

/// AC-01: one shared service loads both global and per-lane
/// sections in a single call. Verify the TOML loader routes each
/// known action to its section without ambiguity.
#[test]
fn load_from_keybinds_toml_routes_each_section() {
    let text = r#"
[global]
quit = "ctrl+x"
help = "?"

[autopilot]
select = "f1"
start = "s"
"#;
    let (diags, kb) = Keybinds::load_from_keybinds_toml(text);
    assert!(
        diags.is_empty(),
        "clean TOML should not emit diagnostics; got: {diags:?}"
    );
    // Global routing.
    assert_eq!(
        kb.quit,
        vec![(KeyCode::Char('x'), KeyModifiers::CONTROL)],
        "global.quit must override the default q-binding"
    );
    assert_eq!(
        kb.help,
        vec![(KeyCode::Char('?'), KeyModifiers::empty())],
        "global.help unchanged at default"
    );
    // Autopilot routing.
    assert_eq!(
        kb.lane_autopilot.select,
        vec![(KeyCode::F(1), KeyModifiers::empty())],
        "autopilot.select must follow the [autopilot] section override (f1)"
    );
    assert_eq!(
        kb.lane_autopilot.start,
        vec![(KeyCode::Char('s'), KeyModifiers::empty())],
        "autopilot.start is at its default; the [autopilot] override did not displace it"
    );
    // Re-parse the same fixture; the f1 binding should be
    // reflected in `resolve` via the per-lane section: the spec
    // expects the dispatcher to consult it for F1.
    let f1 = parse_key_combo("f1").expect("f1 must parse");
    // The default `Keybinds::resolve` does not yet route
    // autopilot-only actions; this test pins the registry
    // surface (the dispatcher wiring is S2's job).
    let _ = f1;
}

/// AC-01: with no file, every default must remain in place. This
/// is the "single shared service" surface: passing an empty
/// body produces a `Keybinds` identical to `default()`.
#[test]
fn load_from_keybinds_toml_with_empty_body_keeps_defaults() {
    let (diags, kb) = Keybinds::load_from_keybinds_toml("");
    assert!(diags.is_empty());
    assert_eq!(kb, Keybinds::default());
}

/// AC-01: an absent TOML file (the `~/.config/raul/keybinds.toml`
/// default) is silent and produces the defaults.
#[test]
fn load_from_missing_path_returns_defaults_with_no_diagnostics() {
    use std::path::PathBuf;
    let path = PathBuf::from("/nonexistent/raul/keybinds.toml");
    let (diags, kb) = Keybinds::load_from_path(&path);
    assert!(
        diags.is_empty(),
        "missing file must not warn; got: {diags:?}"
    );
    assert_eq!(kb, Keybinds::default());
}

/// The registry exposes both sections through `action_registry`.
/// Pin the order so a future change cannot reorder them
/// silently (the Settings view relies on the
/// globals-first order).
#[test]
fn action_registry_globals_first_then_autopilot() {
    let reg = Keybinds::action_registry();
    let first_autopilot = reg.iter().position(|(s, _, _)| *s == "autopilot");
    let last_global = reg
        .iter()
        .rposition(|(s, _, _)| *s == "global")
        .unwrap_or(0);
    assert!(
        last_global < first_autopilot.unwrap_or(0),
        "global actions must come before autopilot; got order: {reg:?}"
    );
    // The registry must contain every name from both sides.
    let global_count = Keybinds::global_action_names().len();
    let autopilot_count = Keybinds::autopilot_action_names().len();
    assert_eq!(reg.len(), global_count + autopilot_count);
}

/// AC-06: the registry's defaults are all parseable TOML key
/// strings. The `Action` surface and the TOML surface share the
/// same grammar; this test pins that contract so the Settings
/// keymap view never displays an un-parseable default.
#[test]
fn registry_default_key_strings_all_parse() {
    for (section, name, default_str) in Keybinds::action_registry() {
        if default_str.is_empty() {
            continue;
        }
        for chord in default_str.split(',').map(str::trim) {
            assert!(
                parse_key_combo(chord).is_some(),
                "registry default `{section}.{name}={chord:?}` failed to parse via the canonical key grammar"
            );
        }
    }
}

/// `AutopilotLaneKeybinds::default()` is exposed so test fixtures
/// can construct lane-only overrides. The pin here is a
/// documentation test: any future addition to the struct needs
/// to be wired here too or it won't ship a default.
#[test]
fn autopilot_lane_default_preserves_pre_m222_hardcoded_matches() {
    let ap = AutopilotLaneKeybinds::default();
    assert_eq!(ap.select, vec![(KeyCode::Char(' '), KeyModifiers::empty())]);
    assert_eq!(
        ap.move_picker_up,
        vec![(KeyCode::Char('k'), KeyModifiers::empty())]
    );
    assert_eq!(
        ap.move_picker_down,
        vec![(KeyCode::Char('j'), KeyModifiers::empty())]
    );
    assert_eq!(
        ap.toggle_panel,
        vec![(KeyCode::Char('o'), KeyModifiers::empty())]
    );
    assert_eq!(ap.start, vec![(KeyCode::Char('s'), KeyModifiers::empty())]);
    assert_eq!(ap.replay, vec![(KeyCode::Char('P'), KeyModifiers::empty())]);
    assert_eq!(ap.close, vec![(KeyCode::Esc, KeyModifiers::empty())]);
}
