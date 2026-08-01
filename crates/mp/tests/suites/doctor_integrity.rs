use std::fs;
use std::process::Command;

use crate::common::{mp_bin, repo_root};
use serde_json::Value;
use tempfile::TempDir;

/// Since M29, templates and schemas are embedded in the binary and doctor
/// self-tests that embedded registry. A corrupt or empty template on disk
/// (e.g. under an MP_HOME override) must NOT change the integrity result,
/// because the embedded assets are the source of truth.
#[test]
fn doctor_robust_to_corrupt_disk_templates() {
    let home = setup_doctor_env();
    fs::write(home.path().join("templates/defaults/track.json"), "").expect("truncate");
    fs::write(
        home.path().join("templates/defaults/plan.json"),
        "{{{ not valid json }}",
    )
    .expect("corrupt");
    fs::write(home.path().join("templates/AGENTS-TEMPLATE.md"), "").expect("truncate");

    let json = run_doctor(home.path());
    assert_eq!(
        json["ok"], true,
        "doctor must rely on embedded assets, not disk"
    );
    let checks = json["checks"].as_array().expect("checks array");
    for field in ["track.json", "plan.json", "AGENTS-TEMPLATE"] {
        assert!(
            checks
                .iter()
                .any(|c| c["name"].as_str().unwrap().contains(field)),
            "checks should still list integrity:{field}"
        );
    }
}

/// With no MP_HOME and no assets on disk, doctor still reports embedded
/// templates/schemas as healthy (self-contained binary).
#[test]
fn doctor_green_with_no_disk_assets() {
    let tmp = TempDir::new().expect("temp");
    let json = run_doctor_env(tmp.path(), true);
    assert_eq!(json["ok"], true, "doctor ok with only embedded assets");
    assert_eq!(json["templates"], true);
    assert_eq!(json["schemas"], true);
}

/// mp doctor on the repo root (full disk tree present) returns ok:true.
#[test]
fn doctor_passes_on_valid_tree() {
    let json = run_doctor(&repo_root());
    assert_eq!(json["ok"], true, "doctor should pass on valid tree");
}

fn setup_doctor_env() -> TempDir {
    let tmp = TempDir::new().expect("temp");
    let src_templates = repo_root().join("templates");
    let dst_templates = tmp.path().join("templates");
    copy_recursive(&src_templates, &dst_templates);

    let src_schemas = repo_root().join("schemas");
    let dst_schemas = tmp.path().join("schemas");
    copy_recursive(&src_schemas, &dst_schemas);
    tmp
}

fn run_doctor(home: &std::path::Path) -> Value {
    run_doctor_env(home, true)
}

fn run_doctor_env(home: &std::path::Path, set_mp_home: bool) -> Value {
    let install_stub = home.join("_install_stub");
    fs::create_dir_all(&install_stub).ok();

    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_INSTALL_DIR", &install_stub);
    if set_mp_home {
        cmd.env("MP_HOME", home);
    } else {
        // Point HOME somewhere harmless so dirs_home() resolution doesn't
        // accidentally find a real ~/.agents/master-plan; the binary should
        // still work via embedded assets.
        cmd.env("HOME", home);
        cmd.env_remove("MP_HOME");
        cmd.env_remove("MPH_HOME");
    }
    cmd.args(["doctor", "--format", "json"]);
    let out = cmd.output().expect("doctor");
    assert!(
        out.status.success(),
        "doctor command should exit 0 (ok field governs pass/fail): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("doctor json")
}

fn copy_recursive(src: &std::path::Path, dst: &std::path::Path) {
    if src.is_dir() {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            copy_recursive(&entry.path(), &dst.join(entry.file_name()));
        }
    } else {
        fs::copy(src, dst).unwrap();
    }
}
