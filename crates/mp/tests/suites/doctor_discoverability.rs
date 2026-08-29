//! Doctor runtime PATH / install-layout discoverability checks.

use std::process::Command;

use crate::common::{isolated_harness_env, mp_bin, repo_root};
use tempfile::TempDir;

fn run_install(install_root: &TempDir) -> std::process::Output {
    let source = repo_root().to_string_lossy().to_string();
    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", install_root.path());
    isolated_harness_env(&mut cmd, install_root.path());
    cmd.args(["install", "--dev", "--source", &source, "--format", "json"])
        .output()
        .expect("install")
}

#[test]
fn doctor_reports_mp_missing_when_path_stripped() {
    let root = TempDir::new().expect("temp");
    let install = run_install(&root);
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );

    let out = Command::new(mp_bin())
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", root.path())
        .env("PATH", "/usr/bin:/bin")
        .args(["doctor", "--format", "json"])
        .output()
        .expect("doctor");
    // M197 F-11 / AC-02: `mp doctor` now exits non-zero when the
    // report is red so shell pipelines and CI detect the failure.
    // The earlier behavior silently returned Ok(()) regardless of
    // check status, which made this test (which intentionally
    // strips PATH) pass on the wrong contract.
    assert!(
        !out.status.success(),
        "doctor must exit non-zero when checks fail: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["ok"],
        serde_json::Value::Bool(false),
        "doctor report ok field must be false when checks fail"
    );
    let checks = json["checks"].as_array().expect("checks");
    let mp_check = checks
        .iter()
        .find(|c| c["name"] == "runtime:mp_on_path")
        .expect("runtime:mp_on_path");
    assert_eq!(mp_check["ok"], false);
    assert!(
        mp_check["message"].as_str().unwrap().contains("env.sh"),
        "should point at env.sh remediation"
    );
}

#[test]
fn doctor_ok_when_install_bin_on_path() {
    let root = TempDir::new().expect("temp");
    let install = run_install(&root);
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );

    let bin_dir = root.path().join("bin");
    let path = format!("{}:/usr/bin:/bin", bin_dir.display());
    let out = Command::new(mp_bin())
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", root.path())
        .env("PATH", &path)
        .args(["doctor", "--format", "json"])
        .output()
        .expect("doctor");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let checks = json["checks"].as_array().expect("checks");
    let mp_check = checks
        .iter()
        .find(|c| c["name"] == "runtime:mp_on_path")
        .expect("runtime:mp_on_path");
    assert_eq!(mp_check["ok"], true);
    let raul_check = checks
        .iter()
        .find(|c| c["name"] == "runtime:raul_binary")
        .expect("runtime:raul_binary");
    assert_eq!(raul_check["ok"], true);
}

#[test]
fn install_report_includes_runtime_path_check() {
    let root = TempDir::new().expect("temp");
    let source = repo_root().to_string_lossy().to_string();
    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", root.path())
        .env("PATH", "/usr/bin:/bin");
    isolated_harness_env(&mut cmd, root.path());
    let install = cmd
        .args(["install", "--dev", "--source", &source, "--format", "json"])
        .output()
        .expect("install");
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&install.stdout).unwrap();
    let checks = json["doctor"]["checks"].as_array().expect("doctor checks");
    let mp_check = checks
        .iter()
        .find(|c| c["name"] == "runtime:mp_on_path")
        .expect("install report should include runtime:mp_on_path");
    assert_eq!(mp_check["ok"], false);
    assert!(
        mp_check["message"].as_str().unwrap().contains("env.sh"),
        "should point at env.sh"
    );
}

#[test]
fn install_pi_harness_doctor_reports_artifacts() {
    let root = TempDir::new().expect("temp");
    let source = repo_root().to_string_lossy().to_string();
    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", root.path());
    isolated_harness_env(&mut cmd, root.path());
    let out = cmd
        .args([
            "install",
            "--dev",
            "--source",
            &source,
            "--harness",
            "pi",
            "--skills",
            "mp-flow,mp-runner,mp-coordinator,spec-grill",
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
    let harnesses = json["doctor"]["harnesses"].as_array().expect("harnesses");
    let pi = harnesses
        .iter()
        .find(|h| h["id"] == "pi")
        .expect("pi harness entry");
    assert_eq!(pi["skill_installed"], true);
    assert_eq!(pi["spec_grill_installed"], true);
    assert_eq!(pi["convention_file_installed"], true);
}
