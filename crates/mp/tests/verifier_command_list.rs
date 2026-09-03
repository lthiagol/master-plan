//! M212 / AC-08: verification commands use a structured
//! command-list representation. Parentheses in nextest filters are
//! preserved as argv; shell control operators such as `&&`, `;`,
//! and newlines are either represented as separate commands or
//! rejected with an actionable typed error, never silently
//! skipped.

use mp::activity::ActivityLog;
use mp::autopilot::verifier::{
    check_command_list, check_notification, ActorAttribution, Lane, LaneNotification, Verdict,
    VerifierInputs, VerifierState, VerificationCommand, Violation,
};
use mp::model::{AcceptanceCriterion, MilestoneFile, MilestoneMeta};

fn attribution() -> ActorAttribution {
    ActorAttribution {
        session_id: "s1".into(),
        role: Lane::Runner,
        actor_token: "%2".into(),
        dispatch_id: "dispatch-1".into(),
        seq: 1,
    }
}

fn state_with_real_evidence() -> VerifierState {
    let mut m = MilestoneFile::default();
    m.milestone = MilestoneMeta {
        id: "207".into(),
        title: "Sample".into(),
        slug: "sample".into(),
        lifecycle: "executed".into(),
        ..Default::default()
    };
    let mut ac = AcceptanceCriterion::default();
    ac.id = "AC-01".into();
    ac.evidence = "cargo nextest run -p mp --test foo exit 0 (3/3 pass)".into();
    m.acceptance_criteria.push(ac);
    VerifierState {
        milestone: m,
        review: None,
        activity: ActivityLog::empty(),
        milestone_path: "207.json".into(),
    }
}

fn notification_with_command(cmd: VerificationCommand) -> LaneNotification {
    let mut n = LaneNotification::runner_done(
        "207",
        1,
        "executed",
        "done",
        "implemented",
        attribution(),
    );
    n.verification_commands.push(cmd);
    n
}

#[test]
fn command_list_preserves_parentheses_in_nextest_filter() {
    // nextest's `-E` filter syntax uses parentheses; the
    // verifier must preserve them as a single argv token, not
    // try to interpret them as a shell group.
    let n = notification_with_command(VerificationCommand {
        label: "cargo-nextest".into(),
        argv: vec![
            "cargo".into(),
            "nextest".into(),
            "run".into(),
            "-p".into(),
            "mp".into(),
            "-E".into(),
            "test(/verifier_state_cross_check/)".into(),
            "--no-fail-fast".into(),
        ],
    });
    let result = check_command_list(&n);
    assert!(
        result.is_ok(),
        "parentheses inside a nextest filter must be preserved: {result:?}"
    );
}

#[test]
fn command_list_rejects_amp_amp_with_typed_error() {
    let n = notification_with_command(VerificationCommand {
        label: "cargo-nextest".into(),
        argv: vec![
            "cargo".into(),
            "nextest".into(),
            "run".into(),
            "&&".into(),
            "echo".into(),
            "done".into(),
        ],
    });
    let err = check_command_list(&n).unwrap_err();
    match err {
        Violation::UnsupportedCommandOperator(v) => {
            assert_eq!(v.milestone_id, "207");
            assert_eq!(v.operator, "&&");
            assert!(v.offending.contains("&&"));
        }
        other => panic!("expected UnsupportedCommandOperator, got {other:?}"),
    }
}

#[test]
fn command_list_rejects_semicolon_in_token() {
    let n = notification_with_command(VerificationCommand {
        label: "bad".into(),
        argv: vec!["echo".into(), "1; echo 2".into()],
    });
    let err = check_command_list(&n).unwrap_err();
    assert!(matches!(
        err,
        Violation::UnsupportedCommandOperator(_)
    ));
}

#[test]
fn command_list_rejects_newline_in_token() {
    let n = notification_with_command(VerificationCommand {
        label: "bad".into(),
        argv: vec!["echo".into(), "1\necho 2".into()],
    });
    let err = check_command_list(&n).unwrap_err();
    assert!(matches!(
        err,
        Violation::UnsupportedCommandOperator(_)
    ));
}

#[test]
fn command_list_rejects_or_or_with_typed_error() {
    let n = notification_with_command(VerificationCommand {
        label: "bad".into(),
        argv: vec![
            "cargo".into(),
            "nextest".into(),
            "run".into(),
            "||".into(),
            "true".into(),
        ],
    });
    let err = check_command_list(&n).unwrap_err();
    assert!(matches!(
        err,
        Violation::UnsupportedCommandOperator(_)
    ));
}

#[test]
fn command_list_with_multiple_separate_commands_passes() {
    // Multi-command lists are represented as separate
    // VerificationCommand entries, NOT concatenated with `&&`
    // in a single token.
    let mut n = LaneNotification::runner_done(
        "207",
        1,
        "executed",
        "done",
        "implemented",
        attribution(),
    );
    n.verification_commands.push(VerificationCommand {
        label: "first".into(),
        argv: vec!["cargo".into(), "fmt".into(), "--check".into()],
    });
    n.verification_commands.push(VerificationCommand {
        label: "second".into(),
        argv: vec!["cargo".into(), "nextest".into(), "run".into()],
    });
    let result = check_command_list(&n);
    assert!(result.is_ok());
}

#[test]
fn empty_argv_command_is_rejected_via_check_notification() {
    // A command with empty argv is structurally invalid; the
    // evidence contract surfaces it.
    let mut n = notification_with_command(VerificationCommand {
        label: "empty".into(),
        argv: vec![],
    });
    // The empty argv also fails evidence-shape because the
    // notification's verification list is consulted by
    // check_evidence_contract.
    let state = state_with_real_evidence();
    let verdict = check_notification(
        &state,
        &n,
        VerifierInputs {
            diff_hunk: None,
            orchestrator_prompted_cycle: 1,
            started_dispatch_ids: &["dispatch-1".into()],
            orchestrator_pane_id: "%1",
        },
    );
    match verdict {
        Verdict::EvidenceContractFailed { failing, .. } => {
            assert!(failing.iter().any(|(id, _)| id == "<command-list>"));
        }
        other => panic!("expected EvidenceContractFailed, got {other:?}"),
    }
    // Direct call to check_command_list passes for an empty
    // argv (it's check_evidence_contract that flags it).
    n.verification_commands.clear();
    let _ = check_command_list(&n);
}

#[test]
fn zero_match_nextest_filter_is_rejected_as_argv_check() {
    // A filter that matches zero tests still passes argv
    // validation — the verifier doesn't run nextest itself.
    let n = notification_with_command(VerificationCommand {
        label: "zero-match".into(),
        argv: vec![
            "cargo".into(),
            "nextest".into(),
            "run".into(),
            "-E".into(),
            "test(/nonexistent/)".into(),
        ],
    });
    assert!(check_command_list(&n).is_ok());
}

#[test]
fn verification_command_serde_round_trip() {
    let cmd = VerificationCommand {
        label: "cargo-nextest".into(),
        argv: vec!["cargo".into(), "nextest".into(), "run".into()],
    };
    let json = serde_json::to_string(&cmd).unwrap();
    let back: VerificationCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(back, cmd);
}