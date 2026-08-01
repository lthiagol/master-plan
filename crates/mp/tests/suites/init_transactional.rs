use std::fs;
use std::process::Command;

use crate::common::{mp_bin, repo_root};
use tempfile::TempDir;

/// Force a failure mid-init by corrupting the plan.toml template;
/// assert no partial master-plan/ directory remains.
#[test]
fn init_transactional_no_partial_dir_on_failure() {
    let project = TempDir::new().expect("project");
    let broken_home = setup_broken_templates();

    let plan_dir = project.path().join("master-plan");
    assert!(!plan_dir.exists(), "plan dir should not exist before init");

    let out = Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", broken_home.path())
        .args(["init", "--profile", "full", "--format", "json"])
        .output()
        .expect("init");

    // Should fail because plan.toml template is corrupted
    assert!(
        !out.status.success(),
        "init should fail with corrupted template: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No partial master-plan/ dir should remain
    assert!(
        !plan_dir.exists(),
        "no partial master-plan/ should remain after failed init"
    );
}

/// Normal init from valid templates succeeds.
#[test]
fn init_succeeds_with_valid_templates() {
    let project = TempDir::new().expect("project");
    let plan_dir = project.path().join("master-plan");

    let out = Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args(["init", "--profile", "full", "--format", "json"])
        .output()
        .expect("init");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        plan_dir.is_dir(),
        "plan dir should exist after successful init"
    );
    assert!(plan_dir.join("plan.json").is_file());
    assert!(plan_dir.join("AGENTS.md").is_file());
}

/// Legacy TOML plan trees must be migrated — init must not scaffold JSON alongside.
#[test]
fn init_refuses_legacy_toml_plan_dir() {
    let project = TempDir::new().expect("project");
    let plan_dir = project.path().join("master-plan");
    fs::create_dir_all(plan_dir.join("milestones")).expect("milestones dir");
    fs::write(plan_dir.join("plan.toml"), "version = 1\n").expect("plan.toml");
    fs::write(plan_dir.join("milestones/01-test.toml"), "id = \"01\"\n").expect("milestone");

    let out = Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args(["init", "--profile", "full", "--format", "json"])
        .output()
        .expect("init");

    assert!(
        !out.status.success(),
        "init must refuse legacy TOML: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("legacy TOML") && stderr.contains("migrate"),
        "stderr should mention legacy TOML and migrate: {stderr}"
    );
    assert!(
        !plan_dir.join("plan.json").exists(),
        "must not scaffold plan.json alongside legacy TOML"
    );
    assert!(plan_dir.join("plan.toml").is_file());
    assert!(plan_dir.join("milestones/01-test.toml").is_file());
}

/// --force does not bypass legacy TOML — migration is required first.
#[test]
fn init_refuses_legacy_toml_even_with_force() {
    let project = TempDir::new().expect("project");
    let plan_dir = project.path().join("master-plan");
    fs::create_dir_all(&plan_dir).expect("plan dir");
    fs::write(plan_dir.join("plan.toml"), "version = 1\n").expect("plan.toml");

    let out = Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args(["init", "--profile", "full", "--force", "--format", "json"])
        .output()
        .expect("init");

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("legacy TOML"),
        "stderr should mention legacy TOML: {stderr}"
    );
    assert!(!plan_dir.join("plan.json").exists());
}

/// Re-init on existing project aborts without --force.
#[test]
fn reinit_on_existing_fails_without_force() {
    let project = TempDir::new().expect("project");
    let _plan_dir = project.path().join("master-plan");

    let out = Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args(["init", "--profile", "full", "--format", "json"])
        .output()
        .expect("init 1");
    assert!(out.status.success());

    // Second init without --force should fail
    let out2 = Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args(["init", "--profile", "full", "--format", "json"])
        .output()
        .expect("init 2");
    assert!(
        !out2.status.success(),
        "re-init without --force should fail"
    );
    let stderr = String::from_utf8_lossy(&out2.stderr);
    assert!(
        stderr.contains("already initialized") || stderr.contains("--force"),
        "stderr should mention already initialized or --force: {}",
        stderr
    );
}

/// --force re-generates on existing project.
#[test]
fn force_reinit_succeeds() {
    let project = TempDir::new().expect("project");
    let plan_dir = project.path().join("master-plan");

    let out = Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args(["init", "--profile", "full", "--format", "json"])
        .output()
        .expect("init 1");
    assert!(out.status.success());

    let out2 = Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args(["init", "--profile", "full", "--force", "--format", "json"])
        .output()
        .expect("init 2");
    assert!(
        out2.status.success(),
        "re-init with --force should succeed: {}",
        String::from_utf8_lossy(&out2.stderr)
    );
    assert!(plan_dir.is_dir());
    assert!(plan_dir.join("plan.json").is_file());
}

fn setup_broken_templates() -> TempDir {
    let tmp = TempDir::new().expect("temp");
    let src = repo_root().join("templates");
    let dst = tmp.path().join("templates");
    copy_recursive(&src, &dst);

    // Corrupt plan.json so init fails at plan parsing
    let plan = dst.join("defaults/plan.json");
    fs::write(&plan, "{{{ definitely not valid json for sure }}}").expect("corrupt plan");
    tmp
}

fn copy_recursive(src: &std::path::Path, dst: &std::path::Path) {
    if src.is_dir() {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            copy_recursive(&entry.path(), &dst.join(entry.file_name()));
        }
    } else {
        fs::copy(src, dst).unwrap();
    }
}
