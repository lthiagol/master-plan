//! M105 S4 (B-41) — `mp edit strip-dropped-keys` integration test.
//!
//! Pinned behavior:
//!   1. The utility removes every key in `milestone::DROPPED_CEREMONY_KEYS`
//!      from every milestone file in scope.
//!   2. It is idempotent: re-running on a clean plan returns
//!      `files_modified == 0` and does not rewrite any file.
//!   3. After running, `mp validate --summary` stays 0 errors / 0 warnings.
//!   4. An arbitrary milestone JSON whose keys are stripped stays valid
//!      JSON and keeps all load-bearing fields intact.

use std::path::Path;

use crate::common::TestEnv;

/// Recipe for a single milestone file with some dropped keys injected.
/// The load-bearing fields (title, depends_on, scope, …) are intentionally
/// minimal but valid.
fn milestone_with_dropped_keys(id: &str) -> String {
    format!(
        r#"{{
  "milestone": {{
    "id": "{id}",
    "title": "Sample {id}",
    "spec_status": "ready",
    "execution_status": "planned",
    "depends_on": [],
    "priority": "low",
    "risk": "low",
    "effort": "S"
  }},
  "intent": {{ "outcome": "test" }},
  "problem": {{ "description": "smoke" }},
  "scope": {{
    "in_scope": ["x"],
    "out_of_scope": ["y"]
  }},
  "acceptance_criteria": [],
  "design_decisions": [],
  "steps": [],
  "work_packages": [],
  "follow_ups": [],
  "behavior": {{ "scenarios": [] }},
  "context": {{}},
  "requirements": [],
  "success_criteria": [],
  "assumptions": [],
  "interface": {{}},
  "risks": [],
  "technical_context": {{}}
}}"#
    )
}

/// Count how many dropped keys a milestone JSON still carries.
/// M114 review F-3: source the list from production (`DROPPED_CEREMONY_KEYS`)
/// instead of a hand-maintained local copy, so the test catches drift if
/// production adds or removes a dropped key.
fn dropped_key_count(text: &str) -> usize {
    mp::milestone::DROPPED_CEREMONY_KEYS
        .iter()
        .filter(|k| text.contains(&format!("\"{k}\":")))
        .count()
}

fn write_milestone(dir: &Path, id: &str) {
    let body = milestone_with_dropped_keys(id);
    std::fs::write(dir.join(format!("{id}.json")), body).expect("write milestone");
}

#[test]
fn strip_removes_all_dropped_keys() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone(&milestones_dir, "alpha");
    write_milestone(&milestones_dir, "beta");

    let out = env.run(&["edit", "strip-dropped-keys", "--format", "json"]);
    assert!(
        out.status.success(),
        "mp edit strip-dropped-keys failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("strip output is valid JSON");
    assert_eq!(json["files_scanned"], serde_json::Value::from(2));
    assert_eq!(json["files_modified"], serde_json::Value::from(2));
    assert_eq!(json["total_keys_removed"], serde_json::Value::from(18)); // 2 files × 9 keys
    assert_eq!(json["idempotent_run"], serde_json::Value::Bool(false));

    // Each file should have 0 dropped keys left.
    for id in ["alpha", "beta"] {
        let body = std::fs::read_to_string(milestones_dir.join(format!("{id}.json")))
            .expect("read post-strip");
        assert_eq!(
            dropped_key_count(&body),
            0,
            "file {id} still has dropped keys after strip: {body}"
        );
    }
}

#[test]
fn strip_is_idempotent_on_a_clean_plan() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone(&milestones_dir, "alpha");

    // First run cleans.
    let out1 = env.run(&["edit", "strip-dropped-keys", "--format", "json"]);
    assert!(out1.status.success());
    let json1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("strip output is valid JSON");
    assert_eq!(json1["files_modified"], serde_json::Value::from(1));
    assert_eq!(json1["idempotent_run"], serde_json::Value::Bool(false));

    // Capture the file mtime + content snapshot to assert the second run
    // doesn't rewrite the file.
    let path = milestones_dir.join("alpha.json");
    let first_body = std::fs::read_to_string(&path).expect("read after first run");
    // First mtime — captured before first run only the body, but a second
    // run should leave the body byte-for-byte identical (idempotent re-run
    // contract: no file rewrite when no key matches).
    let out2 = env.run(&["edit", "strip-dropped-keys", "--format", "json"]);
    assert!(out2.status.success());
    let json2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("strip output is valid JSON");
    assert_eq!(
        json2["files_modified"],
        serde_json::Value::from(0),
        "second strip run should be a no-op"
    );
    assert_eq!(
        json2["idempotent_run"],
        serde_json::Value::Bool(true),
        "second strip run must report idempotent_run=true"
    );

    let second_body = std::fs::read_to_string(&path).expect("read after second run");
    assert_eq!(
        first_body, second_body,
        "idempotent re-run must leave the file byte-for-byte identical"
    );
}

#[test]
fn strip_preserves_load_bearing_fields() {
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone(&milestones_dir, "alpha");

    let out = env.run(&["edit", "strip-dropped-keys", "--format", "json"]);
    assert!(out.status.success());

    let body = std::fs::read_to_string(milestones_dir.join("alpha.json")).expect("read");
    let json: serde_json::Value =
        serde_json::from_str(&body).expect("post-strip file is valid JSON");
    let m = &json["milestone"];
    assert_eq!(m["id"], serde_json::Value::from("alpha"));
    assert_eq!(m["title"], serde_json::Value::from("Sample alpha"));
    assert_eq!(m["spec_status"], serde_json::Value::from("ready"));
    let intent = json["intent"]["outcome"].as_str().expect("intent.outcome");
    assert_eq!(intent, "test");
    let in_scope = json["scope"]["in_scope"]
        .as_array()
        .expect("scope.in_scope is array");
    assert_eq!(in_scope.len(), 1);
    assert_eq!(
        in_scope[0],
        serde_json::Value::from("x"),
        "load-bearing field must survive the strip"
    );
}

#[test]
fn strip_leaves_an_already_clean_file_untouched() {
    // If a milestone file has no dropped keys to begin with, the strip
    // utility must leave its mtime unchanged (no rewrite) and not list it
    // in `removed_by_file`.
    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    let clean_body = r#"{
  "milestone": {
    "id": "clean",
    "title": "Clean",
    "spec_status": "ready",
    "execution_status": "planned"
  },
  "intent": {},
  "problem": {},
  "scope": {}
}
"#;
    let path = milestones_dir.join("clean.json");
    std::fs::write(&path, clean_body).expect("write");

    let mtime_before = std::fs::metadata(&path)
        .expect("mtime")
        .modified()
        .expect("modified");

    let out = env.run(&["edit", "strip-dropped-keys", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("strip output is valid JSON");
    assert_eq!(json["files_scanned"], serde_json::Value::from(1));
    assert_eq!(json["files_modified"], serde_json::Value::from(0));
    assert_eq!(json["idempotent_run"], serde_json::Value::Bool(true));
    assert_eq!(
        json["removed_by_file"]["clean.json"],
        serde_json::Value::Null,
        "no entry must appear for files that needed no surgery"
    );

    let mtime_after = std::fs::metadata(&path)
        .expect("mtime")
        .modified()
        .expect("modified");
    assert_eq!(
        mtime_before, mtime_after,
        "a clean file must not be rewritten (mtime unchanged)"
    );
}

// M114 review F-2: `post_strip_validate_summary_stays_green` was removed.
// The test's name promised a validate-gate assertion, but its body only
// checked the post-strip file was valid JSON with the right id — strictly
// weaker than `strip_preserves_load_bearing_fields`, which already pins
// JSON validity AND load-bearing fields. AC-07 done_when is explicit that
// the validate gate is enforced end-to-end by `m105-s4-verify.sh` (which
// exists and runs on the dogfood plan), not by a rust-level assertion
// against a minimal fixture. Keeping the misnamed test would mislead
// readers into thinking the gate is rust-covered when it is not.

#[test]
fn strip_acquires_the_plan_write_lock() {
    // M114 review F-1: `mp edit strip-dropped-keys` rewrites milestone
    // files, so it must serialize through the M113 advisory write lock —
    // otherwise a concurrent locked writer (`mp milestone step add`) could
    // interleave with an unlocked strip and clobber a write. This test
    // pins that `cmd_edit` takes the lock: hold the lock in-process, then
    // spawn a strip invocation with a 2s timeout and assert it surfaces
    // the lock/timeout error rather than silently running. Mirrors the
    // `held_lock_surfaces_clear_error` pattern in plan_io_concurrent_writes.
    use mp::plan_io::PlanWriteLock;

    let env = TestEnv::new();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    write_milestone(&milestones_dir, "alpha");

    // The lock lives at <plan_dir>/.mp-write.lock; plan_dir is cwd's
    // master-plan/ (mp auto-discovers it under cwd).
    let lock_path = env.tmp.path().join("master-plan/.mp-write.lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir lock dir");
    }
    let holder = PlanWriteLock::acquire_blocking(&lock_path).expect("holder must grab the lock");

    let out = std::process::Command::new(crate::common::mp_bin())
        .current_dir(env.tmp.path())
        .env("MP_HOME", crate::common::repo_root())
        .env("MP_LOCK_TIMEOUT_SECS", "2")
        .args(["edit", "strip-dropped-keys", "--format", "json"])
        .output()
        .expect("spawn mp edit strip-dropped-keys");
    drop(holder);

    assert!(
        !out.status.success(),
        "strip under a held lock must fail clearly; got status {:?}, stdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("lock") || stderr.contains("timeout"),
        "strip must surface the lock/timeout error; got stderr: {stderr}"
    );
}
