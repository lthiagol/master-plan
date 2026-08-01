//! Shared fixture helpers for review-queue and finding tests (M171 / TW-18).
//!
//! These functions used to be copy-pasted across
//! `crates/mp/tests/suites/reviews_queue.rs`,
//! `crates/mp/tests/suites/review_discovery.rs`,
//! `crates/mp/tests/suites/review_lifecycle.rs`, and
//! `crates/mp/tests/suites/reviews_bulk.rs`. Two duplications were
//! byte-for-byte identical (`write_done_milestone`) and two were
//! near-clones differing only in signature detail
//! (`create_and_complete` with optional executor vs.
//! `create_and_complete_milestone` without). The shared module is the
//! single source of truth — call sites `use crate::common::review_queue_fixture::*;`.
//!
//! The regression test `shared_helpers_are_importable` (below) pins the
//! import surface so a future refactor cannot silently move a helper
//! back into a test file.

use crate::common::TestEnv;

/// Write a fully-shaped "done" milestone JSON directly to the fixture's
/// `master-plan/milestones/` directory and run `mp sync` to refresh the
/// plan index. Used by tests that need a done-but-unreviewed milestone
/// to populate the review queue without driving the full
/// create → approve → wp → step → complete CLI flow.
pub fn write_done_milestone(env: &TestEnv, id: &str, slug: &str, title: &str) {
    use std::fs;
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let milestone = serde_json::json!({
        "milestone": {
            "id": id,
            "title": title,
            "slug": slug,
            "spec_status": "verified",
            "execution_status": "done",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "priority": "normal",
            "created": "2026-06-30",
            "updated": "2026-06-30",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "x" },
        "problem": { "description": "x" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "verification": { "date": "2026-06-30", "branch": "", "evidence": "shipped" },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "done",
            "verification": "manual: accepted — test",
            "status": "passed",
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

/// Write a fully-shaped "spec-review" milestone JSON (spec_status:
/// review, execution_status: planned) and run `mp sync`. Counterpart
/// to [`write_done_milestone`] for the spec-review side of the unified
/// review queue (M90/TW-18 / M171 external-review F-03).
///
/// Both helpers share an identical structural envelope
/// (`{id, slug, title}` + filesystem write + `mp sync`); only the
/// milestone status fields and AC `status` differ. They are kept as
/// two named helpers — rather than one parameterised function — so
/// call sites read at the level of intent (`write_done_milestone` /
/// `write_spec_review_milestone`) and so a future divergence (e.g.,
/// the spec-review variant gaining an interview checklist) lands in
/// one helper without disturbing the other.
pub fn write_spec_review_milestone(env: &TestEnv, id: &str, slug: &str, title: &str) {
    use std::fs;
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let milestone = serde_json::json!({
        "milestone": {
            "id": id,
            "title": title,
            "slug": slug,
            "spec_status": "review",
            "execution_status": "planned",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "priority": "normal",
            "created": "2026-07-02",
            "updated": "2026-07-02",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "x" },
        "problem": { "description": "x" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "verification": { "date": "", "branch": "", "evidence": "" },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "spec ready",
            "verification": "manual: accepted — test",
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

/// Drive the full CLI flow to produce a `done` milestone that lands in
/// the execution-review queue: create → approve → add WP → add step →
/// set in-progress → mark step done → complete. Returns the milestone id.
///
/// `executor` is forwarded to `mp milestone complete --executor <exec>`
/// so call sites that assert on `milestone.executed_by` (review_lifecycle.rs
/// executor attribution tests) can pin a specific value; pass `None` to
/// omit the flag entirely.
///
/// M196: the helper passes `--skip-review` so the milestone reaches
/// terminal `complete` (the review gate is not the subject of these
/// tests; the review lifecycle tests create a fresh milestone at
/// `executed` via `create_and_complete` if they want to exercise the
/// unreviewed end-state). The skip-review flag records `[skip-review]`
/// in evidence so the debt is visible per-call.
///
/// Replaces the prior `create_and_complete` (review_lifecycle.rs) and
/// `create_and_complete_milestone` (reviews_bulk.rs) helpers.
pub fn create_and_complete_milestone(env: &TestEnv, executor: Option<&str>) -> String {
    let json = serde_json::json!({
        "title": "Review Lifecycle Test",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {"outcome": "Test review lifecycle"},
        "problem": {"description": "Testing review lifecycle."},
        "scope": {"in_scope": ["review"], "out_of_scope": ["x", "y"]},
        "acceptance_criteria": [
            {"description": "AC1", "verification": "manual: ok"}
        ]
    });
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        &json.to_string(),
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = v["milestone"]["id"].as_str().unwrap().to_string();

    let assert_ok = |label: &str, out: std::process::Output| {
        assert!(
            out.status.success(),
            "{label} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    assert_ok(
        "approve",
        env.run(&["milestone", "approve", &id, "--format", "json"]),
    );
    assert_ok(
        "wp add",
        env.run(&["milestone", "wp", "add", &id, "--name", "WP1"]),
    );
    assert_ok(
        "step add",
        env.run(&[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "task",
            "--tests",
            "manual: ok",
        ]),
    );
    assert_ok(
        "set-status in-progress",
        env.run(&["milestone", "set-status", &id, "in-progress"]),
    );
    assert_ok(
        "step set-status done",
        env.run(&["milestone", "step", "set-status", &id, "S1", "done"]),
    );

    let mut args: Vec<&str> = vec!["milestone", "complete", &id, "--skip-review"];
    if let Some(exec) = executor {
        args.push("--executor");
        args.push(exec);
    }
    assert_ok("complete", env.run(&args));

    id
}

#[cfg(test)]
mod shared_helpers_are_importable {
    //! Regression test (M171 AC-04): if these helpers ever move out of
    //! `crate::common::review_queue_fixture::*`, the import sites in
    //! the four migrated suites will fail to compile and this assertion
    //! fires. The body itself is intentionally trivial — the value is
    //! in the import site, not the runtime check.

    use super::*;

    #[test]
    fn import_surface_is_stable() {
        let env = TestEnv::new();
        // Touch each helper so the compiler cannot drop the import as
        // unused, and so a future refactor that renames either function
        // produces a compile error in this regression test rather than
        // silently at a downstream call site.
        write_done_milestone(
            &env,
            "fixture-import-check",
            "shared",
            "Shared Import Check",
        );
        write_spec_review_milestone(
            &env,
            "fixture-import-check-spec",
            "shared",
            "Shared Spec Import Check",
        );
        // create_and_complete_milestone is the heavier path; we
        // deliberately do not run it here — it would add a full
        // create→complete round-trip to every test run. The function
        // pointer reference below is enough to pin the import.
        let _: fn(&TestEnv, Option<&str>) -> String = create_and_complete_milestone;
    }
}
