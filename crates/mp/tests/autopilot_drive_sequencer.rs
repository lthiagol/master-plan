//! M149 S8 / AC-04, AC-05: cross-milestone sequencing with per-role
//! pane reuse.
//!
//! Two test surfaces:
//!
//! - **AC-04 (pane reuse)** — verified at the `SystemDriveOps` level:
//!   once the pane cache is populated, `ensure_pane` returns the
//!   cached handle without calling herdr. The cache persists across
//!   milestones because `run_milestones` reuses one ops instance.
//! - **AC-05 (sequential)** — verified via the empty-list contract +
//!   outcomes-preserve-order property using a fake mp that reports
//!   complete on first read.

mod common;

use crate::common::TestEnv;
use mp::autopilot::drive::{
    ensure_pane, run_milestones, DriveOps, PaneHandle, Role, RoleConfigs, SystemDriveOps,
};
use mp::config::RoleConfig;
use std::fs;
use std::path::{Path, PathBuf};

fn install_script(path: &Path, body: &str) -> PathBuf {
    fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
    path.to_path_buf()
}

fn install_fake_herdr_with_log(dir: &Path, log: &Path) -> PathBuf {
    let body = format!(
        r#"#!/bin/sh
echo "argv: $*" >> "{log}"
case "$2" in
  list) echo '{{"agents":[]}}' ;;
  *) echo '{{}}' ;;
esac
"#,
        log = log.display()
    );
    install_script(&dir.join("herdr"), &body)
}

fn install_fake_mp_complete(dir: &Path) -> PathBuf {
    install_script(
        &dir.join("mp"),
        r#"#!/bin/sh
echo '{"milestone":{"lifecycle":"complete"}}'
"#,
    )
}

fn role_configs() -> RoleConfigs {
    RoleConfigs {
        runner: RoleConfig {
            harness: Some("opencode".into()),
            ..Default::default()
        },
        coordinator: RoleConfig {
            harness: Some("opencode".into()),
            ..Default::default()
        },
    }
}

#[test]
fn empty_id_list_is_a_noop_with_all_complete_true() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = bin_dir.join("herdr.log");
    fs::create_dir_all(&bin_dir).unwrap();
    let herdr = install_fake_herdr_with_log(&bin_dir, &log);
    let mp = install_fake_mp_complete(&bin_dir);

    let mut ops = SystemDriveOps::new(mp, herdr, env.tmp.path(), "1", role_configs());
    ops.set_wait_options(mp::autopilot::drive::WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: 0,
    });
    let report = run_milestones(&mut ops, &[], 5).unwrap();
    assert!(report.all_complete);
    assert!(report.outcomes.is_empty());
}

#[test]
fn pane_cache_hit_skips_herdr_spawn_ac04() {
    // Pre-populate the pane_cache as if the runner pane was already
    // spawned by a prior milestone. The next ensure_pane(runner) must
    // return the cached handle WITHOUT invoking herdr — the herdr log
    // stays empty.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = bin_dir.join("herdr.log");
    fs::create_dir_all(&bin_dir).unwrap();
    let herdr = install_fake_herdr_with_log(&bin_dir, &log);

    let mut ops = SystemDriveOps::new(
        env.tmp.path().join("mp-not-used"),
        herdr,
        env.tmp.path(),
        "1",
        role_configs(),
    );
    let cached = PaneHandle {
        label: "role-runner-1".into(),
        pane_id: "%cached".into(),
        reused: true,
    };
    ops.prefill_pane_cache(Role::Runner, cached);

    // Now go through the ops struct: cache hit, no spawn. The herdr log
    // stays empty (no new spawn for the cache-hit case).
    let got = SystemDriveOps::ensure_pane(&mut ops, Role::Runner).unwrap();
    assert_eq!(got.pane_id, "%cached");
    assert!(got.reused);
    let log_text = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !log_text.contains("agent start"),
        "cache hit must not invoke herdr agent start: {log_text}"
    );
    assert_eq!(
        ops.cached_pane(Role::Runner).map(|h| h.pane_id.clone()),
        Some("%cached".to_string())
    );
}

#[test]
fn ops_ensure_pane_caches_first_spawn_for_reuse_ac04() {
    // First ensure_pane(runner) spawns + caches; second ensure_pane
    // (runner) returns the cached handle without spawning again. This
    // is the AC-04 contract at the ops-struct level.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = bin_dir.join("herdr.log");
    fs::create_dir_all(&bin_dir).unwrap();
    let herdr = install_fake_herdr_with_log(&bin_dir, &log);

    let mut ops = SystemDriveOps::new(
        env.tmp.path().join("mp-not-used"),
        herdr,
        env.tmp.path(),
        "1",
        role_configs(),
    );
    // First call: spawns + caches.
    let first = SystemDriveOps::ensure_pane(&mut ops, Role::Runner).unwrap();
    assert!(!first.reused, "first call should spawn, not reuse");
    assert!(ops.cached_pane(Role::Runner).is_some());
    let spawns_after_first = fs::read_to_string(&log)
        .unwrap()
        .matches("agent start role-runner-1")
        .count();
    assert_eq!(spawns_after_first, 1);

    // Second call: cache hit, no new spawn.
    let second = SystemDriveOps::ensure_pane(&mut ops, Role::Runner).unwrap();
    assert!(second.reused, "second call should hit the cache");
    let spawns_after_second = fs::read_to_string(&log)
        .unwrap()
        .matches("agent start role-runner-1")
        .count();
    assert_eq!(
        spawns_after_second, 1,
        "no new spawn on cache hit (AC-04 pane reuse)"
    );
}

#[test]
fn ops_ensure_pane_separates_runner_and_coordinator_panes() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = bin_dir.join("herdr.log");
    fs::create_dir_all(&bin_dir).unwrap();
    let herdr = install_fake_herdr_with_log(&bin_dir, &log);

    let mut ops = SystemDriveOps::new(
        env.tmp.path().join("mp-not-used"),
        herdr,
        env.tmp.path(),
        "1",
        role_configs(),
    );
    let runner = SystemDriveOps::ensure_pane(&mut ops, Role::Runner).unwrap();
    let coord = SystemDriveOps::ensure_pane(&mut ops, Role::Coordinator).unwrap();
    assert_ne!(runner.pane_id, coord.pane_id);
    let log_text = fs::read_to_string(&log).unwrap();
    assert_eq!(
        log_text.matches("agent start role-runner-1").count(),
        1,
        "runner spawned once"
    );
    assert_eq!(
        log_text.matches("agent start role-coordinator-1").count(),
        1,
        "coordinator spawned once"
    );
}

#[test]
fn run_milestones_processes_in_input_order_ac05() {
    let env = TestEnv::new();
    // Create three real milestones. They land at lifecycle=draft →
    // each is skipped by should_skip, but the outcomes list still
    // preserves the input id order. This is the AC-05 contract: no
    // interleaving, strict input-order processing.
    let create_json = r#"{
        "title": "ordering fixture",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "ordering" },
        "problem": { "description": "p" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["y", "z"] },
        "acceptance_criteria": [
            { "description": "ac", "verification": "manual: yes" }
        ]
    }"#;
    let mut ids = Vec::new();
    for _ in 0..3 {
        let created = env.run_json(&[
            "milestone",
            "create",
            "--json",
            create_json,
            "--format",
            "json",
        ]);
        ids.push(created["milestone"]["id"].as_str().unwrap().to_string());
    }

    let bin_dir = env.tmp.path().join("fake-bin");
    let log = bin_dir.join("herdr.log");
    fs::create_dir_all(&bin_dir).unwrap();
    let herdr = install_fake_herdr_with_log(&bin_dir, &log);

    let mut ops = SystemDriveOps::new(
        common::mp_bin().to_path_buf(),
        herdr,
        env.tmp.path(),
        &ids[0],
        role_configs(),
    );
    ops.set_wait_options(mp::autopilot::drive::WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: 0,
    });
    let report = run_milestones(&mut ops, &ids, 5).unwrap();
    let out_ids: Vec<&str> = report.outcomes.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(out_ids, ids.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    // All three should be skipped (draft lifecycle).
    assert!(report.any_skipped);
    assert!(!report.all_complete);
}

#[test]
fn ensure_pane_standalone_reuses_when_label_matches() {
    // Direct check on the standalone ensure_pane helper that
    // SystemDriveOps delegates to on first call.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = bin_dir.join("herdr.log");
    fs::create_dir_all(&bin_dir).unwrap();
    // Fake whose list returns an existing role-runner-1 pane.
    let body = format!(
        r#"#!/bin/sh
echo "argv: $*" >> "{log}"
case "$2" in
  list) echo '{{"agents":[{{"name":"role-runner-1","pane_id":"%existing"}}]}}' ;;
  *) echo '{{}}' ;;
esac
"#,
        log = log.display()
    );
    let herdr = install_script(&bin_dir.join("herdr"), &body);

    let rc = RoleConfig {
        harness: Some("opencode".into()),
        ..Default::default()
    };
    let handle = ensure_pane(&herdr, Role::Runner, 1, &rc, env.tmp.path()).unwrap();
    assert!(handle.reused);
    assert_eq!(handle.pane_id, "%existing");
    let log_text = fs::read_to_string(&log).unwrap();
    assert!(
        !log_text.contains("agent start"),
        "should not spawn when an existing pane matches the label: {log_text}"
    );
}
