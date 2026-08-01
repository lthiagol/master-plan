use std::fs;

use crate::common::{mp_bin, repo_root};
use tempfile::TempDir;

#[test]
fn init_creates_root_agents_md_by_default() {
    let project = TempDir::new().expect("project");

    let out = std::process::Command::new(mp_bin())
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

    let root_agents = project.path().join("AGENTS.md");
    assert!(
        root_agents.is_file(),
        "root AGENTS.md should exist by default"
    );
    let content = fs::read_to_string(&root_agents).unwrap();
    assert!(
        content.contains("master-plan"),
        "should reference master-plan"
    );
}

#[test]
fn skip_root_agents_skips_root_agents_md() {
    let project = TempDir::new().expect("project");

    let out = std::process::Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args([
            "init",
            "--profile",
            "full",
            "--skip-root-agents",
            "--format",
            "json",
        ])
        .output()
        .expect("init");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let root_agents = project.path().join("AGENTS.md");
    assert!(
        !root_agents.exists(),
        "root AGENTS.md should not exist with --skip-root-agents"
    );
}

#[test]
fn init_does_not_overwrite_existing_root_agents() {
    let project = TempDir::new().expect("project");
    // Pre-create a root file
    let root_path = project.path().join("AGENTS.md");
    fs::write(&root_path, "existing content").unwrap();

    let out = std::process::Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args(["init", "--profile", "full", "--format", "json"])
        .output()
        .expect("init");

    assert!(out.status.success());
    let content = fs::read_to_string(&root_path).unwrap();
    assert_eq!(
        content, "existing content",
        "existing root AGENTS.md should not be overwritten"
    );
}
