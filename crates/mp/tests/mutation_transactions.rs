//! Crash consistency and retry-idempotency for multi-resource plan mutations.

mod common;

use std::fs;

use common::TestEnv;

fn assert_success(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{context}: status={:?}\nstdout={}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn add_backlog(env: &TestEnv) {
    assert_success(
        &env.run(&[
            "backlog",
            "add",
            "--desc",
            "transaction source",
            "--priority",
            "high",
        ]),
        "seed backlog",
    );
}

fn track_item_count(env: &TestEnv) -> usize {
    env.run_json(&["track", "show", "bugfix"])["items"]
        .as_array()
        .expect("track items")
        .len()
}

#[test]
fn backlog_promotion_rolls_back_each_write_boundary_and_retry_is_idempotent() {
    for fail_after in ["1", "2"] {
        let env = TestEnv::new();
        add_backlog(&env);
        let backlog_before = fs::read(env.tmp.path().join("master-plan/backlog.json")).unwrap();
        let track_before = fs::read(env.tmp.path().join("master-plan/tracks/bugfix.json")).unwrap();

        let failed = env.run_with_env(
            &[("MP_MUTATION_FAIL_AFTER_WRITE", fail_after)],
            &["backlog", "promote", "B-01", "--to-track", "bugfix"],
        );
        assert!(!failed.status.success(), "failpoint {fail_after} must fail");
        assert_eq!(
            fs::read(env.tmp.path().join("master-plan/backlog.json")).unwrap(),
            backlog_before,
            "source changed at failpoint {fail_after}"
        );
        assert_eq!(
            fs::read(env.tmp.path().join("master-plan/tracks/bugfix.json")).unwrap(),
            track_before,
            "target changed at failpoint {fail_after}"
        );

        assert_success(
            &env.run(&["backlog", "promote", "B-01", "--to-track", "bugfix"]),
            "retry promotion",
        );
        assert_eq!(track_item_count(&env), 1);
        assert_success(
            &env.run(&["backlog", "promote", "B-01", "--to-track", "bugfix"]),
            "idempotent retry",
        );
        assert_eq!(track_item_count(&env), 1, "retry duplicated target");
    }
}

#[test]
fn killed_promotion_recovers_on_retry_without_duplicate_target() {
    let env = TestEnv::new();
    add_backlog(&env);
    let crashed = env.run_with_env(
        &[("MP_MUTATION_CRASH_AFTER_WRITE", "1")],
        &["backlog", "promote", "B-01", "--to-track", "bugfix"],
    );
    assert!(!crashed.status.success(), "crash failpoint must terminate");

    let txn_root = env.tmp.path().join("master-plan/.mp-txn");
    let manifest = fs::read_dir(&txn_root)
        .unwrap()
        .next()
        .expect("pending transaction")
        .unwrap()
        .path()
        .join("manifest.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&manifest).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    assert_success(
        &env.run(&["backlog", "promote", "B-01", "--to-track", "bugfix"]),
        "retry performs recovery",
    );
    assert_eq!(track_item_count(&env), 1);
    assert_eq!(
        fs::read_dir(txn_root).unwrap().count(),
        0,
        "recovery marker remained"
    );
}

fn leave_crashed_promotion(env: &TestEnv) -> std::path::PathBuf {
    add_backlog(env);
    let crashed = env.run_with_env(
        &[("MP_MUTATION_CRASH_AFTER_WRITE", "1")],
        &["backlog", "promote", "B-01", "--to-track", "bugfix"],
    );
    assert!(!crashed.status.success());
    fs::read_dir(env.tmp.path().join("master-plan/.mp-txn"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
}

#[test]
fn recovery_manifest_rejects_parent_traversal() {
    let env = TestEnv::new();
    let txn_dir = leave_crashed_promotion(&env);
    let manifest_path = txn_dir.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["baseline"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("../outside.json"));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let recovery = env.run(&["backlog", "add", "--desc", "must not run"]);
    assert!(!recovery.status.success());
    assert!(!env.tmp.path().join("outside.json").exists());
}

#[cfg(unix)]
#[test]
fn recovery_rejects_symlinked_before_image() {
    use std::os::unix::fs::symlink;

    let env = TestEnv::new();
    let txn_dir = leave_crashed_promotion(&env);
    let backup = txn_dir.join("before/backlog.json");
    fs::remove_file(&backup).unwrap();
    let outside = env.tmp.path().join("outside.json");
    fs::write(&outside, b"sentinel").unwrap();
    symlink(&outside, &backup).unwrap();

    let recovery = env.run(&["backlog", "add", "--desc", "must not run"]);
    assert!(!recovery.status.success());
    assert_eq!(fs::read(outside).unwrap(), b"sentinel");
}

fn create_milestone(env: &TestEnv) -> String {
    let value = env.run_json(&[
        "milestone",
        "create",
        "--json",
        r#"{
          "title":"archive transaction",
          "intent":{"outcome":"archive safely"},
          "problem":{"description":"partial archive"},
          "scope":{"in_scope":["archive"],"out_of_scope":["other","later"]},
          "acceptance_criteria":[{"description":"safe","verification":"manual: test"}]
        }"#,
    ]);
    value["milestone"]["id"].as_str().unwrap().to_string()
}

#[test]
fn milestone_archive_failure_restores_file_and_metadata() {
    let env = TestEnv::new();
    let id = create_milestone(&env);
    let active = fs::read_dir(env.tmp.path().join("master-plan/milestones"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("{id}-"))
        })
        .unwrap();
    let active_before = fs::read(&active).unwrap();
    let meta_path = env.tmp.path().join("master-plan/archive/meta.json");
    let meta_before = fs::read(&meta_path).unwrap();

    for fail_after in ["1", "2"] {
        let failed = env.run_with_env(
            &[("MP_MUTATION_FAIL_AFTER_WRITE", fail_after)],
            &["milestone", "archive", &id],
        );
        assert!(!failed.status.success(), "failpoint {fail_after}");
        assert_eq!(fs::read(&active).unwrap(), active_before);
        assert_eq!(fs::read(&meta_path).unwrap(), meta_before);
        assert!(
            fs::read_dir(env.tmp.path().join("master-plan/archive/milestones"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}

fn satisfy_brief(env: &TestEnv) {
    let todo = env.run_json(&["brief", "todo"]);
    for topic in todo["topics"].as_array().unwrap() {
        let id = topic["id"].as_str().unwrap();
        assert_success(&env.run(&["brief", "skip", id]), "skip brief topic");
    }
}

#[test]
fn brief_done_is_atomic_at_both_writes() {
    for fail_after in ["1", "2"] {
        let env = TestEnv::new();
        satisfy_brief(&env);
        let brief_path = env.tmp.path().join("master-plan/brief.json");
        let plan_path = env.tmp.path().join("master-plan/plan.json");
        let before = (
            fs::read(&brief_path).unwrap(),
            fs::read(&plan_path).unwrap(),
        );
        let failed = env.run_with_env(
            &[("MP_MUTATION_FAIL_AFTER_WRITE", fail_after)],
            &["brief", "done"],
        );
        assert!(!failed.status.success());
        assert_eq!(fs::read(&brief_path).unwrap(), before.0);
        assert_eq!(fs::read(&plan_path).unwrap(), before.1);
        assert_success(&env.run(&["brief", "done"]), "brief done retry");
    }
}

#[test]
fn session_start_failure_leaves_no_partial_session_or_milestone() {
    for fail_after in ["1", "2"] {
        let env = TestEnv::new();
        let milestones_before = fs::read_dir(env.tmp.path().join("master-plan/milestones"))
            .unwrap()
            .count();
        let failed = env.run_with_env(
            &[("MP_MUTATION_FAIL_AFTER_WRITE", fail_after)],
            &["session", "start", "--branch", &format!("txn-{fail_after}")],
        );
        assert!(!failed.status.success());
        assert!(!env
            .tmp
            .path()
            .join(format!("master-plan/sessions/txn-{fail_after}"))
            .exists());
        assert_eq!(
            fs::read_dir(env.tmp.path().join("master-plan/milestones"))
                .unwrap()
                .count(),
            milestones_before
        );
        assert_success(
            &env.run(&["session", "start", "--branch", &format!("txn-{fail_after}")]),
            "session start retry",
        );
    }
}

#[test]
fn activity_append_failure_does_not_report_primary_session_rollback() {
    let env = TestEnv::new();
    let output = env.run_with_env(
        &[("MP_MUTATION_FAIL_AFTER_WRITE", "3")],
        &["session", "start", "--branch", "activity-best-effort"],
    );
    assert_success(&output, "activity failure must be best effort");
    assert!(env
        .tmp
        .path()
        .join("master-plan/sessions/activity-best-effort/session.json")
        .is_file());
}

#[test]
fn successful_promotions_are_idempotent_when_response_is_retried() {
    let idea_env = TestEnv::new();
    assert_success(
        &idea_env.run(&["idea", "create", "--title", "retry idea"]),
        "create idea",
    );
    assert_success(
        &idea_env.run(&["idea", "promote", "ID-01", "--to-backlog"]),
        "promote idea",
    );
    assert_success(
        &idea_env.run(&["idea", "promote", "ID-01", "--to-backlog"]),
        "retry idea promotion",
    );
    assert_eq!(
        idea_env.run_json(&["backlog", "list"])["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let brief_env = TestEnv::new();
    let topic = brief_env.run_json(&["brief", "todo"])["topics"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_success(
        &brief_env.run(&["brief", "edit", &topic, "--body", "retry body"]),
        "fill brief",
    );
    assert_success(
        &brief_env.run(&["brief", "promote", &topic, "--to-idea"]),
        "promote brief",
    );
    assert_success(
        &brief_env.run(&["brief", "promote", &topic, "--to-idea"]),
        "retry brief promotion",
    );
    assert_eq!(
        brief_env.run_json(&["idea", "list"])["ideas"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let track_env = TestEnv::new();
    let track_id = track_env.run_json(&[
        "track",
        "add",
        "bugfix",
        "--title",
        "retry track",
        "--problem",
        "retry",
    ])["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_success(
        &track_env.run(&["track", "promote", "bugfix", &track_id, "--to-milestone"]),
        "promote track",
    );
    assert_success(
        &track_env.run(&["track", "promote", "bugfix", &track_id, "--to-milestone"]),
        "retry track promotion",
    );
    assert_eq!(
        fs::read_dir(track_env.tmp.path().join("master-plan/milestones"))
            .unwrap()
            .count(),
        1
    );

    let session_env = TestEnv::new();
    let started = session_env.run_json(&[
        "session",
        "start",
        "--branch",
        "retry-session",
        "--title",
        "retry session",
    ]);
    let session_id = started["session_id"].as_str().unwrap();
    assert_success(
        &session_env.run(&["session", "promote", session_id]),
        "promote session",
    );
    assert_success(
        &session_env.run(&["session", "promote", session_id]),
        "retry session promotion",
    );
    assert_eq!(
        fs::read_dir(session_env.tmp.path().join("master-plan/milestones"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn commit_crash_after_final_write_preserves_sealed_mutation() {
    // F-04: crash after all authoritative writes succeed but before txn cleanup
    // must not undo the durable mutation on the next writer.
    let env = TestEnv::new();
    add_backlog(&env);
    let crashed = env.run_with_env(
        &[("MP_MUTATION_CRASH_AFTER_WRITE", "2")],
        &["backlog", "promote", "B-01", "--to-track", "bugfix"],
    );
    assert!(!crashed.status.success(), "post-seal crash must terminate");

    let backlog = serde_json::from_slice::<serde_json::Value>(
        &fs::read(env.tmp.path().join("master-plan/backlog.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(backlog["items"][0]["status"], "resolved");
    assert_eq!(track_item_count(&env), 1);

    let txn_root = env.tmp.path().join("master-plan/.mp-txn");
    let txn_dir = fs::read_dir(&txn_root)
        .unwrap()
        .next()
        .expect("pending sealed transaction")
        .unwrap()
        .path();
    assert!(
        txn_dir.join("COMMITTED").is_file(),
        "durable commit marker missing after final-write crash"
    );

    assert_success(
        &env.run(&["backlog", "add", "--desc", "recover"]),
        "unrelated writer triggers recovery",
    );
    let backlog = serde_json::from_slice::<serde_json::Value>(
        &fs::read(env.tmp.path().join("master-plan/backlog.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        backlog["items"][0]["status"], "resolved",
        "recovery must not undo a sealed promotion"
    );
    assert_eq!(track_item_count(&env), 1, "recovery must keep track target");
    assert_eq!(
        fs::read_dir(&txn_root).map(|d| d.count()).unwrap_or(0),
        0,
        "sealed txn must be cleaned up"
    );
}

#[test]
fn plan_relocate_failpoint_rolls_back_path_and_config() {
    // F-05/F-08: FAIL_AFTER_WRITE at rename (1) or config persist (2) must keep
    // plan path and workflow.plan.location converged; retry must succeed.
    for fail_after in ["1", "2"] {
        let env = TestEnv::new();
        let project = env.tmp.path();
        let failed = env.run_with_env(
            &[("MP_MUTATION_FAIL_AFTER_WRITE", fail_after)],
            &["plan", "relocate", "master-plan", "relocated-plan"],
        );
        assert!(
            !failed.status.success(),
            "relocate failpoint={fail_after} must fail"
        );
        assert!(
            project.join("master-plan").is_dir(),
            "old plan path must be restored (fail_after={fail_after})"
        );
        assert!(
            !project.join("relocated-plan").exists(),
            "partial relocate target must not remain (fail_after={fail_after})"
        );
        let cfg: serde_json::Value =
            serde_json::from_slice(&fs::read(project.join("master-plan/config.json")).unwrap())
                .unwrap();
        let location = cfg["workflow"]["plan"]["location"]
            .as_str()
            .unwrap_or("master-plan");
        assert_eq!(
            location, "master-plan",
            "config location must stay aligned with plan path (fail_after={fail_after})"
        );

        assert_success(
            &env.run(&["plan", "relocate", "master-plan", "relocated-plan"]),
            &format!("relocate retry after fail_after={fail_after}"),
        );
        assert!(
            project.join("relocated-plan").is_dir(),
            "retry must land at new path (fail_after={fail_after})"
        );
        assert!(
            !project.join("master-plan").exists(),
            "retry must leave old path (fail_after={fail_after})"
        );
        let cfg_after: serde_json::Value =
            serde_json::from_slice(&fs::read(project.join("relocated-plan/config.json")).unwrap())
                .unwrap();
        assert_eq!(
            cfg_after["workflow"]["plan"]["location"].as_str(),
            Some("relocated-plan"),
            "successful relocate must converge location (after fail_after={fail_after})"
        );
    }
}

#[test]
fn track_purge_removes_archive_meta_entry() {
    // F-06: track purge must drop archive/meta.json entry with the file.
    let env = TestEnv::new();
    let id = create_milestone(&env);
    assert_success(&env.run(&["milestone", "archive", &id]), "archive");
    let meta_before: serde_json::Value = serde_json::from_slice(
        &fs::read(env.tmp.path().join("master-plan/archive/meta.json")).unwrap(),
    )
    .unwrap();
    assert!(
        meta_before["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["entity_id"] == id),
        "meta must list archived milestone before purge"
    );

    assert_success(
        &env.run(&["track", "purge", "archived", "milestone", &id, "--confirm"]),
        "track purge",
    );
    assert!(
        fs::read_dir(env.tmp.path().join("master-plan/archive/milestones"))
            .unwrap()
            .next()
            .is_none(),
        "archive file must be gone"
    );
    let meta_after: serde_json::Value = serde_json::from_slice(
        &fs::read(env.tmp.path().join("master-plan/archive/meta.json")).unwrap(),
    )
    .unwrap();
    assert!(
        meta_after["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["entity_id"] != id),
        "archive meta must not retain purged milestone"
    );
}

#[test]
fn track_item_restore_removes_archive_meta_entry() {
    // F-07: restoring a track-item must clear its archive meta entry.
    let env = TestEnv::new();
    let item_id = env.run_json(&[
        "track",
        "add",
        "bugfix",
        "--title",
        "restore meta",
        "--problem",
        "stale meta",
    ])["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_success(
        &env.run(&["track", "archive", "track-item", "bugfix", &item_id]),
        "archive track-item",
    );
    let meta_archived: serde_json::Value = serde_json::from_slice(
        &fs::read(env.tmp.path().join("master-plan/archive/meta.json")).unwrap(),
    )
    .unwrap();
    assert!(meta_archived["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["entity_id"] == item_id));

    assert_success(
        &env.run(&[
            "track",
            "restore",
            "archived",
            "track-item",
            &item_id,
            "--kind",
            "bugfix",
        ]),
        "restore track-item",
    );
    let track = env.run_json(&["track", "show", "bugfix"]);
    let item = track["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == item_id)
        .expect("item");
    assert_eq!(item["status"], "pending");
    let meta_after: serde_json::Value = serde_json::from_slice(
        &fs::read(env.tmp.path().join("master-plan/archive/meta.json")).unwrap(),
    )
    .unwrap();
    assert!(
        meta_after["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["entity_id"] != item_id),
        "archive meta must drop restored track-item"
    );
}
