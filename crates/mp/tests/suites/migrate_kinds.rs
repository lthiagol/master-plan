//! M102 R3 (F-04 / F-08): migrate_kinds collapses track-bugfix.json +
//! track-tweak.json + ideas.json into backlog.json with kind=bug /
//! kind=tweak / kind=idea. Idempotent (items already in backlog are
//! skipped on a re-run). --dry-run reports without writing. Pin
//! tests cover all three source kinds + the dry-run path + the
//! idempotency invariant.

use crate::common::lib_api;
use crate::common::TestEnv;
use mp_model::BacklogFile;

fn read_backlog(env: &TestEnv) -> BacklogFile {
    let raw = std::fs::read_to_string(env.tmp.path().join("master-plan/backlog.json"))
        .expect("backlog.json exists after migrate");
    serde_json::from_str(&raw).expect("backlog.json parses")
}

fn write_track_bugfix(env: &TestEnv, items: &[(&str, &str)]) {
    let objs: Vec<serde_json::Value> = items
        .iter()
        .map(|(id, title)| {
            serde_json::json!({
                "id": id, "title": title, "status": "pending",
                "effort": "S", "problem": "", "done_when": "", "verification": "",
                "evidence": "", "created": "2026-07-01", "completed": "",
                "archived_at": "", "steps": [],
            })
        })
        .collect();
    let body = serde_json::json!({
        "track": {
            "kind": "bugfix",
            "title": "Bugfixes",
            "perpetual": false,
            "scope": "in_scope",
            "created": "2026-07-01",
        },
        "items": objs,
    });
    // C-1 fix: track files live at master-plan/tracks/{kind}.json
    // (the canonical location). The previous test wrote to the old
    // flat path which masked the production path bug.
    std::fs::create_dir_all(env.tmp.path().join("master-plan/tracks")).unwrap();
    std::fs::write(
        env.tmp.path().join("master-plan/tracks/bugfix.json"),
        serde_json::to_string_pretty(&body).unwrap(),
    )
    .unwrap();
}

fn write_track_tweak(env: &TestEnv, items: &[(&str, &str)]) {
    let objs: Vec<serde_json::Value> = items
        .iter()
        .map(|(id, title)| {
            serde_json::json!({
                "id": id, "title": title, "status": "pending",
                "effort": "S", "problem": "", "done_when": "", "verification": "",
                "evidence": "", "created": "2026-07-01", "completed": "",
                "archived_at": "", "steps": [],
            })
        })
        .collect();
    let body = serde_json::json!({
        "track": {
            "kind": "tweak",
            "title": "Tweaks",
            "perpetual": false,
            "scope": "in_scope",
            "created": "2026-07-01",
        },
        "items": objs,
    });
    std::fs::create_dir_all(env.tmp.path().join("master-plan/tracks")).unwrap();
    std::fs::write(
        env.tmp.path().join("master-plan/tracks/tweak.json"),
        serde_json::to_string_pretty(&body).unwrap(),
    )
    .unwrap();
}

fn write_ideas(env: &TestEnv, items: &[(&str, &str)]) {
    let objs: Vec<serde_json::Value> = items
        .iter()
        .map(|(id, title)| {
            serde_json::json!({
                "id": id, "title": title, "body": "",
                "status": "open", "tags": [],
                "source": "conversation", "created": "2026-07-01",
                "promoted_to": "",
            })
        })
        .collect();
    let body = serde_json::json!({ "ideas": objs });
    std::fs::write(
        env.tmp.path().join("master-plan/ideas.json"),
        serde_json::to_string_pretty(&body).unwrap(),
    )
    .unwrap();
}

#[test]
fn migrate_kinds_collapses_three_sources_into_backlog() {
    let env = TestEnv::new();
    write_track_bugfix(&env, &[("BF-01", "bug A")]);
    write_track_tweak(&env, &[("TW-01", "tweak A")]);
    write_ideas(&env, &[("ID-01", "idea A")]);

    let out = lib_api::run(&env, &["migrate", "--kinds", "--format", "json"]);
    if !out.status.success() {
        eprintln!("MIGRATE STDERR: {}", String::from_utf8_lossy(&out.stderr));
        eprintln!("MIGRATE STDOUT: {}", String::from_utf8_lossy(&out.stdout));
    }
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["from_bugfix"], 1);
    assert_eq!(v["from_tweak"], 1);
    assert_eq!(v["from_ideas"], 1);

    // backlog.json should now have 3 items, sources from each kind.
    let bl = read_backlog(&env);
    assert_eq!(
        bl.items.len(),
        3,
        "expected 3 items in backlog; got: {:?}",
        bl.items
    );
    let kinds: Vec<&str> = bl.items.iter().map(|i| i.source.as_str()).collect();
    assert!(kinds.contains(&"track-bugfix"));
    assert!(kinds.contains(&"track-tweak"));
    assert!(kinds.contains(&"ideas"));

    // Priorities: bug/tweak default to "regular", ideas to "low".
    for item in &bl.items {
        match item.source.as_str() {
            "track-bugfix" | "track-tweak" => assert_eq!(item.priority, "regular"),
            "ideas" => assert_eq!(item.priority, "low"),
            _ => panic!("unexpected source: {}", item.source),
        }
    }

    // Source files deleted.
    assert!(!env
        .tmp
        .path()
        .join("master-plan/tracks/bugfix.json")
        .exists());
    assert!(!env
        .tmp
        .path()
        .join("master-plan/tracks/tweak.json")
        .exists());
    assert!(!env.tmp.path().join("master-plan/ideas.json").exists());
}

#[test]
fn migrate_kinds_dry_run_does_not_write() {
    let env = TestEnv::new();
    // Capture the pre-existing backlog.json mtime (TestEnv::new runs
    // mp init --profile full which creates a backlog.json). The
    // dry-run assertion is that the file is NOT touched — the
    // existence itself is fine (init creates it).
    let backlog_path = env.tmp.path().join("master-plan/backlog.json");
    assert!(
        backlog_path.exists(),
        "TestEnv init must create backlog.json"
    );
    let mtime_before = std::fs::metadata(&backlog_path)
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    write_track_bugfix(&env, &[("BF-01", "bug A")]);
    let out = lib_api::run(
        &env,
        &["migrate", "--kinds", "--dry-run", "--format", "json"],
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["from_bugfix"], 1);

    // Source files still present after dry-run (no delete).
    assert!(env
        .tmp
        .path()
        .join("master-plan/tracks/bugfix.json")
        .exists());
    // backlog.json mtime unchanged (no write).
    let mtime_after = std::fs::metadata(&backlog_path)
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        mtime_before, mtime_after,
        "dry-run must not touch backlog.json (mtime changed: before={mtime_before:?} after={mtime_after:?})"
    );
}

#[test]
fn migrate_kinds_is_idempotent() {
    let env = TestEnv::new();
    write_track_bugfix(&env, &[("BF-01", "bug A")]);

    // First migration.
    let out = lib_api::run(&env, &["migrate", "--kinds", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["from_bugfix"], 1);

    // Create a fresh track-bugfix with the SAME id (mimicking a stale
    // source) and re-run. Idempotency: items already in backlog are
    // skipped, so this re-run reports 0.
    write_track_bugfix(&env, &[("BF-01", "bug A duplicate")]);
    let out = lib_api::run(&env, &["migrate", "--kinds", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["from_bugfix"], 0, "duplicate id should be skipped");
    let bl = read_backlog(&env);
    assert_eq!(
        bl.items.len(),
        1,
        "no duplicate appended; got: {:?}",
        bl.items
    );
}

/// H-1 atomicity regression pin: on the real (non-dry-run) migration,
/// the source files are renamed to `.bak` BEFORE the merge is built,
/// so a partial failure doesn't lose data. After the backlog write
/// succeeds, the `.bak` files are deleted. The `.bak` file must NOT
/// exist after a successful migration (so a re-run sees an empty
/// `tracks/` directory).
#[test]
fn migrate_kinds_clears_bak_after_successful_migration() {
    let env = TestEnv::new();
    write_track_bugfix(&env, &[("BF-01", "bug A")]);
    write_track_tweak(&env, &[("TW-01", "tweak A")]);
    write_ideas(&env, &[("ID-01", "idea A")]);

    let out = lib_api::run(&env, &["migrate", "--kinds", "--format", "json"]);
    assert!(out.status.success());

    // The original source files are gone.
    assert!(!env
        .tmp
        .path()
        .join("master-plan/tracks/bugfix.json")
        .exists());
    assert!(!env
        .tmp
        .path()
        .join("master-plan/tracks/tweak.json")
        .exists());
    assert!(!env.tmp.path().join("master-plan/ideas.json").exists());

    // The .bak files are gone too (cleanup after successful write).
    assert!(!env
        .tmp
        .path()
        .join("master-plan/tracks/bugfix.json.bak")
        .exists());
    assert!(!env
        .tmp
        .path()
        .join("master-plan/tracks/tweak.json.bak")
        .exists());
    assert!(!env.tmp.path().join("master-plan/ideas.json.bak").exists());

    // The .tmp file (used for atomic rename) is also gone.
    assert!(!env.tmp.path().join("master-plan/backlog.json.tmp").exists());
}
