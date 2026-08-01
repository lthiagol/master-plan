use std::process::Command;

use crate::common::{isolated_harness_env, mp_bin, repo_root, TestEnv};
use tempfile::TempDir;

#[test]
fn init_with_project_skills() {
    let env = TestEnv::blank();
    let out = env.run(&[
        "init",
        "--profile",
        "hybrid",
        "--with-cursor-skill",
        "--with-opencode-skill",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // M146: install_project_skill deploys only `category=core` skills
    // (mp-flow, mp-runner, mp-coordinator), matching global `mp install`.
    // Catalog skills (spec-grill, codebase-design, diagnosing-bugs) are
    // opt-in via `mp install --skills`, not auto-deployed on init.
    // With both --with-cursor-skill and --with-opencode-skill that's
    // 3 core skills * 2 harnesses = 6 paths.
    assert_eq!(json["skills_installed"].as_array().unwrap().len(), 6);
    for skill in ["mp-flow", "mp-runner", "mp-coordinator"] {
        assert!(
            env.tmp
                .path()
                .join(format!(".cursor/skills/{skill}/SKILL.md"))
                .exists(),
            ".cursor/skills/{skill}/SKILL.md should exist after init"
        );
        assert!(
            env.tmp
                .path()
                .join(format!(".opencode/skills/{skill}/SKILL.md"))
                .exists(),
            ".opencode/skills/{skill}/SKILL.md should exist after init"
        );
    }
}

#[test]
fn idea_promote_to_milestone_and_backlog() {
    let env = TestEnv::new();

    assert!(env
        .run(&[
            "idea",
            "create",
            "--title",
            "Notifications",
            "--body",
            "Add email alerts",
            "--format",
            "json",
        ])
        .status
        .success());

    let promote = env.run(&[
        "idea",
        "promote",
        "ID-01",
        "--to-milestone",
        "--format",
        "json",
    ]);
    assert!(
        promote.status.success(),
        "{}",
        String::from_utf8_lossy(&promote.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&promote.stdout).unwrap();
    assert!(json["promoted_to"]
        .as_str()
        .unwrap()
        .starts_with("milestone:"));

    assert!(env
        .run(&[
            "idea",
            "create",
            "--title",
            "Later thing",
            "--format",
            "json",
        ])
        .status
        .success());
    let promote_bl = env.run(&[
        "idea",
        "promote",
        "ID-02",
        "--to-backlog",
        "--format",
        "json",
    ]);
    assert!(promote_bl.status.success());
    let bl_json: serde_json::Value = serde_json::from_slice(&promote_bl.stdout).unwrap();
    assert!(bl_json["promoted_to"]
        .as_str()
        .unwrap()
        .starts_with("backlog:"));
}

#[test]
fn session_promote_copies_milestone() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "hybrid", "--format", "json"])
        .status
        .success());

    let start = env.run(&[
        "session",
        "start",
        "--branch",
        "feature/promote",
        "--title",
        "Promote me",
        "--format",
        "json",
    ]);
    assert!(start.status.success());
    let started: serde_json::Value = serde_json::from_slice(&start.stdout).unwrap();
    let sid = started["session_id"].as_str().unwrap();

    let promote = env.run(&["session", "promote", sid, "--format", "json"]);
    assert!(
        promote.status.success(),
        "{}",
        String::from_utf8_lossy(&promote.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&promote.stdout).unwrap();
    assert!(json["milestone_file"]
        .as_str()
        .unwrap()
        .contains("milestones/"));
}

#[test]
fn uninstall_removes_custom_dirs() {
    let install_root = TempDir::new().expect("install");
    let agents_skill = TempDir::new().expect("agents");
    let cursor_skill = TempDir::new().expect("cursor");
    let root = repo_root();

    let mut install_cmd = Command::new(mp_bin());
    install_cmd
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", install_root.path());
    // Default all 8 harness skill-dirs to install_root subdirs, then
    // override the two the test inspects so the deploy lands in the
    // dedicated TempDirs we assert against.
    isolated_harness_env(&mut install_cmd, install_root.path());
    install_cmd
        .env("MP_OPENCODE_SKILL_DIR", agents_skill.path())
        .env("MP_CURSOR_SKILL_DIR", cursor_skill.path());
    let install = install_cmd
        .args([
            "install",
            "--harness",
            "both",
            "--dev",
            "--source",
            &root.to_string_lossy(),
            "--format",
            "json",
        ])
        .output()
        .expect("install");
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );

    let mut uninstall_cmd = Command::new(mp_bin());
    uninstall_cmd.env("MP_INSTALL_DIR", install_root.path());
    isolated_harness_env(&mut uninstall_cmd, install_root.path());
    uninstall_cmd
        .env("MP_OPENCODE_SKILL_DIR", agents_skill.path())
        .env("MP_CURSOR_SKILL_DIR", cursor_skill.path());
    let uninstall = uninstall_cmd
        .args(["uninstall", "--harness", "both", "--format", "json"])
        .output()
        .expect("uninstall");
    assert!(uninstall.status.success());
    assert!(!install_root.path().join("bin/mp").exists());
    assert!(!install_root.path().join("bin/raul").exists());
}
