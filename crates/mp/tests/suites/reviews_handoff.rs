//! M133 AC-02: `mp reviews handoff` records a coordinator/runner
//! hand-off on a milestone, consistent with the four-point hand-off
//! contract documented in `mp-flow`'s Hand-off protocol section (data
//! / session-boundary / evidence). The record is persisted atomically
//! in `reviews.json`.

use serde_json::Value;

use crate::common::TestEnv;

fn write_milestone(env: &TestEnv, id: &str, slug: &str, title: &str) {
    use std::fs;
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let milestone = serde_json::json!({
        "milestone": {
            "id": id,
            "title": title,
            "slug": slug,
            "spec_status": "ready",
            "execution_status": "planned",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "priority": "normal",
            "created": "2026-07-09",
            "updated": "2026-07-09",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "x" },
        "problem": { "description": "x" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "x",
            "verification": "manual: test",
            "status": "pending",
            "evidence": "",
        }],
    });
    let json = serde_json::to_string_pretty(&milestone).unwrap();
    fs::write(dir.join(format!("{id}-{slug}.json")), format!("{json}\n")).unwrap();
    let out = env.run(&["sync", "--format", "json"]);
    assert!(
        out.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn reviews_handoff_records_session_boundary_contract() {
    let env = TestEnv::new();
    write_milestone(&env, "133", "reviews-handoff", "Reviews handoff test");

    // Stage 4 → 5 hand-off: coordinator → runner. Mirrors the (a)
    // hand-off point in `mp-flow`'s Hand-off protocol section.
    let record = env.run(&[
        "reviews",
        "handoff",
        "133",
        "--from-session",
        "coordinator",
        "--to-session",
        "runner",
        "--data",
        "approved spec + per-AC verification integrity report (verify-ac green) + G1-G4, G14 all green",
        "--session-boundary",
        "coordinator's planning session (stages 1-4) closes; runner's execution session (stage 5) opens in a fresh session",
        "--evidence",
        "milestone file on disk (source of truth) + integrity report surfaced as part of the hand-off payload",
        "--format",
        "json",
    ]);
    assert!(
        record.status.success(),
        "handoff record failed: {}",
        String::from_utf8_lossy(&record.stderr)
    );
    let v: Value = serde_json::from_slice(&record.stdout).unwrap();
    assert_eq!(v["ok"], true);
    let handoff = &v["handoff"];
    assert_eq!(handoff["id"], "H-01");
    assert_eq!(handoff["milestone_id"], "133");
    assert_eq!(handoff["from_session"], "coordinator");
    assert_eq!(handoff["to_session"], "runner");
    assert!(
        handoff["data"]
            .as_str()
            .unwrap()
            .contains("verification integrity report"),
        "data must persist the hand-off payload"
    );
    assert!(
        handoff["session_boundary"]
            .as_str()
            .unwrap()
            .contains("fresh session"),
        "session_boundary must record the discipline"
    );
    assert!(
        handoff["evidence"]
            .as_str()
            .unwrap()
            .contains("milestone file on disk"),
        "evidence must record the audit trail"
    );
    assert!(
        handoff["created_at"].is_string() && !handoff["created_at"].as_str().unwrap().is_empty(),
        "RFC3339 created_at must be set"
    );

    // Verify the hand-off landed in reviews.json atomically.
    let path = env.tmp.path().join("master-plan/reviews.json");
    let file: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
    let handoffs = file["handoffs"].as_array().unwrap();
    assert_eq!(handoffs.len(), 1);
    assert_eq!(handoffs[0]["id"], "H-01");
    assert_eq!(handoffs[0]["from_session"], "coordinator");
    assert_eq!(handoffs[0]["to_session"], "runner");

    // Stage 7 → 8: runner → coordinator. Mirrors hand-off point (b).
    let record2 = env.run(&[
        "reviews",
        "handoff",
        "133",
        "--from-session",
        "runner",
        "--to-session",
        "coordinator",
        "--data",
        "self-reviewed lifecycle + self-findings (round-1 review at stage 6) + per-step + per-AC evidence",
        "--format",
        "json",
    ]);
    assert!(
        record2.status.success(),
        "second handoff failed: {}",
        String::from_utf8_lossy(&record2.stderr)
    );
    let v: Value = serde_json::from_slice(&record2.stdout).unwrap();
    assert_eq!(v["handoff"]["id"], "H-02");
    assert_eq!(v["handoff"]["from_session"], "runner");
    assert_eq!(v["handoff"]["to_session"], "coordinator");

    // On-disk: both handoffs recorded, oldest-first by created_at.
    let file: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
    let handoffs = file["handoffs"].as_array().unwrap();
    assert_eq!(handoffs.len(), 2);
    assert_eq!(handoffs[0]["id"], "H-01");
    assert_eq!(handoffs[1]["id"], "H-02");
}

#[test]
fn reviews_handoff_validates_draft() {
    let env = TestEnv::new();
    write_milestone(&env, "133", "reviews-handoff-validate", "Handoff validate");

    // Both --from-session and --to-session empty must fail.
    let bad = env.run(&[
        "reviews", "handoff", "133", "--data", "anything", "--format", "json",
    ]);
    assert!(
        !bad.status.success(),
        "empty from+to session must fail; stderr: {}",
        String::from_utf8_lossy(&bad.stderr)
    );

    // Empty data must fail (the contract requires something to pass).
    let bad = env.run(&[
        "reviews",
        "handoff",
        "133",
        "--from-session",
        "coordinator",
        "--to-session",
        "runner",
        "--data",
        "   ",
        "--format",
        "json",
    ]);
    assert!(
        !bad.status.success(),
        "empty data must fail; stderr: {}",
        String::from_utf8_lossy(&bad.stderr)
    );

    // Malformed RFC3339 timestamp must fail.
    let bad = env.run(&[
        "reviews",
        "handoff",
        "133",
        "--from-session",
        "coordinator",
        "--to-session",
        "runner",
        "--data",
        "ok",
        "--at",
        "yesterday",
        "--format",
        "json",
    ]);
    assert!(
        !bad.status.success(),
        "non-RFC3339 timestamp must fail; stderr: {}",
        String::from_utf8_lossy(&bad.stderr)
    );

    // Missing milestone must fail.
    let bad = env.run(&[
        "reviews",
        "handoff",
        "999",
        "--from-session",
        "coordinator",
        "--to-session",
        "runner",
        "--data",
        "ok",
        "--format",
        "json",
    ]);
    assert!(
        !bad.status.success(),
        "missing milestone must fail; stderr: {}",
        String::from_utf8_lossy(&bad.stderr)
    );
}

#[test]
fn reviews_handoff_accepts_only_to_session() {
    // Free-form session names mean a one-sided hand-off is valid (the
    // skill's hand-off points are directional but a real hand-off can
    // be authored from a placeholder role). validate_draft only requires
    // *one* of from/to to be non-empty.
    let env = TestEnv::new();
    write_milestone(
        &env,
        "133",
        "reviews-handoff-one-sided",
        "One-sided handoff",
    );

    let ok = env.run(&[
        "reviews",
        "handoff",
        "133",
        "--to-session",
        "runner",
        "--data",
        "external review with findings passed to runner",
        "--format",
        "json",
    ]);
    assert!(
        ok.status.success(),
        "one-sided handoff (only to_session) must succeed; stderr: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let v: Value = serde_json::from_slice(&ok.stdout).unwrap();
    assert_eq!(v["handoff"]["from_session"], "");
    assert_eq!(v["handoff"]["to_session"], "runner");
}
