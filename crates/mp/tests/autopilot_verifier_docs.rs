//! M212 / AC-05: documentation describes (a) the `mp reviews pass`
//! writing pattern (writes to milestone JSON + reviews.json; does
//! NOT add an activity.json event by default); (b) the cycle 1
//! recovery lesson (review-pass lost to activity.json truncation;
//! verifier cross-checks three sources to catch it); (c) the
//! multi-source verification convention (milestone JSON +
//! reviews.json + activity.json).

use std::path::PathBuf;

const DOC_PATH: &str = "docs/mp/autopilot-verifier.md";

fn doc_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join(DOC_PATH)
}

fn doc_text() -> String {
    let path = doc_path();
    assert!(
        path.exists(),
        "documentation file does not exist: {}",
        path.display()
    );
    std::fs::read_to_string(&path).expect("read verifier documentation")
}

#[test]
fn doc_describes_mp_reviews_pass_writing_pattern() {
    let doc = doc_text();
    // (a) `mp reviews pass` writing pattern: writes to
    // reviews.json AND milestone JSON, does NOT add an
    // activity.json event by default.
    assert!(
        doc.contains("mp reviews pass"),
        "documentation must mention `mp reviews pass` writing pattern"
    );
    assert!(
        doc.contains("reviews.json"),
        "documentation must reference reviews.json as the durable review record"
    );
    assert!(
        doc.contains("does NOT"),
        "documentation must explicitly state what reviews pass does NOT do"
    );
    assert!(
        doc.contains("activity.json"),
        "documentation must explain activity.json is NOT touched by reviews pass"
    );
    assert!(
        doc.contains("external-review")
            && doc.contains("flow_stages"),
        "documentation must explain the milestone JSON flow_stages side effect"
    );
}

#[test]
fn doc_describes_cycle_1_recovery_lesson() {
    let doc = doc_text();
    // (b) The cycle 1 recovery lesson: a review-pass record
    // lost to activity.json truncation; the verifier
    // cross-checks three sources to catch it. The doc uses
    // "cycle 1" rather than the milestone ID (consumer-surface
    // hygiene).
    assert!(
        doc.contains("cycle 1") || doc.contains("Cycle 1"),
        "documentation must describe the cycle 1 recovery lesson"
    );
    assert!(
        doc.contains("fabricat") || doc.contains("trust"),
        "documentation must explain the fabrication / trust gap"
    );
    assert!(
        doc.contains("three sources"),
        "documentation must explain the three-source verification convention"
    );
}

#[test]
fn doc_describes_multi_source_verification_convention() {
    let doc = doc_text();
    // (c) Multi-source verification convention: milestone JSON
    // + reviews.json + activity.json.
    assert!(
        doc.contains("Milestone JSON"),
        "documentation must list milestone JSON as a source"
    );
    assert!(
        doc.contains("reviews.json"),
        "documentation must list reviews.json as a source"
    );
    assert!(
        doc.contains("activity.json"),
        "documentation must list activity.json as a source"
    );
    assert!(
        doc.contains("cross-checks") || doc.contains("cross-check"),
        "documentation must explain cross-checking all three sources"
    );
}

#[test]
fn doc_lists_seven_typed_role_boundary_detectors() {
    let doc = doc_text();
    // The seven detectors and their lanes are documented as a
    // table; each lane role appears in the table.
    assert!(
        doc.contains("RunnerReviewViolation"),
        "documentation must list detector 1"
    );
    assert!(
        doc.contains("RunnerClaimViolation"),
        "documentation must list detector 2"
    );
    assert!(
        doc.contains("RunnerPlanEditViolation"),
        "documentation must list detector 3"
    );
    assert!(
        doc.contains("ReviewerCodeEditViolation"),
        "documentation must list detector 4"
    );
    assert!(
        doc.contains("ReviewerPrematurePassViolation"),
        "documentation must list detector 5"
    );
    assert!(
        doc.contains("PreStartNotificationViolation"),
        "documentation must list detector 6"
    );
    assert!(
        doc.contains("OrchestratorCodeEditViolation"),
        "documentation must list detector 7"
    );
}

#[test]
fn doc_describes_actor_attribution_with_five_fields() {
    let doc = doc_text();
    // AC-06: every autopilot mutation carries session_id, role,
    // actor_token, dispatch_id, and seq. The documentation
    // documents all five fields.
    assert!(doc.contains("session_id"), "documentation must list session_id");
    assert!(doc.contains("role"), "documentation must list role");
    assert!(
        doc.contains("actor_token"),
        "documentation must list actor_token"
    );
    assert!(
        doc.contains("dispatch_id"),
        "documentation must list dispatch_id"
    );
    assert!(doc.contains("seq"), "documentation must list seq");
}

#[test]
fn doc_describes_topology_aware_remediation() {
    let doc = doc_text();
    assert!(
        doc.contains("3-pane") || doc.contains("three-agent"),
        "documentation must cover 3-pane remediation"
    );
    assert!(
        doc.contains("2-pane") || doc.contains("two-agent"),
        "documentation must cover 2-pane remediation"
    );
    assert!(
        doc.contains("1-pane") || doc.contains("one-agent"),
        "documentation must cover 1-pane remediation"
    );
    assert!(
        doc.contains("Resend") || doc.contains("resend"),
        "documentation must explain the Resend remediation"
    );
    assert!(
        doc.contains("EscalateToUser") || doc.contains("Escalate"),
        "documentation must explain the EscalateToUser remediation"
    );
}

#[test]
fn doc_explains_per_ac_evidence_contract() {
    let doc = doc_text();
    // The contract: exact command + exit code + observed pass
    // count. Generic summaries are rejected.
    assert!(
        doc.contains("cargo nextest"),
        "documentation must show real cargo nextest evidence example"
    );
    assert!(
        doc.contains("exit 0"),
        "documentation must show the exit-code token"
    );
    assert!(
        doc.contains("pass") && doc.contains("(/") && doc.contains("/"),
        "documentation must show the (<passed>/<total> pass) count"
    );
    assert!(
        doc.contains("Generic") || doc.contains("generic"),
        "documentation must warn against generic-summary evidence"
    );
}

#[test]
fn doc_explains_command_list_operator_rejection() {
    let doc = doc_text();
    // AC-08: shell control operators are rejected, not
    // silently skipped.
    assert!(
        doc.contains("`&&`") || doc.contains("&&"),
        "documentation must cover && rejection"
    );
    assert!(
        doc.contains("`;`") || doc.contains(";"),
        "documentation must cover ; rejection"
    );
    assert!(
        doc.contains("parentheses") || doc.contains("argv"),
        "documentation must cover nextest-filter parentheses preserved as argv"
    );
}

#[test]
fn doc_does_not_leak_internal_milestone_ids() {
    let doc = doc_text();
    // Consumer-surface hygiene: no internal milestone IDs (M\d+)
    // on the consumer-facing documentation.
    let re = regex::Regex::new(r"\bM\d{2,4}\b").unwrap();
    let matches: Vec<_> = re.find_iter(&doc).collect();
    assert!(
        matches.is_empty(),
        "documentation must not leak internal milestone IDs (M\\d+); found: {:?}",
        matches
    );
}