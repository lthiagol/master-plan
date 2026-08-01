//! M133 AC-03: comments + handoffs appear in `mp show milestone` output
//! (JSON) and `raul show` consumes the new fields without breaking
//! existing consumers. Backcompat: existing reviews verdict/findings
//! shape unchanged; comments/handoffs are additive arrays.

use serde_json::Value;

use crate::common::lib_api;
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
    let out = lib_api::run(env, &["sync", "--format", "json"]);
    assert!(
        out.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn show_milestone_includes_comments_and_handoffs() {
    let env = TestEnv::new();
    write_milestone(&env, "133", "show-trail", "Show trail test");

    // Add a comment.
    let add = lib_api::run(
        &env,
        &[
            "reviews",
            "comment",
            "add",
            "133",
            "--author",
            "reviewer-a",
            "--body",
            "Looks good.",
            "--format",
            "json",
        ],
    );
    assert!(add.status.success());

    // Record a hand-off.
    let handoff = lib_api::run(
        &env,
        &[
            "reviews",
            "handoff",
            "133",
            "--from-session",
            "coordinator",
            "--to-session",
            "runner",
            "--data",
            "approved spec + integrity report",
            "--session-boundary",
            "fresh session",
            "--evidence",
            "milestone file on disk",
            "--format",
            "json",
        ],
    );
    assert!(handoff.status.success());

    // mp show milestone must surface both arrays.
    let show = lib_api::run(&env, &["show", "milestone", "133", "--format", "json"]);
    assert!(
        show.status.success(),
        "show failed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let v: Value = serde_json::from_slice(&show.stdout).unwrap();

    assert!(
        v["comments"].is_array(),
        "show milestone must include comments array; got: {}",
        v
    );
    assert_eq!(v["comments"].as_array().unwrap().len(), 1);
    assert_eq!(v["comments"][0]["id"], "C-01");
    assert_eq!(v["comments"][0]["author"], "reviewer-a");

    assert!(
        v["handoffs"].is_array(),
        "show milestone must include handoffs array; got: {}",
        v
    );
    assert_eq!(v["handoffs"].as_array().unwrap().len(), 1);
    assert_eq!(v["handoffs"][0]["id"], "H-01");
    assert_eq!(v["handoffs"][0]["from_session"], "coordinator");
    assert_eq!(v["handoffs"][0]["to_session"], "runner");

    // Reviews trail array is also surfaced (parity with `mp reviews show`).
    assert!(
        v["reviews"].is_array(),
        "show milestone must include reviews array"
    );
    assert_eq!(v["reviews"].as_array().unwrap().len(), 0);

    // Existing keys are still present (no shape break). `steps` and
    // `work_packages` use `skip_serializing_if = "Vec::is_empty"` so a
    // milestone with no steps/work_packages will *omit* them from the
    // output rather than surface `[]` — that's the documented model
    // shape, not an M133 regression. Assert the keys that *are* always
    // present.
    assert!(v["milestone"]["id"].as_str() == Some("133"));
    assert!(v["acceptance_criteria"].is_array());
    assert!(v["intent"].is_object());
    assert!(v["scope"].is_object());
}

#[test]
fn show_milestone_supports_fields_projection_on_comments_and_handoffs() {
    // The --fields path must also surface the new arrays — without
    // re-injection they would be overwritten by the raw-on-disk merge.
    let env = TestEnv::new();
    write_milestone(&env, "133", "show-trail-fields", "Show trail fields");

    lib_api::run(
        &env,
        &[
            "reviews",
            "comment",
            "add",
            "133",
            "--author",
            "reviewer-a",
            "--body",
            "First comment",
            "--format",
            "json",
        ],
    );
    lib_api::run(
        &env,
        &[
            "reviews",
            "comment",
            "add",
            "133",
            "--author",
            "reviewer-b",
            "--body",
            "Second comment",
            "--format",
            "json",
        ],
    );

    let show = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            "133",
            "--fields",
            "comments",
            "--format",
            "json",
        ],
    );
    assert!(
        show.status.success(),
        "show --fields comments failed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let v: Value = serde_json::from_slice(&show.stdout).unwrap();
    let comments = v["comments"]
        .as_array()
        .expect("comments array must surface under --fields projection");
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0]["id"], "C-01");
    assert_eq!(comments[1]["id"], "C-02");
}

#[test]
fn show_milestone_omits_review_trail_when_reviews_json_missing() {
    // Pre-M133 plans have no reviews.json. mp show must still work
    // (return empty arrays, not panic).
    let env = TestEnv::new();
    write_milestone(&env, "133", "show-trail-empty", "Show trail empty");

    // Defensively remove reviews.json if it exists (the test harness
    // may have created it via other tests' shared plan_dir).
    let reviews_path = env.tmp.path().join("master-plan/reviews.json");
    if reviews_path.exists() {
        std::fs::remove_file(&reviews_path).unwrap();
    }

    let show = lib_api::run(&env, &["show", "milestone", "133", "--format", "json"]);
    assert!(
        show.status.success(),
        "show with no reviews.json must succeed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let v: Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(v["comments"].as_array().unwrap().len(), 0);
    assert_eq!(v["handoffs"].as_array().unwrap().len(), 0);
    assert_eq!(v["reviews"].as_array().unwrap().len(), 0);
}

#[test]
fn show_milestone_surfaces_corrupt_reviews_json() {
    // M133 review remediation: a corrupt reviews.json (present but
    // unparseable) is a data-integrity defect, not the benign missing-
    // file case. mp show must surface it via a non-null
    // `review_trail_error` field (BF-17 lanes_error pattern) rather
    // than silently masking it as empty arrays. The command still
    // succeeds (show must not block on a review-file read failure).
    let env = TestEnv::new();
    write_milestone(&env, "133", "show-trail-corrupt", "Show trail corrupt");

    // Write a corrupt reviews.json.
    let reviews_path = env.tmp.path().join("master-plan/reviews.json");
    std::fs::write(&reviews_path, "{ this is not valid json").unwrap();

    let show = lib_api::run(&env, &["show", "milestone", "133", "--format", "json"]);
    assert!(
        show.status.success(),
        "show with corrupt reviews.json must still succeed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let v: Value = serde_json::from_slice(&show.stdout).unwrap();
    // Arrays fall back to empty (backcompat) ...
    assert_eq!(v["comments"].as_array().unwrap().len(), 0);
    assert_eq!(v["handoffs"].as_array().unwrap().len(), 0);
    // ... but the defect is surfaced, not hidden.
    let err = v["review_trail_error"].as_str();
    assert!(
        err.map(|e| !e.is_empty()).unwrap_or(false),
        "corrupt reviews.json must surface a non-empty review_trail_error; got: {v}"
    );
}
