//! mp install env.sh snippet for agent shells.

use std::process::Command;

use crate::common::{isolated_harness_env, mp_bin, repo_root, run_with_retry};
use tempfile::TempDir;

fn run_install(root: &TempDir) -> std::process::Output {
    let source = repo_root().to_string_lossy().to_string();
    run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", root.path());
            isolated_harness_env(&mut cmd, root.path());
            cmd.args(["install", "--dev", "--source", &source, "--format", "json"]);
            cmd
        },
        5,
    )
}

#[test]
fn install_writes_env_sh() {
    let root = TempDir::new().expect("temp");
    let out = run_install(&root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let env_sh = root.path().join("env.sh");
    assert!(env_sh.is_file(), "env.sh should exist");
    let content = std::fs::read_to_string(&env_sh).expect("read env.sh");
    assert!(content.contains("export MP_HOME="));
    assert!(content.contains("/bin':\"$PATH\""));
}

#[test]
fn uninstall_removes_env_sh() {
    let root = TempDir::new().expect("temp");
    let _ = run_install(&root);

    let install_root = root.path().to_path_buf();
    let out = run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", &install_root)
                .args(["uninstall", "--harness", "both", "--format", "json"]);
            isolated_harness_env(&mut cmd, root.path());
            cmd
        },
        5,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!root.path().join("env.sh").exists());
}
