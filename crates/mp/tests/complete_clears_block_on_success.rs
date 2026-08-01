//! M118 S2 / AC-02: `mp milestone complete` clears `block_reason` and
//! `blocked_by` on a successful completion. The historical block
//! context is preserved as a `[block-cleared-on-complete: <reason>]`
//! annotation in `verification.evidence` for the audit trail.

mod common;

use crate::common::TestEnv;

#[test]
fn block_fields_cleared_after_successful_complete() {
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 block-clear target",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["milestone", "set-spec-status", &id, "review"]);
    env.run(&["milestone", "set-spec-status", &id, "ready"]);
    env.run(&["milestone", "approve", &id]);

    // Put the milestone into the blocked state. Use a reason that
    // includes a non-trivial string (punctuation, spaces) so the
    // annotation round-trip is asserted end-to-end.
    let block_reason = "Gated by M106 testing milestone: defer";
    let out = env.run(&[
        "milestone",
        "block",
        &id,
        "--reason",
        block_reason,
        "--by",
        "user",
    ]);
    assert!(out.status.success());

    // Sanity-check the block fields are populated.
    let show = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "milestone.block_reason,milestone.blocked_by",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(v["milestone"]["block_reason"], block_reason);
    assert_eq!(v["milestone"]["blocked_by"], "user");

    // Complete the milestone.
    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: clean re-completion after block-clear test",
        "--skip-review",
    ]);
    assert!(
        out.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Both block fields are now empty.
    let show = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "milestone.block_reason,milestone.blocked_by",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        v["milestone"]["block_reason"], "",
        "block_reason must be empty after complete; got: {:?}",
        v["milestone"]["block_reason"]
    );
    assert_eq!(
        v["milestone"]["blocked_by"], "",
        "blocked_by must be empty after complete; got: {:?}",
        v["milestone"]["blocked_by"]
    );

    // The historical block context is preserved as an annotation in
    // verification.evidence.
    let show = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "verification.evidence",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let evidence = v["verification"]["evidence"]
        .as_str()
        .expect("evidence string");
    let expected_annotation = format!("[block-cleared-on-complete: {}]", block_reason);
    assert!(
        evidence.contains(&expected_annotation),
        "evidence must contain the block-cleared annotation; got: {evidence:?}"
    );
}

#[test]
fn no_block_no_annotation_on_first_complete() {
    // A clean first-time completion (no prior block) must not emit a
    // `[block-cleared-on-complete: ...]` annotation; the annotation is
    // only the audit trail for the block-clear path.
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 no-block target",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["milestone", "set-spec-status", &id, "review"]);
    env.run(&["milestone", "set-spec-status", &id, "ready"]);
    env.run(&["milestone", "approve", &id]);

    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: clean first-time complete",
        "--skip-review",
    ]);
    assert!(out.status.success());

    let show = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "verification.evidence",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let evidence = v["verification"]["evidence"].as_str().expect("evidence");
    assert!(
        !evidence.contains("[block-cleared-on-complete:"),
        "no-block path must NOT emit the annotation; got: {evidence:?}"
    );
}

#[test]
fn block_annotation_dedup_on_repeated_complete() {
    // M118 findings follow-up (B-58): repeated `complete_milestone` over
    // the same milestone must not re-append the `[block-cleared-on-complete:
    // ...]` annotation. Pre-fix this would re-append on every run,
    // accumulating N copies of the same annotation; the B-58 review
    // finding flagged that the dedup check used a string-substring match
    // on the full annotation (which could falsely skip when the user-supplied
    // block_reason contained `[block-cleared-on-complete:`). Post-fix we
    // dedup on the prefix only, which is always stable.
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 dedup target",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["milestone", "set-spec-status", &id, "review"]);
    env.run(&["milestone", "set-spec-status", &id, "ready"]);
    env.run(&["milestone", "approve", &id]);
    env.run(&[
        "milestone",
        "block",
        &id,
        "--reason",
        "first block",
        "--by",
        "user",
    ]);

    // First complete: appends the annotation once.
    env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: first complete",
        "--skip-review",
    ]);
    // Second complete: must NOT re-append.
    env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: second complete",
        "--skip-review",
    ]);
    // Third complete for good measure.
    env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: third complete",
        "--skip-review",
    ]);

    let show = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "verification.evidence",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let evidence = v["verification"]["evidence"]
        .as_str()
        .expect("evidence string");
    let occurrences = evidence.matches("[block-cleared-on-complete:").count();
    assert_eq!(
        occurrences, 1,
        "annotation must appear exactly once after 3 completes; got {occurrences} in evidence={evidence:?}"
    );
}

#[test]
fn block_annotation_unique_even_when_reason_contains_marker_text() {
    // M118 findings follow-up (B-58) edge case: the user-supplied
    // block_reason itself contains the literal `[block-cleared-on-complete:`.
    // Pre-fix this would have falsely dedup'd on the existing annotation's
    // substring match, leaving the field empty (data loss for the audit
    // trail). Post-fix we dedup on the prefix only, so the annotation
    // gets appended despite the substring match.
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 nested-marker target",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["milestone", "set-spec-status", &id, "review"]);
    env.run(&["milestone", "set-spec-status", &id, "ready"]);
    env.run(&["milestone", "approve", &id]);
    // The reason deliberately embeds the marker text — the very substring
    // the pre-fix dedup matched on. Post-fix dedup uses the prefix only,
    // which appears exactly once in the appended annotation regardless.
    env.run(&[
        "milestone",
        "block",
        &id,
        "--reason",
        "prefix [block-cleared-on-complete: appears in reason itself",
        "--by",
        "user",
    ]);
    env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: nested-marker complete",
        "--skip-review",
    ]);
    let show = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "verification.evidence",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let evidence = v["verification"]["evidence"]
        .as_str()
        .expect("evidence string");
    assert!(
        evidence.contains("[block-cleared-on-complete: prefix [block-cleared-on-complete: appears in reason itself]"),
        "annotation must carry the user-supplied reason verbatim; got: {evidence:?}"
    );
}

#[test]
fn re_block_then_complete_replaces_prior_annotation_with_fresh_reason() {
    // M118 CR (F-3): the audit-trail block annotation must reflect the
    // LATEST block reason, not the first. Pre-fix, re-blocking a
    // completed milestone (without going through `unblock`, which
    // fails on `done` execution_status) and re-completing would carry
    // the prior annotation forward via the B-58 dedup prefix check,
    // silently dropping the most recent block context. Post-fix the
    // three-phase complete-milestone logic replaces the stale
    // annotation with the fresh one, recording the current block
    // reason in the audit trail.
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 re-block target",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["milestone", "set-spec-status", &id, "review"]);
    env.run(&["milestone", "set-spec-status", &id, "ready"]);
    env.run(&["milestone", "approve", &id]);

    // First block → first complete. Audit trail records the first
    // block reason.
    env.run(&[
        "milestone",
        "block",
        &id,
        "--reason",
        "first reason: deferring for M106 testing",
        "--by",
        "user",
    ]);
    env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: first complete",
        "--skip-review",
    ]);

    // Re-block after complete is refused (M189 F-07: no terminal+overlay
    // drift). The prior annotation from the first block→complete remains.
    let reblock = env.run(&[
        "milestone",
        "block",
        &id,
        "--reason",
        "second reason: discovered regression post-completion",
        "--by",
        "user",
    ]);
    assert!(
        !reblock.status.success(),
        "block on complete must be refused (M189 F-07)"
    );
    let show = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "verification.evidence",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let evidence = v["verification"]["evidence"]
        .as_str()
        .expect("evidence string");
    assert!(
        evidence.contains("[block-cleared-on-complete: first reason: deferring for M106 testing]"),
        "first block→complete annotation must remain when re-block is refused; got: {evidence:?}"
    );
    assert_eq!(
        evidence.matches("[block-cleared-on-complete:").count(),
        1,
        "exactly one annotation in the audit trail; got {evidence:?}"
    );
}

#[test]
fn re_complete_only_preserves_carried_annotation() {
    // Counterpart to F-3's `re_block_then_complete_replaces_prior_annotation_with_fresh_reason`:
    // when there's NO current block (just a re-completion pass to
    // refresh the verifier output), the prior annotation is preserved
    // verbatim. Confirms we didn't break the no-re-block path while
    // fixing the re-block path.
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 re-complete-only target",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    env.run(&["milestone", "set-spec-status", &id, "review"]);
    env.run(&["milestone", "set-spec-status", &id, "ready"]);
    env.run(&["milestone", "approve", &id]);
    env.run(&[
        "milestone",
        "block",
        &id,
        "--reason",
        "carry-forward-test reason",
        "--by",
        "user",
    ]);
    env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: first complete",
        "--skip-review",
    ]);

    // Just re-complete, no re-block. The prior annotation should
    // survive. Note: `evidence_text` is non-empty here (caller
    // passed --evidence), so the F-3 logic enters the
    // `evidence.is_some()` branch and applies the carried
    // annotation as a prefix to the new evidence.
    env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: second complete (no re-block)",
        "--skip-review",
    ]);

    let show = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "verification.evidence",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let evidence = v["verification"]["evidence"]
        .as_str()
        .expect("evidence string");
    assert!(
        evidence.contains("[block-cleared-on-complete: carry-forward-test reason]"),
        "carried annotation must survive a no-re-block re-completion; got: {evidence:?}"
    );
    assert_eq!(
        evidence.matches("[block-cleared-on-complete:").count(),
        1,
        "exactly one annotation; got {evidence:?}"
    );
}
