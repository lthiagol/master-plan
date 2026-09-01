//! M202 AC-18: bulk set-stage skips cancelled milestones (no-op with
//! reason `cancelled`) and supports --dry-run with the same per-id
//! before/after reporting every other bulk mutator uses.

use crate::common::lib_api;
use crate::common::TestEnv;
use serde_json::json;

fn create(env: &TestEnv, title: &str) -> String {
    let payload = json!({
        "title": title,
        "intent": {"outcome": "bulk-set-stage fixture"},
        "problem": {"description": "set-stage coverage"},
        "scope": {"in_scope": ["x"], "out_of_scope": ["y", "z"]},
        "acceptance_criteria": [{"description": "x", "verification": "manual: ok"}]
    });
    let out = lib_api::run(
        env,
        &[
            "milestone",
            "create",
            "--json",
            &payload.to_string(),
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn bulk_set_stage_skips_cancelled() {
    let env = TestEnv::new();
    let active = create(&env, "active-target");
    let cancelled = create(&env, "cancelled-target");
    // Cancel the second milestone.
    let approve = lib_api::run(
        &env,
        &["milestone", "approve", &cancelled],
    );
    assert!(approve.status.success());
    let start = lib_api::run(&env, &["milestone", "set-status", &cancelled, "in-progress"]);
    assert!(start.status.success());
    let cancel = lib_api::run(&env, &["milestone", "set-status", &cancelled, "cancelled"]);
    assert!(
        cancel.status.success(),
        "cancel failed: {}",
        String::from_utf8_lossy(&cancel.stderr)
    );

    // Bulk set-stage on both. Active gets done; cancelled is skipped.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-stage",
            "--ids",
            &format!("{active},{cancelled}"),
            "--stage",
            "external-review",
            "--status",
            "done",
        ],
    );
    assert!(
        out.status.success(),
        "bulk set-stage failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["succeeded"], 2, "cancelled target is skipped, not failed; got {payload}");
    assert_eq!(payload["failed"], 0);
    // Cancelled target must appear with reason cancelled.
    let cancelled_row = payload["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(cancelled))
        .expect("cancelled row missing");
    assert_eq!(cancelled_row["ok"], true, "cancelled must be skipped, not failed");
    assert_eq!(
        cancelled_row["reason"], "cancelled",
        "cancelled row must carry reason=cancelled; got: {cancelled_row}"
    );
    // Active target must NOT carry a reason marker.
    let active_row = payload["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(active))
        .expect("active row missing");
    assert!(
        active_row.get("reason").is_none() || active_row["reason"].is_null(),
        "active row must not carry a reason marker; got: {active_row}"
    );
    // Active target must have flow_stages.external-review=done.
    let show_active = lib_api::run(&env, &["show", "milestone", &active, "--format", "raw"]);
    let doc: serde_json::Value = serde_json::from_slice(&show_active.stdout).unwrap();
    let flow = doc["milestone"]["flow_stages"]
        .as_object()
        .expect("flow_stages present after bulk set-stage");
    assert_eq!(flow["external-review"]["status"], "done");
    // Cancelled target must NOT have been mutated.
    let show_cancelled = lib_api::run(
        &env,
        &["show", "milestone", &cancelled, "--format", "raw"],
    );
    let cdoc: serde_json::Value = serde_json::from_slice(&show_cancelled.stdout).unwrap();
    let cflow = cdoc["milestone"]["flow_stages"].as_object();
    assert!(
        cflow.is_none()
            || !cflow.unwrap().contains_key("external-review")
            || cflow.unwrap()["external-review"]["status"] != "done",
        "cancelled milestone must not have been mutated by bulk set-stage; got: {cflow:?}"
    );
}

#[test]
fn bulk_set_stage_dry_run() {
    let env = TestEnv::new();
    let id = create(&env, "dry-run-target");

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-stage",
            "--ids",
            &id,
            "--stage",
            "document",
            "--status",
            "done",
            "--dry-run",
        ],
    );
    assert!(
        out.status.success(),
        "dry-run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["succeeded"], 1);
    assert_eq!(payload["failed"], 0);
    // The before/after fields must be present (dry-run reports the
    // would-be mutation).
    let row = &payload["results"][0];
    assert_eq!(row["ok"], true);
    assert_eq!(row["after"]["stage"], "document");
    assert_eq!(row["after"]["status"], "done");
    // Live path must not have been touched.
    let show = lib_api::run(&env, &["show", "milestone", &id, "--format", "raw"]);
    let doc: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let flow = doc["milestone"]["flow_stages"].as_object();
    assert!(
        flow.is_none()
            || !flow.unwrap().contains_key("document")
            || flow.unwrap()["document"]["status"] != "done",
        "dry-run must not mutate on disk; got: {flow:?}"
    );
}

#[test]
fn bulk_set_stage_validates_stage_and_status_eagerly() {
    let env = TestEnv::new();
    let id = create(&env, "eager-validation");
    // Bad stage slug must fail the whole batch with a precise error
    // before any milestone is touched.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-stage",
            "--ids",
            &id,
            "--stage",
            "not-a-stage",
            "--status",
            "done",
        ],
    );
    assert!(!out.status.success(), "bad stage must exit non-zero");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("invalid stage") && combined.contains("not-a-stage"),
        "{combined}"
    );
    // Bad status must also fail loudly.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-stage",
            "--ids",
            &id,
            "--stage",
            "draft",
            "--status",
            "bogus",
        ],
    );
    assert!(!out.status.success(), "bad status must exit non-zero");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("invalid status") && combined.contains("bogus"),
        "{combined}"
    );
}
