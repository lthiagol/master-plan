//! Install binary deploy parity (mp + raul).

use std::process::Command;

use crate::common::{isolated_harness_env, mp_bin, repo_root, run_with_retry};
use tempfile::TempDir;

fn run_install(install_root: &TempDir, args: &[&str]) -> std::process::Output {
    let source = repo_root().to_string_lossy().to_string();
    let mut all = Vec::from(args);
    all.extend_from_slice(&["--dev", "--source", &source, "--format", "json"]);
    run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", install_root.path());
            isolated_harness_env(&mut cmd, install_root.path());
            cmd.args(&all);
            cmd
        },
        5,
    )
}

#[test]
fn install_deploys_mp_and_raul() {
    let root = TempDir::new().expect("temp");
    let out = run_install(&root, &["install"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        root.path().join("bin/mp").is_file(),
        "mp binary should exist"
    );
    assert!(
        root.path().join("bin/raul").is_file(),
        "raul binary should exist"
    );
}

#[test]
fn uninstall_removes_raul_binary() {
    let root = TempDir::new().expect("temp");
    let _ = run_install(&root, &["install"]);
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
    assert!(!root.path().join("bin/mp").exists());
    assert!(!root.path().join("bin/raul").exists());
}

#[test]
fn make_install_global_deploys_both_binaries() {
    let root = TempDir::new().expect("temp");
    let install_dir = root.path().join("install");
    let out = Command::new("make")
        .current_dir(repo_root())
        .env("INSTALL_DIR", &install_dir)
        .arg("install-global")
        .output()
        .expect("make install-global");
    assert!(
        out.status.success(),
        "make install-global failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        install_dir.join("bin/mp").is_file(),
        "mp binary should exist"
    );
    assert!(
        install_dir.join("bin/raul").is_file(),
        "raul binary should exist"
    );
}
