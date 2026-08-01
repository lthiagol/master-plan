//! M132 AC-02: parallel test processes must not share
//! `~/.agents/master-plan/` install-dir state. Each test (or test
//! process) gets isolated state via per-test `MP_INSTALL_DIR` and
//! per-harness `MP_<HARNESS>_SKILL_DIR` overrides, so an `uninstall`
//! / `stow` in one process cannot race another.
//!
//! (MP_HOME itself is shared at the repo root — it's read-only during
//! install for asset lookups — and isolation is achieved via the
//! per-test `MP_INSTALL_DIR` plus the 8 per-harness env vars set by
//! `isolated_harness_env`.)
//!
//! Pre-M132, four integration tests under default threading
//! intermittently failed in this exact way:
//! - `suite_install::uninstall_removes_env_sh`
//! - `milestone_bulk::bulk_dry_run_does_not_persist`
//! - `install_deploy::uninstall_removes_raul_binary`
//! - `milestone_priority::set_priority_all_valid_values`
//!
//! The fix: route every install/uninstall through per-test `TempDir`s
//! for both `MP_INSTALL_DIR` and the per-harness `MP_<HARNESS>_SKILL_DIR`
//! env vars, so no two concurrent processes touch the same on-disk
//! path.
//!
//! `parallel_tests_use_isolated_agents_state` (single-harness path)
//! fires N=8 parallel `mp install --harness opencode` invocations
//! plus N=8 parallel `mp uninstall --purge`, then verifies (a) every
//! install succeeded, (b) every tempdir got its own `bin/mp` +
//! `bin/raul` + `env.sh` (no shared filesystem state), and (c) every
//! uninstall only removed its own tempdir (no cross-process
//! contamination).
//!
//! `parallel_tests_isolate_all_eight_harnesses` (multi-harness path)
//! widens coverage: it fires N=8 parallel `--harness both` installs
//! and asserts each of the 8 `MP_<HARNESS>_SKILL_DIR` paths got its
//! own `SKILL.md` (so a regression where two harnesses silently
//! collided would be caught).

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use tempfile::TempDir;

const N: usize = 8;

fn install_at(install_root: &Path) -> std::process::Output {
    let source = common::repo_root().to_string_lossy().to_string();
    let mut cmd = Command::new(common::mp_bin());
    cmd.env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", install_root)
        .args([
            "install",
            "--harness",
            "opencode",
            "--dev",
            "--source",
            &source,
            "--format",
            "json",
        ]);
    common::isolated_harness_env(&mut cmd, install_root);
    cmd.output().expect("install")
}

fn install_all_harnesses_at(install_root: &Path) -> std::process::Output {
    let source = common::repo_root().to_string_lossy().to_string();
    let mut cmd = Command::new(common::mp_bin());
    cmd.env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", install_root)
        .args([
            "install",
            "--harness",
            "both",
            "--dev",
            "--source",
            &source,
            "--format",
            "json",
        ]);
    common::isolated_harness_env(&mut cmd, install_root);
    cmd.output().expect("install")
}

fn uninstall_at(install_root: &Path) -> std::process::Output {
    let mut cmd = Command::new(common::mp_bin());
    cmd.env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", install_root)
        .args(["uninstall", "--purge", "--format", "json"]);
    common::isolated_harness_env(&mut cmd, install_root);
    cmd.output().expect("uninstall")
}

fn harness_subpath(id: &str) -> &'static str {
    match id {
        "opencode" => "harness/opencode/skills",
        "cursor" => "harness/cursor/skills",
        "claude-code" => "harness/claude-code/skills",
        "gemini" => "harness/gemini/skills",
        "codex" => "harness/codex/skills",
        "windsurf" => "harness/windsurf/skills",
        "cline" => "harness/cline/skills",
        "pi" => "harness/pi/agent/skills",
        other => panic!("unknown harness id {other}"),
    }
}

fn assert_toolkit_artifacts(root: &Path, i: usize, phase: &str) {
    assert!(
        root.join("bin/mp").is_file(),
        "thread {i} ({phase}): bin/mp missing under its own tempdir {}",
        root.display()
    );
    assert!(
        root.join("bin/raul").is_file(),
        "thread {i} ({phase}): bin/raul missing under its own tempdir {}",
        root.display()
    );
    assert!(
        root.join("env.sh").is_file(),
        "thread {i} ({phase}): env.sh missing under its own tempdir {}",
        root.display()
    );
}

fn assert_toolkit_cleanup(root: &Path, i: usize, phase: &str) {
    assert!(
        !root.join("bin/mp").exists(),
        "thread {i} ({phase}): bin/mp should be gone after uninstall"
    );
    assert!(
        !root.join("bin/raul").exists(),
        "thread {i} ({phase}): bin/raul should be gone after uninstall"
    );
    assert!(
        !root.join("env.sh").exists(),
        "thread {i} ({phase}): env.sh should be gone after uninstall"
    );
}

fn run_parallel<F>(spawn_op: F, op_name: &str) -> Vec<(usize, TempDir, std::process::Output)>
where
    F: Fn(&Path) -> std::process::Output + Send + Clone + 'static,
{
    let roots: Vec<TempDir> = (0..N).map(|_| TempDir::new().expect("temp")).collect();
    let mut handles = Vec::new();
    for (i, root) in roots.iter().enumerate() {
        let root_path: PathBuf = root.path().to_path_buf();
        let spawn_op = spawn_op.clone();
        handles.push(thread::spawn(move || {
            let out = spawn_op(&root_path);
            (i, out)
        }));
    }
    let mut owned: Vec<(usize, TempDir, std::process::Output)> = Vec::new();
    let mut failures: Vec<(usize, String)> = Vec::new();
    for (handle, root) in handles.into_iter().zip(roots) {
        let (i, out) = handle.join().expect("thread panic");
        if !out.status.success() {
            failures.push((
                i,
                format!(
                    "{op_name} failed: status={:?} stderr={}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr)
                ),
            ));
        }
        owned.push((i, root, out));
    }
    assert!(
        failures.is_empty(),
        "every parallel {op_name} must succeed; failures: {failures:?}"
    );
    owned.sort_by_key(|(i, _, _)| *i);
    owned
}

#[test]
fn parallel_tests_use_isolated_agents_state() {
    let owned = run_parallel(install_at, "install");

    for (i, root, _) in &owned {
        assert_toolkit_artifacts(root.path(), *i, "install");
    }

    // Phase 2: fire N parallel `uninstall --purge`. Each thread only
    // touches its OWN tempdir.
    let paths: Vec<PathBuf> = owned
        .iter()
        .map(|(_, r, _)| r.path().to_path_buf())
        .collect();
    let mut handles = Vec::new();
    for (i, root, _) in owned {
        let path = paths[i].clone();
        handles.push(thread::spawn(move || (i, root, uninstall_at(&path))));
    }
    let mut uninstalled: Vec<(usize, TempDir, std::process::Output)> = Vec::new();
    let mut failures: Vec<(usize, String)> = Vec::new();
    for handle in handles {
        let (i, root, out) = handle.join().expect("thread panic");
        if !out.status.success() {
            failures.push((
                i,
                format!(
                    "uninstall failed: status={:?} stderr={}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr)
                ),
            ));
        }
        uninstalled.push((i, root, out));
    }
    assert!(
        failures.is_empty(),
        "every parallel uninstall must succeed; failures: {failures:?}"
    );
    uninstalled.sort_by_key(|(i, _, _)| *i);
    for (i, root, _) in &uninstalled {
        assert_toolkit_cleanup(root.path(), *i, "uninstall");
    }
}

#[test]
fn parallel_tests_isolate_all_eight_harnesses() {
    // Same harness as the single-harness test, but `--harness both`
    // expands to all 8 harnesses. Verify every harness's
    // MP_<HARNESS>_SKILL_DIR override produced a distinct on-disk
    // SKILL.md inside its OWN tempdir.
    let owned = run_parallel(install_all_harnesses_at, "install --harness both");

    let harness_ids = [
        "opencode",
        "cursor",
        "claude-code",
        "gemini",
        "codex",
        "windsurf",
        "cline",
        "pi",
    ];
    for (i, root, _) in &owned {
        assert_toolkit_artifacts(root.path(), *i, "install");

        for h in harness_ids {
            let skill_dir = root.path().join(harness_subpath(h));
            // The install registry ships mp-flow + mp-runner + mp-coordinator
            // by default. Each skill gets its own subdir under the harness.
            let has_skill = ["mp-flow", "mp-runner", "mp-coordinator"]
                .iter()
                .any(|s| skill_dir.join(s).join("SKILL.md").is_file());
            assert!(
                has_skill,
                "thread {i}: {h} has no CPD skill SKILL.md under its own tempdir {} (subpath {})",
                root.path().display(),
                skill_dir.display()
            );
        }
    }
}
