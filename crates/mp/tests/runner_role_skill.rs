use std::fs;

use common::repo_root;

mod common;

fn skills_dir() -> std::path::PathBuf {
    repo_root().join("templates/skills")
}

fn read_md(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| panic!("missing: {}", path.display()))
}

#[test]
fn mp_runner_walks_execute_then_fixing() {
    let runner_dir = skills_dir().join("mp-runner");
    assert!(runner_dir.is_dir(), "mp-runner skill dir must exist");

    let skill_md = read_md(&runner_dir.join("SKILL.md"));
    let executing_md = read_md(&runner_dir.join("executing.md"));
    let fixing_md = read_md(&runner_dir.join("fixing.md"));
    let atomic_writes_md = read_md(&runner_dir.join("atomic-writes.md"));

    // AC-01: SKILL.md has role identity, stage ownership, sub-mode map, hand-off contract
    assert!(
        skill_md.contains("runner"),
        "SKILL.md must contain 'runner' (role identity)"
    );
    let section_count = skill_md.lines().filter(|l| l.starts_with("## ")).count();
    assert!(
        section_count >= 3,
        "SKILL.md must have >= 3 top-level sections, got {}",
        section_count
    );
    assert!(
        skill_md.contains("Stage ownership"),
        "SKILL.md must have stage ownership table"
    );
    assert!(
        skill_md.contains("Sub-mode map"),
        "SKILL.md must have sub-mode map"
    );
    assert!(
        skill_md.contains("Hand-off contract"),
        "SKILL.md must have hand-off contract"
    );
    assert!(
        skill_md.contains("Evidence hygiene"),
        "SKILL.md must reference evidence hygiene discipline"
    );

    // AC-02: executing.md has the four canonical execution commands plus self-review
    assert!(
        executing_md.contains("mp milestone set-status") || executing_md.contains("set-status"),
        "executing.md must reference set-status command"
    );
    assert!(
        executing_md.contains("mp milestone step done") || executing_md.contains("step done"),
        "executing.md must reference step done command"
    );
    assert!(
        executing_md.to_lowercase().contains("criterion pass"),
        "executing.md must reference criterion pass command"
    );
    assert!(
        executing_md
            .to_lowercase()
            .contains("mp reviews finding add"),
        "executing.md must reference reviews finding add for self-review"
    );
    assert!(
        executing_md.contains("mp milestone complete") || executing_md.contains("complete"),
        "executing.md must reference milestone complete command"
    );
    let mp_cmd_count = executing_md
        .lines()
        .filter(|l| l.contains("mp milestone") || l.contains("mp reviews"))
        .count();
    assert!(
        mp_cmd_count >= 5,
        "executing.md must have >= 5 mp command references, got {}",
        mp_cmd_count
    );
    assert!(
        executing_md.to_lowercase().contains("evidence hygiene"),
        "executing.md must reference evidence hygiene"
    );

    // AC-03: fixing.md has fix cycle + session-boundary discipline
    // (inlined prose; no L-code provenance on the consumer surface).
    assert!(
        fixing_md.contains("author should not be the only reviewer")
            || fixing_md.contains("session-boundary"),
        "fixing.md must state the session-boundary discipline"
    );
    assert!(
        fixing_md.to_lowercase().contains("not the same")
            && fixing_md.to_lowercase().contains("session that fixes"),
        "fixing.md must state the session-boundary rule"
    );
    assert!(
        fixing_md.to_lowercase().contains("mp reviews finding list"),
        "fixing.md must reference finding list"
    );
    assert!(
        fixing_md
            .to_lowercase()
            .contains("mp reviews finding resolve"),
        "fixing.md must reference finding resolve"
    );
    assert!(
        fixing_md.to_lowercase().contains("mp milestone verify"),
        "fixing.md must reference milestone verify"
    );

    // AC-04: atomic-writes.md has killpg + process group
    assert!(
        atomic_writes_md.to_lowercase().contains("killpg"),
        "atomic-writes.md must reference killpg"
    );
    assert!(
        atomic_writes_md.to_lowercase().contains("process group"),
        "atomic-writes.md must reference process group"
    );
    assert!(
        atomic_writes_md.to_lowercase().contains("advisory"),
        "atomic-writes.md must reference advisory lock"
    );

    // Verify manifest has correct id and consumes mp-flow
    let manifest_path = runner_dir.join("manifest.json");
    assert!(manifest_path.is_file(), "manifest.json must exist");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["id"], "mp-runner");
    assert!(manifest["consumes"].is_array());
    let consumes: Vec<&str> = manifest["consumes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(consumes.contains(&"mp-flow"));
}

#[test]
fn mp_runner_deploys_cleanly() {
    let env = TestEnv::blank();
    let root = repo_root();
    let install_dir = env.tmp.path().join("install-target");

    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", &install_dir)
        .arg("--plan-dir")
        .arg(env.tmp.path().join("master-plan"))
        .arg("install")
        .arg("--dev")
        .arg("--harness")
        .arg("opencode")
        .arg("--skills")
        .arg("mp-runner")
        .arg("--format")
        .arg("json");
    common::isolated_harness_env(&mut cmd, env.tmp.path());

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install --skills=mp-runner should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify mp-runner SKILL.md is deployed
    let skill_dir = env.tmp.path().join("harness/opencode/skills/mp-runner");
    let skill_md = skill_dir.join("SKILL.md");
    assert!(skill_md.is_file(), "SKILL.md should be deployed");
    let content = fs::read_to_string(&skill_md).unwrap();
    assert!(
        content.contains("runner"),
        "deployed SKILL.md should contain the role identity"
    );

    // Verify templates are in the install target
    let templates_dir = env
        .tmp
        .path()
        .join("install-target/templates/skills/mp-runner");
    assert!(templates_dir.join("SKILL.md").is_file());
    assert!(templates_dir.join("executing.md").is_file());
    assert!(templates_dir.join("fixing.md").is_file());
    assert!(templates_dir.join("atomic-writes.md").is_file());
}

struct TestEnv {
    pub tmp: tempfile::TempDir,
}

impl TestEnv {
    fn blank() -> Self {
        Self {
            tmp: tempfile::TempDir::new().expect("temp"),
        }
    }
}
