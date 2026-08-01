//! M113 S2: `mp milestone set-status/approve/complete --dry-run` prints the
//! change set and exits 0 without writing anything. Agents use this to
//! preview a milestone lifecycle transition without reverting via
//! `git checkout` (the 2026-07-04 dogfood-log gap).

mod common;

use crate::common::TestEnv;

fn snapshot_milestones(env: &TestEnv) -> std::collections::BTreeMap<String, String> {
    // Snapshot every milestone file's contents by hash-equivalent diff:
    // we just read each on-disk file's bytes (filter for non-`.gitignore`
    // noise). The test asserts no file content changed between snapshots.
    let mut snapshot = std::collections::BTreeMap::new();
    let milestone_dir = env.tmp.path().join("master-plan/milestones");
    if let Ok(entries) = std::fs::read_dir(&milestone_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let content = std::fs::read_to_string(entry.path())
                .unwrap_or_else(|e| format!("<read err: {e}>"));
            snapshot.insert(name, content);
        }
    }
    snapshot
}

#[test]
fn set_status_dry_run_does_not_mutate_any_file() {
    let env = TestEnv::new();
    // The 'init' command is part of TestEnv::new() and produces a milestone.
    // We use the first milestone (01) for the dry-run.
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"dry-run-set","intent":{"outcome":"x"},"problem":{"description":"x"},"scope":{"in_scope":["a"],"out_of_scope":["b","c"]}}"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let before = snapshot_milestones(&env);

    let out = env.run(&["milestone", "set-status", &id, "in-progress", "--dry-run"]);
    assert!(
        out.status.success(),
        "dry-run must exit 0; got status: {:?}, stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["command"], "milestone set-status");
    assert!(v["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s.as_str().unwrap_or("").contains(&id)));
    // M189 F-03: draft→in-progress is gated; dry-run must surface gates
    // rather than claiming an execution_status flip the live path rejects.
    let gates = v["gates"].as_array().expect("gates key present");
    assert!(
        !gates.is_empty(),
        "set-status in-progress on draft must report start gates; got {v}"
    );

    // No file changed.
    let after = snapshot_milestones(&env);
    assert_eq!(before, after, "set-status --dry-run must not mutate files");
}

#[test]
fn approve_dry_run_does_not_mutate_any_file() {
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"dry-run-approve","intent":{"outcome":"x"},"problem":{"description":"x"},"scope":{"in_scope":["a"],"out_of_scope":["b","c"]}}"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Drive milestone through to `review` so the approve gate wouldn't
    // block in the real path; dry-run should preview regardless.
    env.run(&["milestone", "set-spec-status", &id, "review"]);
    env.run(&["milestone", "set-spec-status", &id, "ready"]);

    let before = snapshot_milestones(&env);
    let out = env.run(&["milestone", "approve", &id, "--dry-run"]);
    assert!(
        out.status.success(),
        "approve --dry-run must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["command"], "milestone approve");

    let after = snapshot_milestones(&env);
    assert_eq!(before, after, "approve --dry-run must not mutate files");
}

#[test]
fn complete_dry_run_does_not_mutate_any_file() {
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"dry-run-complete","intent":{"outcome":"x"},"problem":{"description":"x"},"scope":{"in_scope":["a"],"out_of_scope":["b","c"]}}"#,
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

    let before = snapshot_milestones(&env);
    let out = env.run(&[
        "milestone",
        "complete",
        &id,
        "--evidence",
        "manual: dry-run test",
        "--dry-run",
    ]);
    assert!(
        out.status.success(),
        "complete --dry-run must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["command"], "milestone complete");

    // `verifications` is a list (possibly empty). shape sanity.
    assert!(v["verifications"].is_array());

    let after = snapshot_milestones(&env);
    assert_eq!(before, after, "complete --dry-run must not mutate files");
}

// M113 review F-3: the dry-run preview must mirror the real command's
// gate surface. An `approve` on a freshly-created milestone (spec_status
// draft, ACs not satisfied) is one the real command would reject; the
// preview must surface the same gates in `gates` rather than claiming
// success. Pre-fix the preview reported `{dry_run: true, gates: <absent>}`
// for an input the real invocation would fail on.
#[test]
fn approve_dry_run_surfaces_blocking_gates() {
    let env = TestEnv::new();
    // Freshly created: spec_status=draft, scope/ACs incomplete relative to
    // the ready gate. The real `mp milestone approve` would fail validation.
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"dry-run-blocked","intent":{"outcome":"x"},"problem":{"description":"x"},"scope":{"in_scope":["a"],"out_of_scope":["b","c"]}}"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let before = snapshot_milestones(&env);
    let out = env.run(&["milestone", "approve", &id, "--dry-run"]);
    assert!(
        out.status.success(),
        "dry-run must exit 0 even when gates would block; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true, "must still be a dry-run envelope");
    let gates = v["gates"]
        .as_array()
        .expect("gates array must be present on approve preview");
    assert!(
        !gates.is_empty(),
        "approve on an un-groomed milestone must surface ready-gate failures; got: {gates:?}"
    );
    // Each gate entry carries the issue code so the caller can react.
    assert!(gates.iter().all(|g| g["code"].is_string()));

    // Still read-only: gates are reported, not enforced.
    let after = snapshot_milestones(&env);
    assert_eq!(
        before, after,
        "dry-run must not mutate files even with gates"
    );
}

/// M121 F-09: end-to-end coverage of the verify-ac -> approve gate
/// integration. A milestone with a bogus AC verification
/// (`cargo test -p nonexistent_crate --test nonexistent_test`) must
/// fail approve --dry-run with the M121 validation issue present in
/// the gates report. Without this test, future refactors of the
/// approve gate could silently break the verify-ac integration.
#[test]
fn approve_dry_run_surfaces_verify_ac_failure_on_bogus_verification() {
    let env = TestEnv::new();

    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"Bogus Verify-Ac Approve","intent":{"outcome":"test"},"scope":{"in_scope":["a"],"out_of_scope":["b","c"]}}"#,
        "--format",
        "json",
    ]);
    assert!(
        create.status.success(),
        "create: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let ac_add = env.run(&[
        "milestone",
        "ac",
        "add",
        &id,
        "--description",
        "bogus verification",
        "--verification",
        "cargo test -p nonexistent_crate --test nonexistent_test",
        "--format",
        "json",
    ]);
    assert!(
        ac_add.status.success(),
        "ac add: {}",
        String::from_utf8_lossy(&ac_add.stderr)
    );

    // Drive the milestone through to ready so the approve gate is the
    // only thing blocking (i.e., the verify-ac is the lone failure).
    env.run(&["milestone", "set-spec-status", &id, "review"]);
    env.run(&["milestone", "set-spec-status", &id, "ready"]);

    let out = env.run(&["milestone", "approve", &id, "--dry-run"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("approve --dry-run JSON: stdout={stdout} stderr={stderr}"));

    let gates = v["gates"]
        .as_array()
        .expect("gates array must be present on approve preview");
    let m121_gates: Vec<&serde_json::Value> = gates
        .iter()
        .filter(|g| g["code"].as_str() == Some("M121"))
        .collect();
    assert!(
        !m121_gates.is_empty(),
        "approve --dry-run must surface the M121 gate failure for the bogus verification; got gates: {gates:?}"
    );

    // At least one M121 gate must reference the bogus crate / test name.
    let combined: String = m121_gates
        .iter()
        .filter_map(|g| g["message"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        combined.contains("nonexistent_crate") || combined.contains("nonexistent_test"),
        "M121 gate message must reference the bogus symbol; got: {combined}"
    );
}
