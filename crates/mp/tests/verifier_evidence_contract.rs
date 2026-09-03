//! M212 / AC-07: before accepting runner completion, the verifier
//! requires every step done and every AC evidence value to
//! contain the exact command, exit code, and observed pass count.
//! Generic summaries and evidence overwritten after completion
//! are rejected; the canonical criterion state is re-read after
//! completion.

use mp::activity::ActivityLog;
use mp::autopilot::verifier::{
    check_evidence_contract, check_evidence_not_overwritten, validate_evidence_shape,
    EvidenceShapeError,
};
use mp::model::{AcceptanceCriterion, MilestoneFile, MilestoneMeta};

fn milestone_with_ac(id: &str, ac_id: &str, evidence: &str) -> MilestoneFile {
    MilestoneFile {
        milestone: MilestoneMeta {
            id: id.into(),
            title: "Sample".into(),
            slug: "sample".into(),
            lifecycle: "executed".into(),
            ..Default::default()
        },
        acceptance_criteria: vec![AcceptanceCriterion {
            id: ac_id.into(),
            description: String::new(),
            verification: String::new(),
            status: String::new(),
            evidence: evidence.into(),
        }],
        ..Default::default()
    }
}

// ──── validate_evidence_shape ──────────────────────────────────

#[test]
fn evidence_shape_accepts_real_cargo_nextest_output() {
    let s = "cargo nextest run -p mp --test foo --no-fail-fast exit 0 (3/3 pass)";
    validate_evidence_shape(s).unwrap();
}

#[test]
fn evidence_shape_accepts_make_target_with_pass_count() {
    validate_evidence_shape("make test exit 0 (12/12 pass)").unwrap();
}

#[test]
fn evidence_shape_accepts_scripts_path() {
    validate_evidence_shape("scripts/run-checks.sh exit 0 (5/5 pass)").unwrap();
}

#[test]
fn evidence_shape_rejects_empty_string() {
    assert!(matches!(
        validate_evidence_shape("").unwrap_err(),
        EvidenceShapeError::Empty
    ));
}

#[test]
fn evidence_shape_rejects_whitespace_only() {
    assert!(matches!(
        validate_evidence_shape("   ").unwrap_err(),
        EvidenceShapeError::Empty
    ));
}

#[test]
fn evidence_shape_rejects_m201_cycle_1_generic_summary() {
    // The M201 cycle 1 pattern: runner writes "All steps done"
    // or "M<id> complete: …" as evidence. The verifier rejects.
    for generic in [
        "All steps done",
        "All done",
        "M212 complete: ready for review",
        "M212 cycle done",
        "Cycle 1 done",
        "complete: ready for review",
    ] {
        assert!(
            matches!(
                validate_evidence_shape(generic).unwrap_err(),
                EvidenceShapeError::GenericSummary(_)
            ),
            "{generic:?} must be rejected as generic"
        );
    }
}

#[test]
fn evidence_shape_rejects_missing_command() {
    let s = "foo bar exit 0 (1/1 pass)";
    assert!(matches!(
        validate_evidence_shape(s).unwrap_err(),
        EvidenceShapeError::MissingCommand(_)
    ));
}

#[test]
fn evidence_shape_rejects_missing_exit_code() {
    let s = "cargo nextest run -p mp --test foo (3/3 pass)";
    assert!(matches!(
        validate_evidence_shape(s).unwrap_err(),
        EvidenceShapeError::MissingExitCode(_)
    ));
}

#[test]
fn evidence_shape_rejects_missing_pass_count() {
    let s = "cargo nextest run -p mp --test foo exit 0";
    assert!(matches!(
        validate_evidence_shape(s).unwrap_err(),
        EvidenceShapeError::MissingPassCount(_)
    ));
}

#[test]
fn evidence_shape_rejects_pass_count_without_parens() {
    // "3/3 pass" without parens is ambiguous — must be rejected.
    let s = "cargo nextest run -p mp --test foo exit 0 3/3 pass";
    assert!(matches!(
        validate_evidence_shape(s).unwrap_err(),
        EvidenceShapeError::MissingPassCount(_)
    ));
}

// ──── check_evidence_contract ──────────────────────────────────

#[test]
fn contract_flags_every_ac_with_generic_evidence() {
    let m = milestone_with_ac("207", "AC-01", "All steps done");
    let state = mp::autopilot::verifier::VerifierState {
        milestone: m,
        review: None,
        activity: ActivityLog::empty(),
        milestone_path: "207.json".into(),
    };
    let failing = check_evidence_contract(
        &state,
        &mp::autopilot::verifier::LaneNotification::runner_done(
            "207",
            1,
            "executed",
            "done",
            "implemented",
            mp::autopilot::verifier::ActorAttribution {
                session_id: "s1".into(),
                role: mp::autopilot::verifier::Lane::Runner,
                actor_token: "%2".into(),
                dispatch_id: "dispatch-1".into(),
                seq: 1,
            },
        ),
    );
    assert_eq!(failing.len(), 1);
    assert_eq!(failing[0].0, "AC-01");
    assert!(failing[0].1.contains("generic summary"));
}

#[test]
fn contract_passes_when_evidence_is_well_formed() {
    let m = milestone_with_ac(
        "207",
        "AC-01",
        "cargo nextest run -p mp --test foo exit 0 (3/3 pass)",
    );
    let state = mp::autopilot::verifier::VerifierState {
        milestone: m,
        review: None,
        activity: ActivityLog::empty(),
        milestone_path: "207.json".into(),
    };
    let failing = check_evidence_contract(
        &state,
        &mp::autopilot::verifier::LaneNotification::runner_done(
            "207",
            1,
            "executed",
            "done",
            "implemented",
            mp::autopilot::verifier::ActorAttribution {
                session_id: "s1".into(),
                role: mp::autopilot::verifier::Lane::Runner,
                actor_token: "%2".into(),
                dispatch_id: "dispatch-1".into(),
                seq: 1,
            },
        ),
    );
    assert!(failing.is_empty());
}

// ──── Re-read after completion (overwrite detection) ────────────

#[test]
fn check_evidence_not_overwritten_passes_when_snapshot_matches() {
    let m = milestone_with_ac(
        "207",
        "AC-01",
        "cargo nextest run -p mp --test foo exit 0 (3/3 pass)",
    );
    let state = mp::autopilot::verifier::VerifierState {
        milestone: m,
        review: None,
        activity: ActivityLog::empty(),
        milestone_path: "207.json".into(),
    };
    let pre = [(
        "AC-01",
        "cargo nextest run -p mp --test foo exit 0 (3/3 pass)",
    )];
    check_evidence_not_overwritten(&state, &pre).unwrap();
}

#[test]
fn check_evidence_not_overwritten_fails_when_back_filled() {
    // The runner back-filled a generic summary AFTER
    // lifecycle=executed landed. The canonical criterion
    // evidence now says "All done" but the pre-completion
    // snapshot recorded the real `cargo nextest` output.
    let m = milestone_with_ac("207", "AC-01", "All done");
    let state = mp::autopilot::verifier::VerifierState {
        milestone: m,
        review: None,
        activity: ActivityLog::empty(),
        milestone_path: "207.json".into(),
    };
    let pre = [(
        "AC-01",
        "cargo nextest run -p mp --test foo exit 0 (3/3 pass)",
    )];
    let err = check_evidence_not_overwritten(&state, &pre).unwrap_err();
    assert!(matches!(
        err,
        EvidenceShapeError::OverwrittenAfterCompletion { .. }
    ));
}

#[test]
fn check_evidence_not_overwritten_passes_for_empty_pre_snapshot() {
    let m = milestone_with_ac("207", "AC-01", "All done");
    let state = mp::autopilot::verifier::VerifierState {
        milestone: m,
        review: None,
        activity: ActivityLog::empty(),
        milestone_path: "207.json".into(),
    };
    let pre: [(&str, &str); 0] = [];
    // Empty snapshot means no AC was tracked pre-completion;
    // the verifier accepts the current evidence as-is.
    check_evidence_not_overwritten(&state, &pre).unwrap();
}
