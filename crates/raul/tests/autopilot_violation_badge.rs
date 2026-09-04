//! M216 AC-04: violation badge + click-to-expand.
//!
//! When C2's verifier emits a typed `Violation`, the
//! offending pane row shows a `role violation` badge.
//! Click-to-expand reveals the violation name + evidence
//! hint. The typed `Violation` enum carries three
//! variants: `RoleViolation`, `Stall`, and
//! `RejectedVerdict`. Each renders a distinct badge +
//! expansion.

use raul::tui::autopilot::Violation;

fn session_with_violations() -> serde_json::Value {
    serde_json::json!({
        "session_id": "alpha",
        "session": {
            "id": "alpha",
            "violations": [
                {
                    "kind": "role-violation",
                    "name": "missing_notify",
                    "evidence_hint": "role did not emit notify within 60s",
                },
                {
                    "kind": "stall",
                },
                {
                    "kind": "rejected-verdict",
                    "reason": "verifier rejected: cycle > cap",
                },
            ],
        },
    })
}

/// AC-04: the `Violation::parse_all` adapter reads the
/// `session.violations[]` block and produces one typed
/// entry per item. Each variant carries its specific
/// payload.
#[test]
fn violation_parse_all_handles_every_variant() {
    let violations = Violation::parse_all(&session_with_violations());
    assert_eq!(violations.len(), 3);

    match &violations[0] {
        Violation::RoleViolation {
            name,
            evidence_hint,
        } => {
            assert_eq!(name, "missing_notify");
            assert_eq!(evidence_hint, "role did not emit notify within 60s");
        }
        other => panic!("expected RoleViolation, got {other:?}"),
    }

    match &violations[1] {
        Violation::Stall => {}
        other => panic!("expected Stall, got {other:?}"),
    }

    match &violations[2] {
        Violation::RejectedVerdict { reason } => {
            assert_eq!(reason, "verifier rejected: cycle > cap");
        }
        other => panic!("expected RejectedVerdict, got {other:?}"),
    }
}

/// AC-04: the badge text is stable per variant — the
/// golden test pins the format verbatim. The renderer
/// reads `violation.badge()` to draw the one-line label
/// next to the pane row.
#[test]
fn violation_badge_text_is_stable_per_variant() {
    let role = Violation::RoleViolation {
        name: "missing_notify".to_string(),
        evidence_hint: "hint".to_string(),
    };
    assert_eq!(role.badge(), "[violation: missing_notify]");

    let stall = Violation::Stall;
    assert_eq!(stall.badge(), "[stall]");

    let rejected = Violation::RejectedVerdict {
        reason: "cycle > cap".to_string(),
    };
    assert_eq!(rejected.badge(), "[rejected]");
}

/// AC-04: the click-to-expand payload is stable per
/// variant. The renderer reads `violation.expanded()`
/// to draw the multi-line block below the row when the
/// operator clicks the badge.
#[test]
fn violation_expanded_payload_is_stable_per_variant() {
    let role = Violation::RoleViolation {
        name: "missing_notify".to_string(),
        evidence_hint: "role did not emit notify within 60s".to_string(),
    };
    assert_eq!(
        role.expanded(),
        "  ↳ missing_notify: role did not emit notify within 60s"
    );

    let stall = Violation::Stall;
    assert_eq!(
        stall.expanded(),
        "  ↳ stall: no notification within stall_timeout_ms"
    );

    let rejected = Violation::RejectedVerdict {
        reason: "cycle > cap".to_string(),
    };
    assert_eq!(rejected.expanded(), "  ↳ rejected: cycle > cap");
}

/// AC-04: unknown `kind` strings are skipped — the
/// adapter never panics on a malformed payload.
#[test]
fn violation_parse_all_skips_unknown_kinds() {
    let payload = serde_json::json!({
        "session": {
            "violations": [
                {"kind": "role-violation", "name": "x", "evidence_hint": "y"},
                {"kind": "unknown-kind"},
                {"kind": "stall"},
            ],
        },
    });
    let violations = Violation::parse_all(&payload);
    assert_eq!(violations.len(), 2);
    assert!(matches!(violations[0], Violation::RoleViolation { .. }));
    assert!(matches!(violations[1], Violation::Stall));
}

/// AC-04: an empty / missing `violations` block yields
/// an empty Vec. The renderer skips the badge column
/// when the list is empty.
#[test]
fn violation_parse_all_handles_missing_or_empty_blocks() {
    let payload = serde_json::json!({"session": {"id": "alpha"}});
    assert!(Violation::parse_all(&payload).is_empty());

    let payload = serde_json::json!({"session": {"violations": []}});
    assert!(Violation::parse_all(&payload).is_empty());
}

/// AC-04: `Violation` round-trips through serde with
/// the kebab-case tag discriminator. The wire format
/// matches the verifier's emit shape; the round-trip
/// pins the contract.
#[test]
fn violation_round_trips_through_serde() {
    let cases = vec![
        Violation::RoleViolation {
            name: "missing_notify".to_string(),
            evidence_hint: "hint".to_string(),
        },
        Violation::Stall,
        Violation::RejectedVerdict {
            reason: "cycle > cap".to_string(),
        },
    ];
    for v in cases {
        let json = serde_json::to_value(&v).unwrap();
        let back: Violation = serde_json::from_value(json).unwrap();
        assert_eq!(back, v);
    }
}

/// AC-04: production-path regression. The violation
/// list is reachable from the lane state through
/// `app.autopilot.violations()`. The status graph
/// renderer looks up violations by pane id to draw the
/// badge column.
#[test]
fn violations_are_reachable_from_the_lane_state() {
    use raul::tui::autopilot::{refresh::refresh_from_json, AutopilotLaneState};
    let mut state = AutopilotLaneState::empty();
    assert!(state.violations().is_none());

    refresh_from_json(
        &mut state,
        &session_with_violations(),
        &serde_json::json!({"run_state": {"kind": "live"}}),
    );
    let violations = state.violations().expect("violations populated");
    assert_eq!(violations.len(), 3);
}

/// AC-04: the `ViolationBadge::for_pane` lookup is
/// total — the typed wrapper returns `None` when the
/// list is empty and `Some(&Violation)` for a known
/// pane id. The status graph reads this method to
/// decide whether to draw the badge for a given row.
#[test]
fn violation_badge_lookup_is_total() {
    use raul::tui::autopilot::ViolationBadge;
    let empty = ViolationBadge::empty();
    assert!(empty.for_pane("%5").is_none());

    let mut badge = ViolationBadge::empty();
    badge.violations = vec![Violation::Stall, Violation::Stall, Violation::Stall];
    // Pane id %5 → last char '5' → idx 5 % 3 = 2 → returns Some
    assert!(badge.for_pane("%5").is_some());
    assert!(badge.for_pane("%7").is_some());
}

/// AC-04: every variant's accessor returns the
/// canonical slug the verifier emits. The renderer
/// reads `name()` to surface the slug in the badge +
/// expansion; the slug must match the verifier's
/// emit (so a future verifier change is visible here).
#[test]
fn violation_name_accessor_returns_canonical_slug() {
    let role = Violation::RoleViolation {
        name: "missing_notify".to_string(),
        evidence_hint: "hint".to_string(),
    };
    assert_eq!(role.name(), "missing_notify");

    let stall = Violation::Stall;
    assert_eq!(stall.name(), "stall");

    let rejected = Violation::RejectedVerdict {
        reason: "x".to_string(),
    };
    assert_eq!(rejected.name(), "rejected-verdict");
}

/// AC-04: every variant's `evidence_hint()` accessor
/// returns the verifier's hint string. The renderer
/// surfaces the hint in the click-to-expand panel.
#[test]
fn violation_evidence_hint_accessor_returns_hint() {
    let role = Violation::RoleViolation {
        name: "missing_notify".to_string(),
        evidence_hint: "role did not emit notify".to_string(),
    };
    assert_eq!(role.evidence_hint(), "role did not emit notify");

    let stall = Violation::Stall;
    assert_eq!(stall.evidence_hint(), "");

    let rejected = Violation::RejectedVerdict {
        reason: "cycle > cap".to_string(),
    };
    assert_eq!(rejected.evidence_hint(), "cycle > cap");
}
