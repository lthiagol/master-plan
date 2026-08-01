//! M133 AC-01: `mp reviews comment add` and `mp reviews comment list`
//! roundtrip — structured review comments on a milestone (author, body,
//! optional finding link, RFC3339 timestamp), persisted atomically in
//! `reviews.json`.

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

fn read_reviews_file(env: &TestEnv) -> Value {
    let path = env.tmp.path().join("master-plan/reviews.json");
    assert!(
        path.exists(),
        "reviews.json should exist at {}",
        path.display()
    );
    let text = std::fs::read_to_string(&path).expect("read reviews.json");
    serde_json::from_str(&text).expect("parse reviews.json")
}

#[test]
fn reviews_comment_add_and_list_roundtrip() {
    let env = TestEnv::new();
    write_milestone(&env, "133", "reviews-comments", "Reviews comments test");

    // Add a first comment.
    let add = env.run(&[
        "reviews",
        "comment",
        "add",
        "133",
        "--author",
        "reviewer-a",
        "--body",
        "First observation: the contract reads cleanly.",
        "--format",
        "json",
    ]);
    assert!(
        add.status.success(),
        "comment add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let v: Value = serde_json::from_slice(&add.stdout).unwrap();
    assert_eq!(v["ok"], true, "ok=true");
    assert_eq!(v["comment"]["id"], "C-01");
    assert_eq!(v["comment"]["milestone_id"], "133");
    assert_eq!(v["comment"]["author"], "reviewer-a");
    assert_eq!(
        v["comment"]["body"],
        "First observation: the contract reads cleanly."
    );
    assert!(
        v["comment"]["created_at"].is_string()
            && !v["comment"]["created_at"].as_str().unwrap().is_empty(),
        "RFC3339 created_at must be set"
    );
    assert!(
        !v["comment"].as_object().unwrap().contains_key("finding_id"),
        "unlinked comment must omit finding_id (skip_serializing_if = is_empty)"
    );

    // List — should return the comment oldest-first.
    let list = env.run(&["reviews", "comment", "list", "133", "--format", "json"]);
    assert!(
        list.status.success(),
        "comment list failed: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let v: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(v["milestone"], "133");
    assert_eq!(v["count"], 1);
    let comments = v["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["id"], "C-01");
    assert_eq!(comments[0]["author"], "reviewer-a");

    // Create a real finding so the comment's --finding link is a valid
    // referential reference (M133 review remediation: --finding is now
    // existence-checked, not just shape-checked).
    let finding = env.run(&[
        "reviews",
        "finding",
        "add",
        "133",
        "--severity",
        "high",
        "--category",
        "correctness",
        "--desc",
        "scope drift in AC-01",
        "--format",
        "json",
    ]);
    assert!(
        finding.status.success(),
        "finding add failed: {}",
        String::from_utf8_lossy(&finding.stderr)
    );
    let fv: Value = serde_json::from_slice(&finding.stdout).unwrap();
    let finding_id = fv["finding"]["id"]
        .as_str()
        .expect("finding id")
        .to_string();

    // Add a second comment with a finding link.
    let add2 = env.run(&[
        "reviews",
        "comment",
        "add",
        "133",
        "--author",
        "reviewer-b",
        "--body",
        "Acknowledged; the link confirms scope.",
        "--finding",
        &finding_id,
        "--format",
        "json",
    ]);
    assert!(
        add2.status.success(),
        "comment add (linked) failed: {}",
        String::from_utf8_lossy(&add2.stderr)
    );
    let v: Value = serde_json::from_slice(&add2.stdout).unwrap();
    assert_eq!(v["comment"]["id"], "C-02");
    assert_eq!(v["comment"]["finding_id"], finding_id);

    // Verify the on-disk reviews.json is the single source of truth
    // and that the file carries both comments. Atomic-write semantics
    // mean the file is either pre-add or post-add — never partial.
    let file = read_reviews_file(&env);
    let comments = file["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0]["id"], "C-01");
    assert_eq!(comments[1]["id"], "C-02");

    // List again — still oldest-first.
    let list2 = env.run(&["reviews", "comment", "list", "133", "--format", "json"]);
    let v: Value = serde_json::from_slice(&list2.stdout).unwrap();
    let comments = v["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0]["id"], "C-01");
    assert_eq!(comments[1]["id"], "C-02");
    // The second comment must come AFTER the first chronologically.
    assert!(
        comments[0]["created_at"].as_str().unwrap() <= comments[1]["created_at"].as_str().unwrap(),
        "comments must be oldest-first (chronological)"
    );
}

#[test]
fn reviews_comment_add_validates_draft() {
    let env = TestEnv::new();
    write_milestone(
        &env,
        "133",
        "reviews-comments-validate",
        "Reviews comments validate",
    );

    // Empty author must fail.
    let bad = env.run(&[
        "reviews", "comment", "add", "133", "--author", "   ", "--body", "ok", "--format", "json",
    ]);
    assert!(!bad.status.success(), "empty author must fail");

    // Empty body must fail.
    let bad = env.run(&[
        "reviews",
        "comment",
        "add",
        "133",
        "--author",
        "reviewer-a",
        "--body",
        "",
        "--format",
        "json",
    ]);
    assert!(!bad.status.success(), "empty body must fail");

    // Malformed finding link must fail.
    let bad = env.run(&[
        "reviews",
        "comment",
        "add",
        "133",
        "--author",
        "reviewer-a",
        "--body",
        "ok",
        "--finding",
        "B-01",
        "--format",
        "json",
    ]);
    assert!(!bad.status.success(), "non-F-NN finding link must fail");

    // Malformed RFC3339 timestamp must fail.
    let bad = env.run(&[
        "reviews",
        "comment",
        "add",
        "133",
        "--author",
        "reviewer-a",
        "--body",
        "ok",
        "--at",
        "2026-07-09",
        "--format",
        "json",
    ]);
    assert!(
        !bad.status.success(),
        "non-RFC3339 timestamp must fail; got: {}",
        String::from_utf8_lossy(&bad.stderr)
    );

    // Missing milestone must fail (parallel to handoff test).
    let bad = env.run(&[
        "reviews",
        "comment",
        "add",
        "999",
        "--author",
        "reviewer-a",
        "--body",
        "ok",
        "--format",
        "json",
    ]);
    assert!(
        !bad.status.success(),
        "missing milestone must fail; stderr: {}",
        String::from_utf8_lossy(&bad.stderr)
    );
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("not found"),
        "error must mention 'not found'; stderr: {}",
        String::from_utf8_lossy(&bad.stderr)
    );
}

#[test]
fn reviews_comment_rejects_dangling_finding_link() {
    // M133 review remediation: a well-formed F-NN that does NOT exist
    // on the milestone must be rejected. Pre-remediation, only the
    // shape was validated, so --finding F-99 silently created a
    // durable dangling reference. This milestone has no findings, so
    // any F-NN link is dangling.
    let env = TestEnv::new();
    write_milestone(
        &env,
        "133",
        "reviews-comments-dangling",
        "Reviews comments dangling",
    );

    let bad = env.run(&[
        "reviews",
        "comment",
        "add",
        "133",
        "--author",
        "reviewer-a",
        "--body",
        "links to a finding that does not exist",
        "--finding",
        "F-99",
        "--format",
        "json",
    ]);
    assert!(
        !bad.status.success(),
        "dangling finding link must fail; stderr: {}",
        String::from_utf8_lossy(&bad.stderr)
    );
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("F-99") && stderr.contains("not found"),
        "error must name the finding and say 'not found'; stderr: {stderr}"
    );
}

#[test]
fn reviews_comment_round_trip_through_legacy_reviews_json() {
    // Backward-compat: a reviews.json with no `comments` field (pre-M133
    // on-disk shape) must load as an empty comments list and the first
    // add must land as C-01 (not C-NN with N>1).
    let env = TestEnv::new();
    write_milestone(&env, "133", "reviews-comments-legacy", "Legacy shape");

    let reviews_path = env.tmp.path().join("master-plan/reviews.json");
    // Seed a legacy-shape reviews.json (only `reviews` field, no
    // `comments` / `handoffs`).
    std::fs::write(&reviews_path, "{\n  \"reviews\": []\n}\n").unwrap();

    let add = env.run(&[
        "reviews",
        "comment",
        "add",
        "133",
        "--author",
        "reviewer-a",
        "--body",
        "First comment after legacy load.",
        "--format",
        "json",
    ]);
    assert!(
        add.status.success(),
        "add after legacy load failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let v: Value = serde_json::from_slice(&add.stdout).unwrap();
    assert_eq!(
        v["comment"]["id"], "C-01",
        "first comment after legacy load must be C-01"
    );
}
