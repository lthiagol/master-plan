//! M215 AC-01: Autopilot lane picker.
//!
//! The picker renders drivable milestones from `mp list milestones`,
//! filtered to the autopilot-eligible lifecycles (`approved` /
//! `in-progress` / `remediation`). Toggle selection preserves
//! insertion order; cursor moves correctly across the list.

use raul::tui::autopilot::{is_picker_eligible, Picker, PickerCandidate};

fn sample_list_payload() -> serde_json::Value {
    serde_json::json!({
        "milestones": [
            {"id": "M207", "title": "Pilot S2", "lifecycle": "approved", "priority": "high"},
            {"id": "M209", "title": "Coordination", "lifecycle": "in-progress", "priority": "normal"},
            {"id": "M210", "title": "Spawn pipeline", "lifecycle": "draft", "priority": "low"},
            {"id": "M211", "title": "Reconcile", "lifecycle": "remediation", "priority": "high"},
            {"id": "M212", "title": "Doc", "lifecycle": "complete", "priority": "low"},
        ]
    })
}

/// AC-01: the picker filters `mp list milestones` to the autopilot-
/// eligible lifecycles (approved / in-progress / remediation). Draft,
/// complete, cancelled, and unknown lifecycles drop out.
#[test]
fn picker_filters_to_autopilot_eligible_lifecycles() {
    let candidates = Picker::filter_candidates(&sample_list_payload());
    let ids: Vec<&str> = candidates.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["207", "209", "211"],
        "only approved / in-progress / remediation rows surface; \
         draft (210) and complete (212) drop out"
    );
    // Every surviving row is eligible by definition — explicit
    // double-check so a future filter regression fails here too.
    for c in &candidates {
        assert!(
            is_picker_eligible(&c.lifecycle),
            "{c:?} must be eligible"
        );
    }
}

/// AC-01: the picker also accepts a bare array payload (some
/// `mp list` subcommands omit the envelope). The filter must
/// behave the same way regardless of the wrapper.
#[test]
fn picker_accepts_bare_array_payload() {
    let bare = serde_json::json!([
        {"id": "M207", "lifecycle": "approved"},
        {"id": "M210", "lifecycle": "draft"},
    ]);
    let candidates = Picker::filter_candidates(&bare);
    assert_eq!(
        candidates.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["207"]
    );
}

/// AC-01: `is_picker_eligible` agrees with `mp_model`'s
/// `is_watch_drivable_lifecycle` (the source of truth for the
/// autopilot-eligible allow-list). Both modules must agree so a
/// lane change does not silently split the picker from the dry-run.
#[test]
fn picker_eligibility_matches_mp_model() {
    for lifecycle in [
        "approved",
        "in-progress",
        "remediation",
        "draft",
        "complete",
        "cancelled",
        "unknown",
        "",
    ] {
        let mp_model = mp_model::is_watch_drivable_lifecycle(lifecycle);
        let picker = is_picker_eligible(lifecycle);
        assert_eq!(
            mp_model, picker,
            "picker vs mp_model disagreement on {lifecycle:?}"
        );
    }
}

/// AC-01: toggle selection preserves insertion order. The user
/// toggles 207, then 211, then 209 — the order matches the toggle
/// order, not the canonical id order. Toggling 211 off removes
/// its slot, leaving the others in their original positions.
#[test]
fn toggle_selection_preserves_insertion_order() {
    let mut picker = Picker::empty();
    picker.refresh_candidates(&sample_list_payload());

    picker.toggle_select("207");
    picker.toggle_select("211");
    picker.toggle_select("209");

    assert_eq!(
        picker.queue_ids(),
        vec!["207", "211", "209"],
        "selection order must match toggle order (not id order)"
    );

    // Toggle 211 off — its slot is removed; 207 and 209 stay put.
    picker.toggle_select("211");
    assert_eq!(
        picker.queue_ids(),
        vec!["207", "209"],
        "remove-at-index must keep surviving selections in place"
    );

    // Toggle 209 off — only 207 survives.
    picker.toggle_select("209");
    assert_eq!(picker.queue_ids(), vec!["207"]);

    // Toggling the last item off leaves an empty queue; the picker
    // stays consistent (no stale `cursor` on a non-existent row).
    picker.toggle_select("207");
    assert!(picker.queue_ids().is_empty());
    assert!(!picker.has_selection());
}

/// AC-01: cursor moves correctly across the list — it wraps past
/// either end so the operator can scroll through the picker with
/// j/k without losing focus at the boundaries.
#[test]
fn cursor_moves_correctly_across_the_list() {
    let mut picker = Picker::empty();
    picker.refresh_candidates(&sample_list_payload());

    // 3 eligible rows. Starting cursor = 0.
    assert_eq!(picker.cursor, 0);
    assert_eq!(picker.cursor_candidate().unwrap().id, "207");

    // Move forward one row at a time.
    picker.move_cursor(1);
    assert_eq!(picker.cursor, 1);
    assert_eq!(picker.cursor_candidate().unwrap().id, "209");

    picker.move_cursor(1);
    assert_eq!(picker.cursor, 2);
    assert_eq!(picker.cursor_candidate().unwrap().id, "211");

    // Wrap past the tail back to the head.
    picker.move_cursor(1);
    assert_eq!(picker.cursor, 0, "cursor must wrap past the tail");

    // Wrap past the head back to the tail.
    picker.move_cursor(-1);
    assert_eq!(picker.cursor, 2, "cursor must wrap past the head");

    // Page-style movement (delta > 1) also wraps correctly.
    picker.move_cursor(5);
    assert_eq!(
        picker.cursor,
        1,
        "delta > len must wrap modulo len (5 % 3 = 2; 2 + 2 = 4 % 3 = 1)"
    );
}

/// AC-01: when the candidate list is empty, the cursor stays at 0
/// and `move_cursor` is a no-op rather than panicking.
#[test]
fn cursor_is_safe_on_empty_candidate_list() {
    let mut picker = Picker::empty();
    // No refresh — candidates is empty.
    assert_eq!(picker.cursor, 0);
    picker.move_cursor(1);
    picker.move_cursor(-1);
    picker.move_cursor(42);
    assert_eq!(
        picker.cursor, 0,
        "cursor must remain at 0 on an empty list — picker.move_cursor must not panic"
    );
    assert!(picker.cursor_candidate().is_none());
}

/// AC-01: refresh drops selections that no longer resolve (the
/// milestone was deleted, lifecycle changed to a non-eligible
/// state, or a typo slipped into the toggle). Surviving selections
/// keep their relative order.
#[test]
fn refresh_drops_unknown_selections() {
    let mut picker = Picker::empty();
    picker.refresh_candidates(&sample_list_payload());
    picker.toggle_select("207");
    picker.toggle_select("209");
    assert_eq!(picker.queue_ids(), vec!["207", "209"]);

    // Refresh against a payload that drops 209 — the selection
    // shrinks to just 207, in the original order.
    let reduced = serde_json::json!({
        "milestones": [
            {"id": "M207", "lifecycle": "approved"},
            {"id": "M211", "lifecycle": "remediation"},
        ]
    });
    picker.refresh_candidates(&reduced);
    assert_eq!(
        picker.queue_ids(),
        vec!["207"],
        "refresh must drop unresolvable ids"
    );
}

/// AC-01: the picker's `PickerCandidate` payload round-trips
/// through serde so the typed shape is the wire format. A future
/// field added to the on-disk shape must surface here as a test
/// regression, not a silent miss.
#[test]
fn picker_candidate_round_trips_through_serde() {
    let c = PickerCandidate {
        id: "207".to_string(),
        title: "Pilot S2".to_string(),
        lifecycle: "approved".to_string(),
        priority: Some("high".to_string()),
    };
    let v = serde_json::to_value(&c).unwrap();
    let back: PickerCandidate = serde_json::from_value(v).unwrap();
    assert_eq!(back, c);
}

/// AC-01: the public type is reachable from the production hot
/// path — `raul::tui::autopilot::Picker` resolves without extra
/// feature gates. Without this, the autopilot lane can't link the
/// typed model into the picker renderer.
#[test]
fn picker_is_exported_from_the_tui_module() {
    // Constructing a `Picker` via the re-exported path proves the
    // type is reachable from the integration test binary. No
    // additional assertions — the type's mutators are pinned by
    // the unit tests in `tui::autopilot`.
    let mut p: Picker = Picker::empty();
    p.move_cursor(0);
    assert_eq!(p.cursor, 0);
}