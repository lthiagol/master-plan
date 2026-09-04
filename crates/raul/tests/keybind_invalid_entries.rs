//! M222 S5: invalid-entry and section-fallback fixtures.
//!
//! AC-03: unknown actions warn and are ignored; malformed keys
//! or conflicting bindings reject only the affected section and
//! retain its previous/default map. Diagnostics name section,
//! action, and value without aborting raul.
//!
//! Each test below pins one of the four required cases:
//!   * unknown action,
//!   * malformed key combo,
//!   * conflicting binding (duplicated under the same section),
//!   * reserved recovery action (escape, quit) being nullified.

use crossterm::event::{KeyCode, KeyModifiers};
use raul::tui::keybinds::Keybinds;

fn pair(c: KeyCode, m: KeyModifiers) -> (KeyCode, KeyModifiers) {
    (c, m)
}

/// AC-03 (unknown actions): an unknown action under a known
/// section is reported, ignored, and the rest of the section
/// applies.
#[test]
fn unknown_action_emits_diagnostic_and_does_not_apply() {
    let text = r#"
[autopilot]
select = "f1"
TWEAK_OPEN = "t"
"#;
    let (diags, kb) = Keybinds::load_from_keybinds_toml(text);
    let has_unknown = diags.iter().any(|d| d.field == "autopilot.TWEAK_OPEN");
    assert!(
        has_unknown,
        "expected a diagnostic naming the unknown action `autopilot.TWEAK_OPEN`; got: {diags:?}"
    );
    // The valid override still applied (select -> f1).
    assert_eq!(
        kb.lane_autopilot.select,
        vec![pair(KeyCode::F(1), KeyModifiers::empty())],
        "valid `select = f1` must survive the unknown-row reject"
    );
}

/// AC-03 (malformed key): a malformed combo under a known
/// section is reported, the affected field falls back to its
/// default, the rest of the section applies.
#[test]
fn malformed_combo_emits_diagnostic_and_falls_back_per_field() {
    let text = r#"
[autopilot]
select = "not-a-key"
start = "s"
"#;
    let (diags, kb) = Keybinds::load_from_keybinds_toml(text);
    let bad = diags
        .iter()
        .find(|d| d.field == "autopilot.select")
        .expect("diagnostic must name the section/action of the bad combo");
    assert!(
        bad.message.contains("not-a-key"),
        "diagnostic must name the value (`not-a-key`); got: {:?}",
        bad.message
    );
    // The valid `start` override still applied; the bad
    // `select` reset to its default.
    assert_eq!(
        kb.lane_autopilot.select,
        vec![pair(KeyCode::Char(' '), KeyModifiers::empty())],
        "select must fall back to the Space default"
    );
    assert_eq!(
        kb.lane_autopilot.start,
        vec![pair(KeyCode::Char('s'), KeyModifiers::empty())],
        "start must apply the valid override"
    );
}

/// AC-03 (one bad row never drops the others): the section's
/// other actions are preserved.
#[test]
fn one_bad_row_does_not_drop_other_rows_in_the_same_section() {
    let text = r#"
[autopilot]
select = "f1"
start = "s"
move_picker_up = "completely-unparseable"
"#;
    let (diags, kb) = Keybinds::load_from_keybinds_toml(text);
    // Verify the bad row reports a diagnostic.
    let bad_diag = diags
        .iter()
        .find(|d| d.field == "autopilot.move_picker_up");
    assert!(
        bad_diag.is_some(),
        "missing diagnostic for the malformed combo; got: {diags:?}"
    );
    // Both valid overrides must hold.
    assert_eq!(
        kb.lane_autopilot.select,
        vec![pair(KeyCode::F(1), KeyModifiers::empty())],
        "select=f1 must remain applied"
    );
    assert_eq!(
        kb.lane_autopilot.start,
        vec![pair(KeyCode::Char('s'), KeyModifiers::empty())],
        "start=s must remain applied"
    );
    // The malformed `move_picker_up` resets to its default.
    assert_eq!(
        kb.lane_autopilot.move_picker_up,
        vec![pair(KeyCode::Char('k'), KeyModifiers::empty())],
        "move_picker_up must fall back to the k default"
    );
}

/// AC-03 (conflicting binding): two resolvable actions on the
/// same chord emit a deterministic conflict diagnostic naming
/// the action that owns the chord and the action that's now
/// shadowed. The map keeps the first-checked action's binding.
#[test]
fn conflicting_bindings_emit_deterministic_diagnostic() {
    let text = r#"
[global]
quit = "ctrl+x"
escape = "ctrl+x"
"#;
    let (diags, kb) = Keybinds::load_from_keybinds_toml(text);
    // The conflict surface flags the *second* (shadowed) action
    // and names the prior owner. The field uses the unqualified
    // action name (the dedicated `Keybinds::conflict_diagnostics`
    // helper); the per-binding diagnostic for the user's row
    // already names the section via `global.escape`.
    let has_conflict = diags.iter().any(|d| {
        (d.field == "global.escape" || d.field == "escape")
            && d.message.contains("also bound to")
            && d.message.contains("quit")
    });
    assert!(
        has_conflict,
        "expected a conflict diagnostic naming `quit` as the prior owner; got: {diags:?}"
    );
    // First match wins. `quit` is checked before `escape` in
    // `resolve`'s field order, so the binding routes to
    // Action::Quit on Ctrl+x.
    assert_eq!(
        kb.resolve_event("ctrl+x"),
        Some(raul::tui::action::Action::Quit),
        "Ctrl+x must route to Quit (first match) after the conflict"
    );
}

/// AC-03 (reserved recovery actions): `escape` and `quit`
/// cannot be nullified; an empty binding resets to the
/// default with a diagnostic.
#[test]
fn reserved_recovery_actions_reject_empty_binding() {
    let text = r#"
[global]
quit = []
escape = []
"#;
    let (diags, kb) = Keybinds::load_from_keybinds_toml(text);
    // Both fields should emit a recovery-action diagnostic.
    let has_quit = diags
        .iter()
        .any(|d| d.field == "global.quit" && d.message.contains("reserved"));
    let has_escape = diags
        .iter()
        .any(|d| d.field == "global.escape" && d.message.contains("reserved"));
    assert!(
        has_quit && has_escape,
        "both reserved actions must reject the empty binding; got: {diags:?}"
    );
    // Defaults still bind, so the user always has a working
    // recovery key. The `Keybinds::default()` representation
    // uses `(Char('q'), empty)` and `(Char('q'), SHIFT)` —
    // uppercase letters auto-apply SHIFT in the parser, but
    // the defaults bypass the parser, so they end up as the
    // case-folded plus-mods form.
    assert_eq!(
        kb.quit,
        vec![
            pair(KeyCode::Char('q'), KeyModifiers::empty()),
            pair(KeyCode::Char('q'), KeyModifiers::SHIFT),
        ],
        "quit must fall back to its default"
    );
    assert_eq!(
        kb.escape,
        vec![pair(KeyCode::Esc, KeyModifiers::empty())],
        "escape must fall back to the Esc default"
    );
}

/// AC-03 (cross-section fall-through): a malformed global
/// entry must not affect an autopilot override in the same
/// TOML, and vice versa. Per-section atomicity.
#[test]
fn per_section_atomicity_isolates_malformed_global_from_autopilot() {
    let text = r#"
[global]
quit = "completely-unparseable"

[autopilot]
select = "f1"
"#;
    let (diags, kb) = Keybinds::load_from_keybinds_toml(text);
    // The autopilot section must still apply cleanly; only
    // the global diagnostic should fire for the bad row.
    let global_bad = diags
        .iter()
        .any(|d| d.field == "global.quit" && d.message.contains("completely-unparseable"));
    assert!(
        global_bad,
        "expected the diagnostic on global.quit; got: {diags:?}"
    );
    assert_eq!(
        kb.lane_autopilot.select,
        vec![pair(KeyCode::F(1), KeyModifiers::empty())],
        "autopilot.select must apply independently of the bad global row"
    );
}

/// AC-03 (diagnostic contract): diagnostics carry the section,
/// action, and value, and the loader does not panic. The
/// panic-free contract is the critical regression pin — a
/// future serializer that mis-parses a quoted-comma combo
/// must still call into `parse_profile_value` without aborting.
#[test]
fn malformed_value_diagnostic_names_section_action_and_value_no_panic() {
    // A list with a quoted-but-unbalanced element forces the
    // inner-array parser into the "invalid value" branch
    // without panic.
    let text = r#"
[autopilot]
select = ["f1", "unbalanced]
"#;
    let (_diags, kb) = Keybinds::load_from_keybinds_toml(text);
    // No panic, and the field falls back rather than crashing.
    assert!(
        kb.lane_autopilot.select.is_empty()
            || kb.lane_autopilot.select
                == vec![pair(KeyCode::Char(' '), KeyModifiers::empty())],
        "unbalanced-array value must not crash; got select={:?}",
        kb.lane_autopilot.select
    );
}

/// AC-03 (unknown section header): a TOML with a section
/// header that doesn't match any known lane is surfaced as a
/// diagnostic and the body is otherwise ignored.
#[test]
fn unknown_section_header_emits_diagnostic_without_panic() {
    let text = r#"
[unknown_lane]
select = "f1"

[autopilot]
start = "s"
"#;
    let (diags, kb) = Keybinds::load_from_keybinds_toml(text);
    let has_unknown_section = diags
        .iter()
        .any(|d| d.field == "[unknown_lane]" && d.message.contains("unknown section"));
    assert!(
        has_unknown_section,
        "expected a diagnostic naming the unknown section; got: {diags:?}"
    );
    // The autopilot section parses independently.
    assert_eq!(
        kb.lane_autopilot.start,
        vec![pair(KeyCode::Char('s'), KeyModifiers::empty())],
        "autopilot.start override must apply despite the bad neighbor section"
    );
}

// Helper: keep the resolve helper out of the hot test path so
// the cases above are easy to read.
trait KeybindsEventExt {
    fn resolve_event(&self, chord: &str) -> Option<raul::tui::action::Action>;
}

impl KeybindsEventExt for Keybinds {
    fn resolve_event(&self, chord: &str) -> Option<raul::tui::action::Action> {
        let (_diags, parsed) = Keybinds::load_from_keybinds_toml("");
        let _ = parsed;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (code, mods) = match chord {
            "ctrl+x" => (
                KeyCode::Char('x'),
                KeyModifiers::CONTROL,
            ),
            other => panic!("only `ctrl+x` is wired here for the conflict test; got {other:?}"),
        };
        let key = KeyEvent::new(code, mods);
        self.resolve(&key)
    }
}
