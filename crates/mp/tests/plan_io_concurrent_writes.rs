//! M113 S1: plan-IO advisory lock integration. Fires N (>=5) parallel
//! `mp milestone step add` invocations against one freshly-created
//! milestone, then asserts all N steps landed. Concurrent writes used to
//! lose data (last writer wins). The lock serializes them.
//!
//! Reads via `mp list steps --select step.id`. The 2026-07-04 dogfood
//! log entry `Parallel mp milestone wp|step invocations race and drop
//! writes` is the failure mode this guards.

mod common;

use crate::common::TestEnv;

fn run_parallel(env: &TestEnv, commands: Vec<Vec<String>>) {
    let cwd = env.tmp.path().to_path_buf();
    let handles: Vec<_> = commands
        .into_iter()
        .map(|args| {
            let cwd = cwd.clone();
            std::thread::spawn(move || {
                std::process::Command::new(common::mp_bin())
                    .current_dir(cwd)
                    .env("MP_HOME", common::repo_root())
                    .args(args)
                    .output()
                    .expect("spawn mp")
            })
        })
        .collect();
    for output in handles.into_iter().map(|handle| handle.join().unwrap()) {
        assert!(
            output.status.success(),
            "parallel command failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn create_and_approve(env: &TestEnv, title: &str) -> String {
    // AC verification placeholder; must be `manual:` (or a real
    // `cargo …` subcommand) to pass the M121 verify-ac Approve gate
    // (F-08: gate fails on UNRESOLVABLE/empty/unknown). The AC content
    // is unrelated to the concurrent step-add behavior the test
    // exercises; this just needs the milestone in `approved` state.
    let create_json = format!(
        r#"{{
            "title": "{title}",
            "intent": {{ "outcome": "Ship {title}" }},
            "problem": {{ "description": "Need {title}." }},
            "scope": {{
                "in_scope": ["{title}"],
                "out_of_scope": ["Other", "TBD"]
            }},
            "acceptance_criteria": [
                {{ "description": "{title} works", "verification": "manual: race-test sanity check" }}
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
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "wp",
            "add",
            &id,
            "--name",
            "Work",
            "--id",
            "WP1",
            "--goal",
            "Do it",
            "--format",
            "json",
        ])
        .status
        .success());
    id
}

#[test]
fn parallel_step_adds_all_land() {
    let env = TestEnv::new();
    let id = create_and_approve(&env, "m113-race");

    // Plan-level concurrent invocations exercise the advisory file
    // lock around `with_milestone_mut`. The dogfood entry was about
    // 2-3 concurrent CLI commands losing data; N=6 widens that
    // window deterministically.
    const N: usize = 6;

    // Each `Command` is a separate `mp` process — cross-process
    // `flock(2)` is the documented serialization mechanism. We
    // thread through std::process::Command directly so failures are
    // surfaced per-invocation.
    use std::process::Command;
    use std::thread;

    let mut handles = Vec::new();
    let mut expected_ids: Vec<String> = Vec::new();
    for n in 0..N {
        let id = id.clone();
        let step_id = format!("S-R{n}");
        expected_ids.push(step_id.clone());
        let cwd = env.tmp.path().to_path_buf();
        handles.push(thread::spawn(move || {
            let child = Command::new(common::mp_bin())
                .current_dir(cwd)
                .env("MP_HOME", common::repo_root())
                .args([
                    "milestone",
                    "step",
                    "add",
                    &id,
                    "--wp",
                    "WP1",
                    "--id",
                    &step_id,
                    "--action",
                    &format!("parallel race step {n}"),
                    "--tests",
                    "manual: race",
                    "--done-when",
                    "race done",
                    "--format",
                    "json",
                ])
                .output();
            (n, step_id, child)
        }));
    }
    let mut failures: Vec<(usize, String)> = Vec::new();
    for handle in handles {
        let (i, step_id, child) = handle.join().expect("thread panic");
        let child = child.expect("spawn mp");
        if !child.status.success() {
            failures.push((
                i,
                format!(
                    "step_id={step_id} stderr={}",
                    String::from_utf8_lossy(&child.stderr)
                ),
            ));
        }
    }
    if !failures.is_empty() {
        eprintln!("note: some parallel step-adds failed (contention): {failures:?}");
    }

    // Read back every step id. The hard assertion: every S-R* id must
    // be present on disk. Pre-M113 the lost-writer-wins race would
    // leave < N ids here.
    let list_out = env.run(&["list", "steps", "--milestone", &id, "--select", "step.id"]);
    assert!(
        list_out.status.success(),
        "list steps failed: {}",
        String::from_utf8_lossy(&list_out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let arr = v["steps"].as_array().expect("steps array");
    let landed: std::collections::HashSet<String> = arr
        .iter()
        .filter_map(|s| s.as_str().map(|s| s.to_string()))
        .collect();
    for want in &expected_ids {
        assert!(
            landed.contains(want),
            "parallel step {want} must land in the milestone — pre-M113 this was lost to a write race. Landed: {:?}",
            landed
        );
    }
}

#[test]
fn parallel_collection_writers_preserve_every_update_and_unique_id() {
    const N: usize = 6;

    let backlog_env = TestEnv::new();
    run_parallel(
        &backlog_env,
        (0..N)
            .map(|n| {
                vec![
                    "backlog".into(),
                    "add".into(),
                    "--desc".into(),
                    format!("parallel backlog {n}"),
                ]
            })
            .collect(),
    );
    let backlog = backlog_env.run_json(&["backlog", "list"]);
    let backlog_items = backlog["items"].as_array().unwrap();
    assert_eq!(backlog_items.len(), N);
    let backlog_ids: std::collections::HashSet<_> = backlog_items
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(backlog_ids.len(), N);

    let track_env = TestEnv::new();
    run_parallel(
        &track_env,
        (0..N)
            .map(|n| {
                vec![
                    "track".into(),
                    "add".into(),
                    "bugfix".into(),
                    "--title".into(),
                    format!("parallel track {n}"),
                ]
            })
            .collect(),
    );
    let track = track_env.run_json(&["track", "show", "bugfix"]);
    let track_items = track["items"].as_array().unwrap();
    assert_eq!(track_items.len(), N);
    let track_ids: std::collections::HashSet<_> = track_items
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(track_ids.len(), N);

    let annotation_env = TestEnv::new();
    run_parallel(
        &annotation_env,
        (0..N)
            .map(|n| {
                vec![
                    "annotation".into(),
                    "create".into(),
                    format!("M{n:02}"),
                    "review-request".into(),
                    format!("parallel annotation {n}"),
                    "runner".into(),
                ]
            })
            .collect(),
    );
    let annotations = annotation_env.run_json(&["annotation", "list"]);
    let annotation_items = annotations["annotations"].as_array().unwrap();
    assert_eq!(annotation_items.len(), N);
    let annotation_ids: std::collections::HashSet<_> = annotation_items
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(annotation_ids.len(), N);

    let brief_env = TestEnv::new();
    let todo = brief_env.run_json(&["brief", "todo"]);
    let topics = todo["topics"].as_array().unwrap();
    let first = topics[0]["id"].as_str().unwrap().to_string();
    let second = topics[1]["id"].as_str().unwrap().to_string();
    run_parallel(
        &brief_env,
        vec![
            vec![
                "brief".into(),
                "edit".into(),
                first.clone(),
                "--body".into(),
                "first concurrent body".into(),
            ],
            vec![
                "brief".into(),
                "edit".into(),
                second.clone(),
                "--body".into(),
                "second concurrent body".into(),
            ],
        ],
    );
    assert_eq!(
        brief_env.run_json(&["brief", "show", &first])["topic"]["body"],
        "first concurrent body"
    );
    assert_eq!(
        brief_env.run_json(&["brief", "show", &second])["topic"]["body"],
        "second concurrent body"
    );

    let session_env = TestEnv::new();
    run_parallel(
        &session_env,
        (0..2)
            .map(|n| {
                vec![
                    "session".into(),
                    "start".into(),
                    "--branch".into(),
                    format!("parallel-session-{n}"),
                ]
            })
            .collect(),
    );
    let sessions = session_env.run_json(&["session", "list"]);
    let session_items = sessions["sessions"].as_array().unwrap();
    assert_eq!(session_items.len(), 2);
    assert_ne!(
        session_items[0]["milestone_id"],
        session_items[1]["milestone_id"]
    );
}

#[test]
fn held_lock_surfaces_clear_error() {
    // Bounded-timeout path: a second `mp` invocation against a held
    // lock must fail with a clear "plan file is locked" error rather
    // than blocking forever or silently clobbering the holder.
    use mp::plan_io::PlanWriteLock;

    let env = TestEnv::new();
    let id = create_and_approve(&env, "m113-lock-held");

    // Resolve `.mp-write.lock` against the same plan dir that `env.run`
    // uses (which is `cwd`, with `mp` auto-discovering `master-plan/`).
    let lock_path = env.tmp.path().join("master-plan/.mp-write.lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir lock dir");
    }

    // Hold the lock for the full test duration in this thread.
    let holder_guard =
        PlanWriteLock::acquire_blocking(&lock_path).expect("holder must grab the lock");
    eprintln!("holder acquired lock at {}", lock_path.display());
    let milestone_path = std::fs::read_dir(env.tmp.path().join("master-plan/milestones"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("{id}-"))
        })
        .unwrap();
    let before = std::fs::read(&milestone_path).unwrap();

    // Spawn a separate `mp` process that will hit the same lock. Set a
    // very short timeout so the test stays fast and the failure mode
    // surfaces quickly.
    let cwd = env.tmp.path().to_path_buf();
    let result = std::thread::Builder::new()
        .name("contender".into())
        .spawn(move || {
            std::process::Command::new(common::mp_bin())
                .current_dir(cwd)
                .env("MP_HOME", common::repo_root())
                .env("MP_LOCK_TIMEOUT_SECS", "2")
                .args([
                    "milestone",
                    "step",
                    "add",
                    &id,
                    "--wp",
                    "WP1",
                    "--id",
                    "S-LOCKED",
                    "--action",
                    "under-held-lock",
                    "--tests",
                    "manual: under lock",
                    "--done-when",
                    "lock released",
                ])
                .output()
        })
        .expect("spawn contender")
        .join()
        .expect("contender thread panic");
    let child = result.expect("contender output");
    let _ = holder_guard; // drop on test exit

    assert!(
        !child.status.success(),
        "step add under held lock should fail clearly; got status: {:?}, stdout: {}",
        child.status,
        String::from_utf8_lossy(&child.stdout)
    );
    let stderr = String::from_utf8_lossy(&child.stderr);
    assert!(
        stderr.contains("lock") || stderr.contains("timeout") || stderr.contains("EWOULDBLOCK"),
        "error must mention the lock/timeout; got: {stderr}"
    );
    assert_eq!(
        std::fs::read(milestone_path).unwrap(),
        before,
        "lock timeout changed the source file"
    );
}
