//! M102 R3 (F-01 + F-09 + F-10 + F-12): the rewritten cmd_status consumes
//! path_engine::build_lanes LaneSummary; the lane field is the lane
//! name string (not Debug of the enum); the total_effort field is
//! omitted from the wire format when empty.
//!
//! Also covers the F-09 fix: --lane <name> looks up by name (not
//! positional index) so reordering the LaneArg enum doesn't break
//! resolution.

use crate::common::lib_api;
use crate::common::TestEnv;

/// M102 R4 (F-01): `mp status` consumes build_lanes LaneSummary. The
/// output has `lanes.execution / lanes.review / lanes.grooming /
/// lanes.backlog` keys (with optional `total_effort` when present).
/// The legacy `milestones.{total, by_execution_status, by_spec_status,
/// by_lifecycle, track_pending, annotations_open}` keys are kept as
/// a deprecation alias (C-2 fix) for existing raul consumers; they
/// are derived from the same build_lanes LaneReport so they don't
/// drift. The new `lanes` block is the canonical source.
#[test]
fn status_consumes_lane_summary() {
    let env = TestEnv::new();
    let out = lib_api::run(&env, &["status", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["lanes"].is_object(),
        "status must include `lanes` (LaneSummary); got: {}",
        v
    );
    // The new wire shape has these per-lane keys (with 0 default).
    for lane in ["blocked", "execution", "review", "grooming", "backlog"] {
        assert!(
            v["lanes"][lane].is_number(),
            "status.lanes.{lane} should be a count; got {}",
            v["lanes"]
        );
    }
    // Legacy keys preserved as a deprecation alias (C-2 fix).
    assert!(
        v["milestones"]["total"].is_number(),
        "milestones.total preserved as alias; got: {}",
        v["milestones"]
    );
    assert!(
        v["milestones"]["by_execution_status"].is_object(),
        "milestones.by_execution_status preserved as alias; got: {}",
        v["milestones"]
    );
    assert!(
        v["track_pending"].is_number(),
        "track_pending preserved as alias; got: {}",
        v
    );
}

/// M102 R3 (F-12): `total_effort` is omitted from the wire format when
/// empty (the "always-'-'" placeholder was a smell for consumers).
#[test]
fn lane_summary_total_effort_omitted_when_unset() {
    let env = TestEnv::new();
    let out = lib_api::run(&env, &["status", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // The total_effort field is omitted (not present) when unset; the
    // serializing contract is `#[serde(skip_serializing_if = "...")]`
    // — when an implementation sets it, it appears.
    let lanes = &v["lanes"];
    // The "lanes" object should not have a total_effort key when the
    // LaneSummary's total_effort is empty. (We can't easily set it
    // from this test, so we just assert the field is absent here.)
    assert!(
        lanes.get("total_effort").is_none()
            || lanes["total_effort"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s != "—"),
        "total_effort should be omitted when unset, or non-placeholder when set; got: {:?}",
        lanes.get("total_effort")
    );
}

/// M102 R3 (F-09): `mp next --lane <name>` resolves the lane by NAME
/// (not positional index into report.lanes[0..3]). An unknown lane
/// must error with a clear message — no panic, no positional fallback.
#[test]
fn lane_resolution_by_name() {
    let env = TestEnv::new();

    // Each named lane resolves cleanly.
    for name in ["execution", "review", "grooming", "backlog"] {
        let out = lib_api::run(&env, &["next", "--lane", name, "--format", "json"]);
        assert!(
            out.status.success(),
            "mp next --lane {name} must succeed; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // An unknown lane name errors with a clear message.
    let out = lib_api::run(&env, &["next", "--lane", "not-a-lane", "--format", "json"]);
    assert!(!out.status.success(), "mp next --lane not-a-lane must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown lane") || stderr.contains("not-a-lane"),
        "stderr should mention unknown lane; got: {stderr}"
    );
}

/// M102 R3 (F-10): the `lane` JSON field reads from `target.name` (the
/// Lane struct's source of truth), not the Debug representation of
/// the CLI arg. This guards against silent drift on enum renames.
#[test]
fn status_lane_field_uses_lane_name() {
    let env = TestEnv::new();

    for name in ["execution", "review", "grooming", "backlog"] {
        let out = lib_api::run(&env, &["next", "--lane", name, "--format", "json"]);
        assert!(out.status.success());
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let lane = v["lane"].as_str().expect("lane field is a string");
        // The wire-format lane name must match the Lane struct's name,
        // not the Debug representation. Both happen to coincide for the
        // current LaneArg (lowercased variant name), but the
        // invariant the test pins is `lane == <name>` (the
        // source-of-truth name), not `lane == <debug output>`.
        assert_eq!(lane, name, "lane field must equal the source-of-truth name");
    }
}

/// M102 R3 (F-11): `mp next --summary` (without --lane) now returns a
/// per-lane summary block (not a silent ignore). The block includes
/// `lanes: {execution, review, grooming, backlog}` counts and a
/// `head` id. Pinned by an end-to-end test on a fresh plan.
#[test]
fn status_summary_legacy() {
    let env = TestEnv::new();
    let out = lib_api::run(&env, &["next", "--summary", "--format", "json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["lanes"].is_object(),
        "--summary must produce a `lanes` object; got {}",
        v
    );
    // The per-lane keys must be the source-of-truth names.
    for lane in ["execution", "review", "grooming", "backlog"] {
        assert!(
            v["lanes"][lane].is_number(),
            "lanes.{lane} must be a count; got {}",
            v["lanes"]
        );
    }
}

/// BF-17 (M131): `mp status` must surface a build_lanes failure rather
/// than silently collapsing it to empty lanes. On a healthy plan the
/// new `lanes_error` field is null; on a plan whose milestones fail to
/// parse it becomes a non-null string while the command still exits
/// successfully with an empty (backcompat) lanes object.
#[test]
fn status_summary_surfaces_build_lanes_error() {
    use std::fs;

    // Healthy plan: lanes_error is null.
    let env = TestEnv::new();
    let out = lib_api::run(&env, &["status", "--format", "json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v.get("lanes_error").map(|e| e.is_null()).unwrap_or(true),
        "healthy plan should have lanes_error=null; got: {v}"
    );

    // Corrupt a milestone file so build_lanes fails to parse it. Create
    // a milestone first (init --profile full has no milestones to
    // corrupt), then overwrite its on-disk JSON with garbage.
    let create_json = r#"{
        "title": "to-corrupt",
        "intent": { "outcome": "x" },
        "problem": { "description": "x" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["other", "tbd"] },
        "acceptance_criteria": [
            { "description": "x works", "verification": "manual: setup" }
        ]
    }"#;
    assert!(
        lib_api::run(
            &env,
            &[
                "milestone",
                "create",
                "--json",
                create_json,
                "--format",
                "json"
            ]
        )
        .status
        .success(),
        "create milestone for corruption failed"
    );

    let plan_dir = env.tmp.path().join("master-plan");
    let milestones_dir = plan_dir.join("milestones");
    let corrupt = fs::read_dir(&milestones_dir)
        .expect("milestones dir")
        .filter_map(Result::ok)
        .next()
        .expect("at least one milestone file");
    fs::write(corrupt.path(), "{ this is not valid json").unwrap();

    let out = lib_api::run(&env, &["status", "--format", "json"]);
    assert!(
        out.status.success(),
        "status must stay green on build_lanes failure; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["lanes_error"].is_string() && !v["lanes_error"].as_str().unwrap().is_empty(),
        "build_lanes failure must surface a non-empty lanes_error; got: {v}"
    );
}
