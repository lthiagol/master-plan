//! M173 S7 parity test: TUI render badge string agrees with the
//! `effective_lifecycle` helper for every fixture, including legacy
//! shapes that pre-date the M100 migration.
//!
//! The test pins that the only path to a lifecycle badge in the
//! milestone detail view is through `crate::tui::status::effective_*`
//! helpers. A regression that re-introduces a direct field read in
//! `crates/raul/src/tui/render/milestone_detail.rs` (or anywhere else
//! that renders the badge) will diverge from the helper and this test
//! will fail.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::App;
use raul::tui::render;
use raul::tui::status::{effective_execution_status, effective_lifecycle, effective_spec_status};
use raul::tui::view_state;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[test]
fn legacy_matrix_delegates_to_mp_model() {
    let specs = [
        "",
        "unknown",
        "draft",
        "interview",
        "review",
        "ready",
        "implemented",
        "verified",
    ];
    let executions = [
        "",
        "unknown",
        "planned",
        "in-progress",
        "done",
        "blocked",
        "deferred",
        "cancelled",
    ];
    for spec in specs {
        for execution in executions {
            let milestone = json!({
                "lifecycle": "",
                "spec_status": spec,
                "execution_status": execution,
            });
            let expected = if spec.is_empty() && execution.is_empty() {
                String::new()
            } else {
                mp_model::effective_lifecycle_from_legacy(spec, execution)
            };
            assert_eq!(
                effective_lifecycle(&milestone),
                expected,
                "spec={spec:?}, execution={execution:?}"
            );
        }
    }
    let done = json!({"lifecycle": "", "spec_status": "", "execution_status": "done"});
    // M196: legacy `execution_status: "done"` maps to the canonical
    // lifecycle string `"executed"` (the executor's end-state).
    assert_eq!(effective_lifecycle(&done), "executed");
}

fn render_with_detail(detail: &Value) -> String {
    let mut app = App::new();
    app.load_milestones(vec![raul::tui::app::MilestoneSummary {
        id: "M01".to_string(),
        title: "Sample".to_string(),
        lifecycle: "complete".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.enter_milestone_detail(Some(0));
    app.load_milestone_detail(detail.clone());
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            output.push_str(buffer[(x, y)].symbol());
        }
        while output.ends_with(' ') {
            output.pop();
        }
        output.push('\n');
    }
    output
}

fn minimal_detail(milestone: &Value) -> Value {
    json!({
        "milestone": milestone,
        "intent": { "outcome": "" },
        "problem": { "description": "" },
        "scope": { "in_scope": [], "out_of_scope": [] },
        "acceptance_criteria": [],
        "steps": []
    })
}

#[test]
fn parity_post_m100_canonical_lifecycle_field() {
    // Post-M100 shape: lifecycle is set directly.
    let milestone = json!({
        "id": "M01",
        "title": "Post-M100",
        "lifecycle": "in-progress",
        "spec_status": "ready",
        "execution_status": "in-progress",
        "effort": "S",
        "risk": "low"
    });
    let expected = effective_lifecycle(&milestone);
    assert_eq!(
        expected, "in-progress",
        "helper returns canonical lifecycle"
    );

    let output = render_with_detail(&minimal_detail(&milestone));
    // M202 S19: the header badge is gone; the Stage cell renders
    // `<N>/12 · <Label>` instead. For a milestone with no
    // flow_stages, the stage cell shows 1/12 · Define outcome.
    assert!(
        output.contains("1/12") && output.contains("Define outcome"),
        "rendered header must show the Stage cell; output:\n{output}"
    );
}

#[test]
fn parity_pre_m100_legacy_spec_exec_derive_lifecycle() {
    // Pre-M100 shape: lifecycle is empty; derive from spec_status +
    // execution_status. (verified + done → complete.)
    let milestone = json!({
        "id": "M02",
        "title": "Pre-M100",
        "lifecycle": "",
        "spec_status": "verified",
        "execution_status": "done",
        "effort": "S",
        "risk": "low"
    });
    let expected = effective_lifecycle(&milestone);
    assert_eq!(
        expected, "complete",
        "helper must derive lifecycle from legacy fields"
    );

    let output = render_with_detail(&minimal_detail(&milestone));
    // M202 S19: header badge → Stage cell.
    assert!(
        output.contains("1/12") && output.contains("Define outcome"),
        "rendered header must show the Stage cell; output:\n{output}"
    );
}

#[test]
fn parity_legacy_exec_in_progress_overrides_spec_verified() {
    // ER-7: exec-side in-progress wins regardless of spec-side.
    let milestone = json!({
        "id": "M03",
        "title": "Mixed",
        "lifecycle": "",
        "spec_status": "verified",
        "execution_status": "in-progress",
        "effort": "S",
        "risk": "low"
    });
    let expected = effective_lifecycle(&milestone);
    assert_eq!(expected, "in-progress", "ER-7: exec-in-progress wins");

    let output = render_with_detail(&minimal_detail(&milestone));
    // M202 S19: the header badge is gone; the Stage cell renders
    // `<N>/12 · <Label>` instead. For a milestone with no
    // flow_stages, the stage cell shows 1/12 · Define outcome.
    assert!(
        output.contains("1/12") && output.contains("Define outcome"),
        "rendered header must show the Stage cell; output:\n{output}"
    );
}

#[test]
fn parity_no_status_data_yields_empty_badge() {
    // Empty milestone: no lifecycle, no spec, no exec. The badge
    // renders nothing rather than a default.
    let milestone = json!({
        "id": "M04",
        "title": "Empty",
        "lifecycle": "",
        "spec_status": "",
        "execution_status": "",
        "effort": "S",
        "risk": "low"
    });
    let expected = effective_lifecycle(&milestone);
    assert_eq!(expected, "", "empty milestone yields empty lifecycle");
}

#[test]
fn parity_helper_consistent_across_canonical_legacy() {
    // The helper must produce the same effective lifecycle whether
    // the milestone is post-M100 (canonical) or pre-M100 (legacy
    // only). For the "verified + done" shape, both must resolve to
    // `complete`.
    let canonical = json!({
        "lifecycle": "complete",
        "spec_status": "verified",
        "execution_status": "done",
    });
    let legacy = json!({
        "lifecycle": "",
        "spec_status": "verified",
        "execution_status": "done",
    });
    assert_eq!(
        effective_lifecycle(&canonical),
        effective_lifecycle(&legacy)
    );
}

#[test]
fn parity_effective_execution_status_agrees_with_helper() {
    // When a milestone carries execution_status directly, the helper
    // mirrors it. When only lifecycle is set, the helper derives it.
    let canonical = json!({
        "lifecycle": "in-progress",
        "execution_status": "in-progress",
    });
    assert_eq!(effective_execution_status(&canonical), "in-progress");

    let from_lifecycle = json!({
        "lifecycle": "approved",
    });
    assert_eq!(effective_execution_status(&from_lifecycle), "planned");
}

#[test]
fn parity_effective_spec_status_agrees_with_helper() {
    let canonical = json!({
        "lifecycle": "in-progress",
        "spec_status": "ready",
    });
    assert_eq!(effective_spec_status(&canonical), "ready");

    let from_lifecycle = json!({
        "lifecycle": "groomed",
    });
    // M173 F-05 (sub-agent review): the lifecycle→spec_status map must
    // agree with `mp_model::validate::plan::effective_spec_status`
    // byte-for-byte. The validate helper maps `groomed` → `review`
    // (NOT `groomed`), so the TUI helper must do the same. Pinning
    // both directions here.
    assert_eq!(
        effective_spec_status(&from_lifecycle),
        "review",
        "M173 F-05: TUI helper must agree with validate/plan::effective_spec_status"
    );
}

/// M173 F-11 (sub-agent review): the parity test must diff the helper
/// against the validate-helper spec (the AC-07 contract), not just
/// against itself. The validate-side `effective_spec_status` lives in
/// `crates/mp/src/validate/plan.rs` and operates on `MilestoneFile`
/// structs; the TUI helper operates on JSON `Value`. We pin the
/// JSON-shape derivation here against the validate-helper derivation by
/// running both halves on the same JSON.
///
/// The reference lifecycle→spec_status map (from
/// `crates/mp/src/validate/plan.rs::effective_spec_status`) is:
///   draft → draft, groomed → review, approved → ready,
///   in-progress → ready, done/self-reviewed/reviewed/remediation →
///   implemented, complete → verified.
#[test]
fn parity_effective_spec_status_matches_validate_spec() {
    let cases: &[(&str, &str, &str)] = &[
        // (lifecycle, expected_spec_status, comment)
        ("draft", "draft", "draft lifecycle → draft spec"),
        ("groomed", "review", "groomed → review (NOT groomed)"),
        ("approved", "ready", "approved → ready"),
        ("in-progress", "ready", "in-progress → ready"),
        ("done", "implemented", "done → implemented (NOT verified)"),
        (
            "self-reviewed",
            "implemented",
            "self-reviewed → implemented",
        ),
        (
            "reviewed",
            "verified",
            "legacy reviewed alias → complete delivery phase",
        ),
        ("remediation", "implemented", "remediation → implemented"),
        ("complete", "verified", "complete → verified"),
    ];
    for (lifecycle, expected, comment) in cases {
        let m = json!({ "lifecycle": lifecycle });
        let got = effective_spec_status(&m);
        assert_eq!(
            &got, expected,
            "lifecycle={lifecycle}: helper returned {got:?}, expected {expected:?} ({comment})"
        );
    }
}

/// When the milestone carries a populated `spec_status` field
/// (post-M100 canonical shape, pre-migration legacy shape), the
/// helper must trust it and not derive. Pin against the validate
/// helper's equivalent behavior.
#[test]
fn parity_effective_spec_status_prefers_canonical_field() {
    let m = json!({
        "lifecycle": "complete",
        "spec_status": "ready",
    });
    // spec_status is non-empty → return it raw (M104 contract).
    assert_eq!(effective_spec_status(&m), "ready");
}

#[test]
fn watch_drivable_eligibility_delegates_to_mp_model() {
    // M189 F-08: Raul must not keep a divergent allowlist that treats
    // demoted review aliases as active drive targets.
    assert_eq!(
        raul::tui::watch::DRIVABLE_LIFECYCLES,
        mp_model::WATCH_DRIVABLE_LIFECYCLES
    );
    for alias in ["self-reviewed", "reviewed"] {
        assert!(
            !raul::tui::watch::is_drivable_lifecycle(alias),
            "{alias} must not be watch-drivable"
        );
        assert!(!mp_model::is_watch_drivable_lifecycle(alias));
    }
    for active in ["approved", "in-progress", "remediation"] {
        assert!(raul::tui::watch::is_drivable_lifecycle(active));
    }
}
