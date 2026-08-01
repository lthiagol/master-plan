//! make install vs mp install parity (M74).

use std::io::Write;
use std::process::Command;

use crate::common::{isolated_harness_env, mp_bin, repo_root};
use tempfile::TempDir;

const V1_HARNESSES: &str = "opencode,cursor,pi";

fn run_mp_install(
    install_root: &TempDir,
    extra_args: &[&str],
    path: Option<&str>,
) -> std::process::Output {
    let source = repo_root().to_string_lossy().to_string();
    let mut args = vec![
        "install",
        "--dev",
        "--source",
        &source,
        "--harness",
        V1_HARNESSES,
        "--format",
        "json",
    ];
    args.extend_from_slice(extra_args);
    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", install_root.path());
    if let Some(path) = path {
        cmd.env("PATH", path);
    }
    isolated_harness_env(&mut cmd, install_root.path());
    cmd.args(&args).output().expect("mp install")
}

fn run_make(install_root: &TempDir, target: &str) -> std::process::Output {
    let mut cmd = Command::new("make");
    cmd.current_dir(repo_root())
        .env("INSTALL_DIR", install_root.path())
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", install_root.path());
    isolated_harness_env(&mut cmd, install_root.path());
    cmd.arg(target).output().expect("make")
}

fn assert_v1_tree(root: &TempDir) {
    let base = root.path();
    assert!(base.join("bin/mp").is_file(), "mp binary");
    assert!(base.join("bin/raul").is_file(), "raul binary");
    assert!(base.join("env.sh").is_file(), "env.sh");
    // The default CPD skill set installs three base skills per harness.
    for skill in ["mp-flow", "mp-runner", "mp-coordinator"] {
        assert!(
            base.join(format!("harness/opencode/skills/{skill}/SKILL.md"))
                .is_file(),
            "opencode {skill}"
        );
        assert!(
            base.join(format!("harness/cursor/skills/{skill}/SKILL.md"))
                .is_file(),
            "cursor {skill}"
        );
        assert!(
            base.join(format!("harness/pi/agent/skills/{skill}/SKILL.md"))
                .is_file(),
            "pi {skill}"
        );
    }
    assert!(
        base.join("harness/pi/agent/AGENTS.md").is_file(),
        "pi AGENTS.md convention"
    );
    assert!(
        !base
            .join("harness/pi/agent/skills/spec-grill/SKILL.md")
            .is_file(),
        "pi spec-grill must NOT install by default (AC-05)"
    );
}

#[test]
fn mp_install_v1_harnesses_deploy_full_tree() {
    let root = TempDir::new().expect("temp");
    let out = run_mp_install(&root, &[], None);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_v1_tree(&root);
}

#[test]
fn make_install_matches_mp_install_layout() {
    let root = TempDir::new().expect("temp");
    let out = run_make(&root, "install");
    assert!(
        out.status.success(),
        "make install failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_v1_tree(&root);
}

#[test]
fn make_install_global_toolkit_only() {
    let root = TempDir::new().expect("temp");
    let out = run_make(&root, "install-global");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(root.path().join("bin/mp").is_file());
    assert!(root.path().join("env.sh").is_file());
    assert!(
        !root
            .path()
            .join("harness/opencode/skills/mp-flow/SKILL.md")
            .exists(),
        "install-global should not deploy skills"
    );
}

#[test]
fn make_uninstall_purge_removes_install() {
    let root = TempDir::new().expect("temp");
    let install = run_make(&root, "install");
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let out = run_make(&root, "uninstall");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!root.path().join("bin/mp").exists());
    assert!(!root.path().join("env.sh").exists());
}

#[test]
fn install_summary_shows_env_sh_remediation() {
    let root = TempDir::new().expect("temp");
    let install = run_mp_install(&root, &[], Some("/usr/bin:/bin"));
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );

    let mut child = Command::new("bash")
        .arg(repo_root().join("scripts/install-summary.sh"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&install.stdout)
        .expect("write stdin");
    let out = child.wait_with_output().expect("summary output");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("env.sh"),
        "summary should mention env.sh when PATH stripped: {stdout}"
    );
    assert!(
        !stdout.contains("Shell snippet:") || stdout.contains("→"),
        "should use doctor PATH branch, not only raw snippet: {stdout}"
    );
}
