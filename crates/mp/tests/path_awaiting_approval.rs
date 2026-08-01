//! M157: `mp path --all` surfaces the `awaiting-approval` lane for
//! milestones whose spec is ready but which have not yet been
//! approved. The lane is mutually exclusive with `grooming` (which
//! requires `spec_status != "ready"`) and with `execution` (which
//! requires `lifecycle` in {approved, in-progress}).
//!
//! AC-03 (M157): the lane populates from `mp path --all` and an
//! `mp milestone approve` flips a milestone off the branch onto the
//! execution trunk.

mod common;
use common::TestEnv;

/// Create a fresh milestone via `mp milestone create`. Returns the
/// normalized id (e.g. `01`).
fn create_milestone(env: &TestEnv, title: &str) -> String {
    let create_json = format!(
        r#"{{
        "title": "{title}",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {{ "outcome": "Ship {title}" }},
        "problem": {{ "description": "Need {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{
                "description": "{title} works",
                "verification": "manual: M157 test sanity check"
            }}
        ]
    }}"#
    );
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        &create_json,
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "milestone create failed: stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "create json parse failed: {e}; stdout={}",
            String::from_utf8_lossy(&out.stdout)
        )
    })["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Patch a milestone file's `spec_status` and `lifecycle` to a
/// legacy-triple shape so we can drive the awaiting-approval lane
/// without running the full ceremony (faster + more deterministic).
fn force_legacy_ready(env: &TestEnv, id: &str, lifecycle: &str) {
    let pattern = format!("{}-", id);
    let mut found = None;
    for entry in std::fs::read_dir(env.tmp.path().join("master-plan/milestones")).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if name.starts_with(&pattern) {
            found = Some(entry.path());
            break;
        }
    }
    let p = found.unwrap_or_else(|| panic!("milestone file for {id}"));
    let raw = std::fs::read_to_string(&p).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["spec_status"] = serde_json::json!("ready");
    m["milestone"]["lifecycle"] = serde_json::json!(lifecycle);
    std::fs::write(&p, serde_json::to_string_pretty(&m).unwrap()).unwrap();
}

#[test]
fn awaiting_approval_lane_populates_for_spec_ready_milestones() {
    // The normal ceremony (`set-spec-status ready`) auto-promotes
    // lifecycle to `approved` via G14, so a fresh milestone never
    // sits in the awaiting-approval lane under normal flow. To test
    // the lane we force the legacy-shape triple (spec=ready,
    // lifecycle=groomed) — the exact gap W-LC-TERMINAL warns about.
    let env = TestEnv::new();
    let id = create_milestone(&env, "M157 awaiting");
    force_legacy_ready(&env, &id, "groomed");

    let out = env.run(&["path", "--all", "--format", "json"]);
    assert!(
        out.status.success(),
        "mp path --all failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let lanes = report["lanes"].as_array().unwrap();
    let lane = lanes
        .iter()
        .find(|l| l["name"] == "awaiting-approval")
        .expect("awaiting-approval lane must be present in mp path --all output");
    let items = lane["items"].as_array().unwrap();
    let ids: Vec<&str> = items
        .iter()
        .map(|it| it["milestone"]["id"].as_str().unwrap_or("?"))
        .collect();
    assert!(
        ids.contains(&id.as_str()),
        "new spec-ready milestone {id} (forced lifecycle=groomed) should appear in awaiting-approval lane; got {ids:?}"
    );
    // Summary carries awaiting_approval count.
    assert!(
        report["summary"]["awaiting_approval"].is_number(),
        "summary.awaiting_approval must be present in mp path wire"
    );
}

#[test]
fn awaiting_approval_lane_mutually_exclusive_with_grooming_and_execution() {
    // Force a milestone into lifecycle=groomed + spec_status=ready.
    // It should land in awaiting-approval, NOT in grooming (grooming
    // now requires spec_status != "ready") and NOT in execution
    // (execution requires lifecycle in {approved, in-progress}).
    let env = TestEnv::new();
    let id = create_milestone(&env, "M157 mutual");
    force_legacy_ready(&env, &id, "groomed");

    let out = env.run(&["path", "--all", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let lanes = report["lanes"].as_array().unwrap();
    let awaiting: Vec<&str> = lanes
        .iter()
        .find(|l| l["name"] == "awaiting-approval")
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["milestone"]["id"].as_str().unwrap_or("?"))
        .collect();
    let grooming: Vec<&str> = lanes.iter().find(|l| l["name"] == "grooming").unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["milestone"]["id"].as_str().unwrap_or("?"))
        .collect();
    let execution: Vec<&str> = lanes.iter().find(|l| l["name"] == "execution").unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["milestone"]["id"].as_str().unwrap_or("?"))
        .collect();
    assert!(
        awaiting.contains(&id.as_str()),
        "{id} must be in awaiting-approval; got {awaiting:?}"
    );
    assert!(
        !grooming.contains(&id.as_str()),
        "{id} must NOT also be in grooming; got {grooming:?}"
    );
    assert!(
        !execution.contains(&id.as_str()),
        "{id} must NOT also be in execution; got {execution:?}"
    );
}

#[test]
fn lifecycle_transition_moves_milestone_off_awaiting_approval_onto_execution() {
    // AC-03: lifecycle transition that approve performs (groomed →
    // approved) moves the milestone from awaiting-approval onto the
    // execution trunk. Phase 1 still force-writes the legacy-shape
    // (spec=ready + lifecycle=groomed) because the normal ceremony
    // auto-promotes to approved via G14. Phase 2 uses the real
    // `mp milestone set-lifecycle approved` mutator (not a second
    // JSON rewrite) so the transition path is the product path.
    let env = TestEnv::new();
    let id = create_milestone(&env, "M157 transition");

    // Phase 1: spec=ready + lifecycle=groomed → awaiting-approval.
    force_legacy_ready(&env, &id, "groomed");
    let before = env.run(&["path", "--all", "--format", "json"]);
    let before_report: serde_json::Value = serde_json::from_slice(&before.stdout).unwrap();
    let before_awaiting: Vec<&str> = before_report["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "awaiting-approval")
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["milestone"]["id"].as_str().unwrap_or("?"))
        .collect();
    assert!(
        before_awaiting.contains(&id.as_str()),
        "precondition: {id} must start in awaiting-approval; got {before_awaiting:?}"
    );

    // Phase 2: real approve mutator (lifecycle effect). set-lifecycle is
    // migration-only after M189 and must not be used as a public jump.
    let set = env.run(&["milestone", "approve", &id, "--format", "json"]);
    assert!(
        set.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&set.stderr)
    );

    let after = env.run(&["path", "--all", "--format", "json"]);
    let after_report: serde_json::Value = serde_json::from_slice(&after.stdout).unwrap();
    let after_awaiting: Vec<&str> = after_report["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "awaiting-approval")
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["milestone"]["id"].as_str().unwrap_or("?"))
        .collect();
    let after_execution: Vec<&str> = after_report["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "execution")
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["milestone"]["id"].as_str().unwrap_or("?"))
        .collect();
    assert!(
        !after_awaiting.contains(&id.as_str()),
        "{id} must be OFF awaiting-approval after set-lifecycle approved; got {after_awaiting:?}"
    );
    assert!(
        after_execution.contains(&id.as_str()),
        "{id} must be ON execution trunk after set-lifecycle approved; got {after_execution:?}"
    );
}
