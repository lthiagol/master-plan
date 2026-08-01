//! M173 S2: `mp install --agents=mp-planner` deploys the dedicated
//! planning agent to the harness's agent profile dir. Tests cover
//! opencode + cursor harnesses, the unknown-agent error path, and
//! the agent_dir env-var override.

mod common;

use std::process::Command;

use common::{mp_bin, repo_root, TestEnv};

/// Run `mp install --dev` with isolated harness env. Returns
/// `(output, agents_root)` where agents_root is the harness's
/// `<agent_profile_dir>/..` sibling — i.e. `~/.agents/agents/` for
/// opencode and `~/.cursor/agents/` for cursor.
fn isolated_install_with_agents(
    env: &TestEnv,
    harness_id: &str,
    agent_id: &str,
) -> (std::process::Output, std::path::PathBuf, std::path::PathBuf) {
    let root = repo_root();
    let install_dir = env.tmp.path().join("install-target");
    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();

    // Pre-create the agent dir under the test's harness scratch and
    // pin it via MP_<HARNESS>_AGENT_DIR. The harness resolver honors
    // this override (mirroring the MP_<HARNESS>_SKILL_DIR pattern).
    let agents_root = match harness_id {
        "opencode" => env.tmp.path().join("harness/opencode/agents"),
        "cursor" => env.tmp.path().join("harness/cursor/agents"),
        other => panic!("unsupported harness in test: {other}"),
    };
    std::fs::create_dir_all(&agents_root)
        .unwrap_or_else(|e| panic!("could not pre-create {}: {e}", agents_root.display()));
    let agent_env_key = format!(
        "MP_{}_AGENT_DIR",
        harness_id.to_uppercase().replace('-', "_")
    );

    let mut cmd = Command::new(mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", &install_dir)
        .env(&agent_env_key, &agents_root)
        .arg("--plan-dir")
        .arg(&plan_dir)
        .args([
            "install",
            "--dev",
            "--harness",
            harness_id,
            "--agents",
            agent_id,
            "--format",
            "json",
        ]);
    common::isolated_harness_env(&mut cmd, env.tmp.path());
    let out = cmd.output().expect("spawn mp");

    // Also return the on-disk skill_dir siblings so the test can
    // pin that no unrelated files landed.
    let skills_root = match harness_id {
        "opencode" => env.tmp.path().join("harness/opencode/skills"),
        "cursor" => env.tmp.path().join("harness/cursor/skills"),
        other => panic!("unsupported harness in test: {other}"),
    };

    (out, agents_root, skills_root)
}

/// AC-02: `mp install --agents=mp-planner` deploys the mp-planner
/// agent to the harness's agent profile dir (opencode).
#[test]
fn install_deploys_mp_planner_agent_to_opencode() {
    let env = TestEnv::blank();
    let (out, agents_root, _skills_root) =
        isolated_install_with_agents(&env, "opencode", "mp-planner");
    assert!(
        out.status.success(),
        "install --agents=mp-planner should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dest = agents_root.join("mp-planner.md");
    assert!(
        dest.is_file(),
        "mp-planner.md must land at {}",
        dest.display()
    );
    let content = std::fs::read_to_string(&dest).expect("read mp-planner.md");
    assert!(
        content.contains("name: mp-planner"),
        "agent front-matter must declare name: mp-planner"
    );
    assert!(
        content.contains("mode: subagent") || content.contains("mode: agent"),
        "agent front-matter must declare a mode"
    );
    assert!(
        content.contains("role: planning"),
        "agent metadata must declare role: planning"
    );
    assert!(
        content.contains("Allowed mp commands"),
        "agent body must enumerate the read-only mp command set"
    );
}

/// AC-02 (cursor harness): same contract on cursor.
#[test]
fn install_deploys_mp_planner_agent_to_cursor() {
    let env = TestEnv::blank();
    let (out, agents_root, _skills_root) =
        isolated_install_with_agents(&env, "cursor", "mp-planner");
    assert!(
        out.status.success(),
        "install --agents=mp-planner should succeed for cursor: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dest = agents_root.join("mp-planner.md");
    assert!(
        dest.is_file(),
        "mp-planner.md must land at {}",
        dest.display()
    );
}

/// `mp install --agents=does-not-exist` fails fast with a structured
/// error pointing at the missing template.
#[test]
fn install_unknown_agent_errors_with_template_path() {
    let env = TestEnv::blank();
    let (out, _agents_root, _skills_root) =
        isolated_install_with_agents(&env, "opencode", "does-not-exist");
    assert!(
        !out.status.success(),
        "install --agents=does-not-exist must fail; stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("does-not-exist"),
        "error must name the missing agent id: {combined}"
    );
    assert!(
        combined.contains("templates/harness/opencode/agents/does-not-exist.md"),
        "error must point at the missing template path: {combined}"
    );
}

/// The deploy must NOT touch the skill dir or create a subdirectory
/// for the agent (agents are single files, not directories).
#[test]
fn install_deploys_agent_as_flat_file_no_subdir() {
    let env = TestEnv::blank();
    let (out, agents_root, _skills_root) =
        isolated_install_with_agents(&env, "opencode", "mp-planner");
    assert!(
        out.status.success(),
        "install --agents=mp-planner should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The agent dir exists and contains the flat .md file — NOT a
    // subdirectory named mp-planner.
    assert!(
        agents_root.is_dir(),
        "agents dir must exist at {}",
        agents_root.display()
    );
    assert!(
        !agents_root.join("mp-planner").is_dir(),
        "agent must NOT deploy as a subdirectory; agents are flat .md files"
    );
    let entries: Vec<_> = std::fs::read_dir(&agents_root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["mp-planner.md".to_string()]);
}

/// The agent file in source is byte-identical to the deployed copy.
/// The deploy path is `read → atomic_write`; this pins that the bytes
/// aren't re-encoded, normalized, or otherwise transformed.
#[test]
fn install_preserves_agent_file_bytes() {
    let env = TestEnv::blank();
    let (out, agents_root, _skills_root) =
        isolated_install_with_agents(&env, "opencode", "mp-planner");
    assert!(
        out.status.success(),
        "install --agents=mp-planner should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let src = repo_root().join("templates/harness/opencode/agents/mp-planner.md");
    let dest = agents_root.join("mp-planner.md");
    let src_bytes = std::fs::read(&src).expect("read src");
    let dest_bytes = std::fs::read(&dest).expect("read dest");
    assert_eq!(
        src_bytes, dest_bytes,
        "deployed agent file must be byte-identical to source"
    );
}
