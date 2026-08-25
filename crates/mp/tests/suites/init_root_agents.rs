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
        "existing root AGENTS.md should not be overwritten by default"
    );

    // M194: the JSON output's `root_agents` field reports
    // `action: "skipped"` plus the snippet body so the caller
    // can render the manual-merge instructions.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("init output should be valid JSON");
    assert_eq!(
        v.get("root_agents")
            .and_then(|r| r.get("action"))
            .and_then(|a| a.as_str()),
        Some("skipped"),
        "default behavior on existing root AGENTS.md must be `skipped`; got: {stdout}"
    );
    assert!(
        v.get("root_agents")
            .and_then(|r| r.get("snippet"))
            .and_then(|s| s.as_str())
            .map(|s| s.contains("master-plan"))
            .unwrap_or(false),
        "skipped status should carry the snippet body so callers can render it"
    );
}

#[test]
fn init_force_overwrites_existing_root_agents() {
    let project = TempDir::new().expect("project");
    let root_path = project.path().join("AGENTS.md");
    fs::write(&root_path, "existing content").unwrap();

    let out = std::process::Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args(["init", "--profile", "full", "--format", "json", "--force"])
        .output()
        .expect("init --force");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let content = fs::read_to_string(&root_path).unwrap();
    assert_ne!(
        content, "existing content",
        "--force must overwrite existing root AGENTS.md"
    );
    assert!(
        content.contains("master-plan"),
        "after --force, the snippet should be present"
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        v.get("root_agents")
            .and_then(|r| r.get("action"))
            .and_then(|a| a.as_str()),
        Some("overwritten"),
        "force path must report `overwritten`; got: {stdout}"
    );
}

#[test]
fn init_merge_appends_to_existing_root_agents() {
    let project = TempDir::new().expect("project");
    let root_path = project.path().join("AGENTS.md");
    let prior = "existing content\n";
    fs::write(&root_path, prior).unwrap();

    let out = std::process::Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args([
            "init",
            "--profile",
            "full",
            "--format",
            "json",
            "--merge-root-agents",
        ])
        .output()
        .expect("init --merge-root-agents");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let content = fs::read_to_string(&root_path).unwrap();
    assert!(
        content.starts_with(prior),
        "--merge must preserve the original content at the top; got: {content:?}"
    );
    assert!(
        content.contains("master-plan"),
        "--merge must append the snippet after the original; got: {content:?}"
    );
    // The separator comment marks the merge boundary so a
    // human (or a future agent) can find where the snippet
    // was inserted.
    assert!(
        content.contains("<!-- master-plan: appended by `mp init --merge`"),
        "--merge must include a visible separator comment marking the boundary; got: {content:?}"
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        v.get("root_agents")
            .and_then(|r| r.get("action"))
            .and_then(|a| a.as_str()),
        Some("merged"),
        "merge path must report `merged`; got: {stdout}"
    );
}

#[test]
fn init_rejects_force_and_merge_root_agents_together() {
    // Clap-level guard: --force and --merge-root-agents are
    // mutually exclusive (destructive vs. additive intent).
    // Passing both is a clap parse error, not a runtime
    // surprise.
    let project = TempDir::new().expect("project");
    let root_path = project.path().join("AGENTS.md");
    fs::write(&root_path, "existing content").unwrap();

    let out = std::process::Command::new(mp_bin())
        .current_dir(project.path())
        .env("MP_HOME", repo_root())
        .args([
            "init",
            "--profile",
            "full",
            "--format",
            "json",
            "--force",
            "--merge-root-agents",
        ])
        .output()
        .expect("init --force --merge-root-agents");

    assert!(
        !out.status.success(),
        "--force --merge-root-agents must be rejected; got status=success\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--force")
            && stderr.contains("--merge-root-agents")
            && (stderr.contains("cannot be used with") || stderr.contains("mutually exclusive")),
        "clap must explain the conflict; got: {stderr}"
    );
    // The existing file must be left intact (no partial write).
    let content = fs::read_to_string(&root_path).unwrap();
    assert_eq!(
        content, "existing content",
        "a rejected flag combination must not modify the existing file"
    );
}
