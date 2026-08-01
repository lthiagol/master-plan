use std::fs;
use std::process::Command;

use crate::common::{isolated_harness_env, mp_bin, path_with_install_bin, repo_root, TestEnv};
use tempfile::TempDir;

#[test]
fn init_from_repo_prefills_plan() {
    let env = TestEnv::blank();
    fs::create_dir_all(env.tmp.path().join("src")).expect("src");
    fs::create_dir_all(env.tmp.path().join("tests")).expect("tests");
    fs::write(
        env.tmp.path().join("Cargo.toml"),
        "[package]\nname = \"demo-app\"\nversion = \"0.1.0\"\n",
    )
    .expect("cargo");
    fs::write(
        env.tmp.path().join("README.md"),
        "# Demo application\n\nA sample brownfield repo.\n",
    )
    .expect("readme");

    let out = env.run_with_env(
        &[],
        &[
            "init",
            "--profile",
            "full",
            "--from-repo",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let bootstrap = &json["bootstrap"];
    assert_eq!(bootstrap["brownfield_likely"], true);
    assert!(bootstrap["stack"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s == "rust"));
    assert_eq!(bootstrap["project_name"], "demo-app");

    let plan = env.run(&["plan", "show", "--format", "json"]);
    assert!(plan.status.success());
    let plan_json: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan_json["plan"]["project"]["name"], "demo-app");
    assert_eq!(plan_json["plan"]["project"]["stack"][0], "rust");
}

#[test]
fn install_to_custom_dirs() {
    let install_root = TempDir::new().expect("install");
    let agents_skill = TempDir::new().expect("agents");
    let cursor_skill = TempDir::new().expect("cursor");
    let work = TempDir::new().expect("work");

    let source = repo_root().to_string_lossy().to_string();
    let path_with_install = path_with_install_bin(install_root.path());
    let mut install_cmd = Command::new(mp_bin());
    install_cmd
        .current_dir(work.path())
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", install_root.path())
        .env("PATH", &path_with_install);
    // Default all 8 harness skill-dirs to install_root subdirs, then
    // override the two the test inspects so the deploy lands in the
    // dedicated TempDirs we assert against.
    isolated_harness_env(&mut install_cmd, install_root.path());
    install_cmd
        .env("MP_OPENCODE_SKILL_DIR", agents_skill.path())
        .env("MP_CURSOR_SKILL_DIR", cursor_skill.path());
    let out = install_cmd
        .args([
            "install",
            "--harness",
            "both",
            "--dev",
            "--source",
            &source,
            "--format",
            "json",
        ])
        .output()
        .expect("install");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert!(install_root.path().join("bin/mp").is_file());
    assert!(install_root.path().join("bin/raul").is_file());
    assert!(install_root.path().join("templates").is_dir());
    // Post-M141: each skill gets its own subdir under the harness's
    // skill root. The harness's MP_<id>_SKILL_DIR points at the
    // parent; SKILL.md lands at MP_<id>_SKILL_DIR/<skill_id>/SKILL.md.
    assert!(agents_skill
        .path()
        .join("mp-flow")
        .join("SKILL.md")
        .is_file());
    assert!(cursor_skill
        .path()
        .join("mp-flow")
        .join("SKILL.md")
        .is_file());
    assert!(json["path_snippet"].as_str().unwrap().contains("MP_HOME"));
}

#[test]
fn doctor_reports_harness_section() {
    let install_stub = TempDir::new().expect("install stub");
    let out = Command::new(mp_bin())
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", install_stub.path())
        .args(["doctor", "--format", "json"])
        .output()
        .expect("doctor");
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let harnesses = json["harnesses"].as_array().expect("harnesses array");
    assert!(harnesses.iter().any(|h| h["id"] == "opencode"));
    assert!(harnesses.iter().any(|h| h["id"] == "cursor"));
}
