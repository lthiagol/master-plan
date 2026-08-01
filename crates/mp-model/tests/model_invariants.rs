//! M101 R3+R4: model invariants — Range::validate, is_valid_side, RFC3339
//! FindingThreadEntry.at validation, schema enums accept '', FindingDraft
//! validates every field. Pinned by unit tests on the model layer.

use mp_model::{
    is_valid_confidence, is_valid_side, transition, FindingDraft, MilestoneEvent,
    MilestoneOverlays, MilestonePhase, MilestoneState, Range, TransitionContext,
};

fn state(phase: MilestonePhase) -> MilestoneState {
    MilestoneState {
        phase,
        overlays: MilestoneOverlays::default(),
        remediation_pre_state: None,
    }
}

#[test]
fn typed_transition_table_rejects_arbitrary_and_terminal_jumps() {
    let ctx = TransitionContext::default();
    assert!(transition(&state(MilestonePhase::Draft), MilestoneEvent::Complete, ctx).is_err());
    assert!(transition(&state(MilestonePhase::Complete), MilestoneEvent::Groom, ctx).is_err());
    let approved = transition(&state(MilestonePhase::Draft), MilestoneEvent::Approve, ctx).unwrap();
    assert_eq!(approved.phase, MilestonePhase::Approved);
    assert_eq!(approved.spec_status, "ready");
    assert_eq!(approved.execution_status, "planned");
}

#[test]
fn block_and_cancel_refuse_complete_terminal_overlay_drift() {
    let ctx = TransitionContext::default();
    let err_block = transition(&state(MilestonePhase::Complete), MilestoneEvent::Block, ctx)
        .expect_err("block on complete must fail");
    assert!(
        err_block.contains("invalid milestone transition"),
        "{err_block}"
    );
    let err_cancel = transition(
        &state(MilestonePhase::Complete),
        MilestoneEvent::Cancel,
        ctx,
    )
    .expect_err("cancel on complete must fail");
    assert!(
        err_cancel.contains("invalid milestone transition"),
        "{err_cancel}"
    );
    // Non-terminal phases still accept the overlays.
    let blocked = transition(
        &state(MilestonePhase::InProgress),
        MilestoneEvent::Block,
        ctx,
    )
    .unwrap();
    assert!(blocked.overlays.blocked);
    assert_eq!(blocked.phase, MilestonePhase::InProgress);
}

#[test]
fn watch_drivable_lifecycles_exclude_review_aliases() {
    use mp_model::{is_watch_drivable_lifecycle, WATCH_DRIVABLE_LIFECYCLES};
    assert_eq!(
        WATCH_DRIVABLE_LIFECYCLES,
        &["approved", "in-progress", "remediation"]
    );
    for alias in ["self-reviewed", "reviewed", "done", "complete"] {
        assert!(
            !is_watch_drivable_lifecycle(alias),
            "{alias} must not be watch-drivable"
        );
    }
}

#[test]
fn transition_restores_exact_remediation_pre_state() {
    for origin in [MilestonePhase::Executed, MilestonePhase::Complete] {
        let entered = transition(
            &state(origin),
            MilestoneEvent::EnterRemediation,
            Default::default(),
        )
        .unwrap();
        assert_eq!(entered.remediation_pre_state, Some(origin));
        let remediation = MilestoneState {
            phase: entered.phase,
            overlays: entered.overlays,
            remediation_pre_state: entered.remediation_pre_state,
        };
        let exited = transition(
            &remediation,
            MilestoneEvent::ExitRemediation,
            Default::default(),
        )
        .unwrap();
        assert_eq!(exited.phase, origin);
        assert_eq!(exited.remediation_pre_state, None);
    }
}

#[test]
fn legacy_review_aliases_are_not_active_phases() {
    // M196: the canonical executor end-state is now `Executed`; the
    // legacy `"done"` alias still parses to `Executed` during the
    // migration window.
    assert_eq!(
        MilestonePhase::from_lifecycle("self-reviewed").unwrap(),
        MilestonePhase::Executed
    );
    assert_eq!(
        MilestonePhase::from_lifecycle("done").unwrap(),
        MilestonePhase::Executed
    );
    assert_eq!(
        MilestonePhase::from_lifecycle("executed").unwrap(),
        MilestonePhase::Executed
    );
    assert_eq!(
        MilestonePhase::from_lifecycle("reviewed").unwrap(),
        MilestonePhase::Complete
    );
    assert!(![
        MilestonePhase::Draft,
        MilestonePhase::Groomed,
        MilestonePhase::Approved,
        MilestonePhase::InProgress,
        MilestonePhase::Executed,
        MilestonePhase::Complete,
        MilestonePhase::Remediation,
    ]
    .iter()
    .any(|phase| matches!(phase.as_str(), "self-reviewed" | "reviewed")));
}

#[test]
fn range_validate_rejects_inverted() {
    let r = Range {
        start_line: 20,
        end_line: 10,
    };
    assert!(r.validate().is_err(), "inverted range must fail validation");
    let r = Range {
        start_line: 10,
        end_line: 20,
    };
    assert!(r.validate().is_ok(), "ascending range must pass validation");
    let r = Range {
        start_line: 5,
        end_line: 5,
    };
    assert!(
        r.validate().is_ok(),
        "equal start/end range must pass (single line)"
    );
}

#[test]
fn is_valid_side_rejects_unknown() {
    assert!(is_valid_side(""));
    assert!(is_valid_side("old"));
    assert!(is_valid_side("new"));
    // Case-sensitive: the CLI parser lowercases before calling, so the
    // model helper only accepts lowercase. Pin that contract.
    assert!(!is_valid_side("OLD"));
    assert!(!is_valid_side("NEW"));
    assert!(!is_valid_side("upstream"));
    assert!(!is_valid_side("left"));
}

#[test]
fn finding_thread_at_validates_rfc3339() {
    use mp_model::FindingThreadEntry;

    let e = FindingThreadEntry {
        author: "test".to_string(),
        at: "".to_string(),
        body: "empty sentinel must pass".to_string(),
    };
    assert!(e.validate().is_ok(), "empty at must pass (legacy sentinel)");

    let e = FindingThreadEntry {
        author: "test".to_string(),
        at: "2026-07-06T12:00:00Z".to_string(),
        body: "Z".to_string(),
    };
    assert!(e.validate().is_ok(), "RFC3339 with Z must pass");

    let e = FindingThreadEntry {
        author: "test".to_string(),
        at: "2026-07-06T12:00:00+00:00".to_string(),
        body: "+00:00".to_string(),
    };
    assert!(e.validate().is_ok(), "RFC3339 with +00:00 must pass");

    let e = FindingThreadEntry {
        author: "test".to_string(),
        at: "2026-07-06T12:00:00.123456Z".to_string(),
        body: "fractional".to_string(),
    };
    assert!(
        e.validate().is_ok(),
        "RFC3339 with fractional seconds must pass"
    );

    let e = FindingThreadEntry {
        author: "test".to_string(),
        at: "not-a-date".to_string(),
        body: "garbage".to_string(),
    };
    assert!(e.validate().is_err(), "non-RFC3339 must fail");

    let e = FindingThreadEntry {
        author: "test".to_string(),
        at: "2026-07-06".to_string(),
        body: "date only".to_string(),
    };
    assert!(e.validate().is_err(), "date-only (no time) must fail");

    let e = FindingThreadEntry {
        author: "test".to_string(),
        at: "12:00:00".to_string(),
        body: "time only".to_string(),
    };
    assert!(e.validate().is_err(), "time-only (no date) must fail");
}

#[test]
fn is_valid_confidence_accepts_documented_set() {
    assert!(is_valid_confidence(""));
    assert!(is_valid_confidence("low"));
    assert!(is_valid_confidence("medium"));
    assert!(is_valid_confidence("high"));
    assert!(!is_valid_confidence("Low"));
    assert!(!is_valid_confidence("MEDIUM"));
    assert!(!is_valid_confidence("critical"));
    assert!(!is_valid_confidence("none"));
}

#[test]
fn finding_draft_validate_rejects_invalid_confidence_and_phase() {
    // M101 R4 + F-11: FindingDraft::validate is the single entry point
    // every write path (current + future) uses. Confidence + phase
    // validation live here so adding a new setter (e.g. update_finding)
    // cannot bypass them by forgetting to wire the validator.
    let mut draft = FindingDraft {
        milestone_id: "01".to_string(),
        severity: "high".to_string(),
        category: "correctness".to_string(),
        description: "draft".to_string(),
        author: "test".to_string(),
        phase: "self".to_string(),
        confidence: "high".to_string(),
        tags: vec![],
        anchor: None,
        thread: vec![],
        summary: String::new(),
        rationale: String::new(),
    };
    assert!(draft.validate().is_ok());

    draft.confidence = "critical".to_string();
    assert!(draft.validate().is_err(), "invalid confidence must fail");

    draft.confidence = "high".to_string();
    draft.phase = "mid-phase".to_string();
    assert!(draft.validate().is_err(), "invalid phase must fail");
}
