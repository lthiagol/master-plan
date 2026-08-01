use crate::common::review_queue_fixture::create_and_complete_milestone;
use crate::common::TestEnv;

#[test]
fn bulk_review_pass_requires_all_or_milestone() {
    let env = TestEnv::new();
    // Passing without --all and without milestone should fail
    let out = env.run(&["reviews", "pass", "--verdict", "ok", "--reviewer", "test"]);
    assert!(!out.status.success(), "should fail without milestone ID");
}

#[test]
fn bulk_review_pass_all_resolves_pending() {
    let env = TestEnv::new();
    let id = create_and_complete_milestone(&env, None);

    // Verify pending
    let pending = env.run(&["reviews", "pending", "--format", "json"]);
    let pv: serde_json::Value = serde_json::from_slice(&pending.stdout).unwrap();
    assert!(
        pv["count"].as_u64().unwrap() >= 1,
        "should have pending reviews"
    );

    // Resolve all
    let out = env.run(&[
        "reviews",
        "pass",
        "--all",
        "--verdict",
        "ok",
        "--reviewer",
        "test-bulk",
    ]);
    assert!(
        out.status.success(),
        "bulk pass failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
    assert!(v["total"].as_u64().unwrap() >= 1);
    let results = v["results"].as_array().unwrap();
    let ours = results
        .iter()
        .find(|r| r["milestone_id"].as_str() == Some(&id));
    assert!(ours.is_some(), "bulk should include milestone {}", id);
    assert!(ours.unwrap()["ok"].as_bool().unwrap());
}

#[test]
fn bulk_review_pass_with_filter() {
    let env = TestEnv::new();
    let _id = create_and_complete_milestone(&env, None);

    let out = env.run(&[
        "reviews",
        "pass",
        "--all",
        "--filter",
        "force-bypassed",
        "--verdict",
        "ok",
        "--reviewer",
        "test-filter",
    ]);
    // This should succeed even if empty (filter found no matches)
    assert!(
        out.status.success(),
        "filtered bulk pass failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn bulk_review_pass_unknown_filter_preset_errors() {
    let env = TestEnv::new();
    let _id = create_and_complete_milestone(&env, None);

    // Typo in preset must NOT silently review the unfiltered queue --
    // a wrong filter on `pass --all` could record reviews for milestones
    // the user never intended to target.
    let out = env.run(&[
        "reviews",
        "pass",
        "--all",
        "--filter",
        "force-bypassedd", // intentional typo
        "--verdict",
        "ok",
        "--reviewer",
        "test-typo",
    ]);
    assert!(
        !out.status.success(),
        "unknown filter preset must fail; got success"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown review filter preset") && stderr.contains("force-bypassedd"),
        "error must name the bad preset; got:\n{stderr}"
    );
}

#[test]
fn reviews_pending_unknown_filter_errors_even_with_empty_queue() {
    // Regression guard: when the pending queue is empty AND the user types a
    // typo'd filter, the filter validator must still fire (no items doesn't
    // mean no validation needed).
    let env = TestEnv::new();
    // No milestones created -- pending list is structurally empty.
    let out = env.run(&["reviews", "pending", "--filter", "force-bypassedd"]);
    assert!(
        !out.status.success(),
        "unknown preset must fail even with empty pending queue"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown review filter preset") && stderr.contains("force-bypassedd"),
        "error must name the bad preset; got:\n{stderr}"
    );
}

#[test]
fn reviews_pending_accepts_filter() {
    let env = TestEnv::new();
    let _id = create_and_complete_milestone(&env, None);

    // Default (no --filter) still works and returns the pending count >= 1.
    let baseline = env.run(&["reviews", "pending", "--format", "json"]);
    assert!(
        baseline.status.success(),
        "baseline pending failed: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let baseline_json: serde_json::Value = serde_json::from_slice(&baseline.stdout).unwrap();
    let baseline_count = baseline_json["count"].as_u64().unwrap();
    assert!(
        baseline_count >= 1,
        "expected >= 1 pending; got {baseline_count}"
    );

    // With --filter force-bypassed on `pending`, the command must succeed.
    // (No fixture here carries the [force-bypassed] marker, so count is 0,
    // but the read path must not reject the unknown filter as clap error.)
    let filtered = env.run(&[
        "reviews",
        "pending",
        "--filter",
        "force-bypassed",
        "--format",
        "json",
    ]);
    assert!(
        filtered.status.success(),
        "filtered pending failed: {}",
        String::from_utf8_lossy(&filtered.stderr)
    );
    let fjson: serde_json::Value = serde_json::from_slice(&filtered.stdout).unwrap();
    assert!(
        fjson["count"].as_u64().is_some(),
        "filtered pending missing count key"
    );
    assert!(
        fjson["pending"].as_array().is_some(),
        "filtered pending missing pending array"
    );

    // Unknown preset must FAIL FAST (typo here is `force-bypassedd`),
    // not silently fall through and return the unfiltered list.
    let typo = env.run(&[
        "reviews",
        "pending",
        "--filter",
        "force-bypassedd",
        "--format",
        "json",
    ]);
    assert!(
        !typo.status.success(),
        "unknown preset must fail; got success"
    );
    let stderr = String::from_utf8_lossy(&typo.stderr);
    assert!(
        stderr.contains("unknown review filter preset") && stderr.contains("force-bypassedd"),
        "error must name the bad preset; got:\n{stderr}"
    );
}

#[test]
fn single_review_pass_still_works() {
    let env = TestEnv::new();
    let id = create_and_complete_milestone(&env, None);

    let out = env.run(&[
        "reviews",
        "pass",
        &id,
        "--verdict",
        "ok",
        "--reviewer",
        "test-single",
    ]);
    assert!(
        out.status.success(),
        "single pass failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
    assert!(v["review"]["milestone_id"].as_str().unwrap() == id);
}
