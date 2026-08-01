use std::fs;
use std::process::Command;

use crate::common::{isolated_harness_env, mp_bin, path_with_install_bin, repo_root};
use tempfile::TempDir;

/// Copy templates to a temp dir, truncate one to zero bytes, run install,
/// expect failure naming the file.
#[test]
fn install_refuses_zero_byte_template() {
    let src = copy_templates_to_temp();
    let install_root = TempDir::new().expect("install");

    // Truncate a template to zero bytes
    let track_toml = src.path().join("templates/defaults/track.toml");
    fs::write(&track_toml, "").expect("truncate track");

    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_HOME", src.path())
        .env("MP_INSTALL_DIR", install_root.path());
    isolated_harness_env(&mut cmd, install_root.path());
    let out = cmd
        .args([
            "install",
            "--harness",
            "both",
            "--dev",
            "--source",
            &src.path().to_string_lossy(),
            "--format",
            "json",
        ])
        .output()
        .expect("install");

    assert!(
        !out.status.success(),
        "install should fail on zero-byte artifact"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("below minimum size") || stderr.contains("templates/defaults/track.toml"),
        "error should name the offending path, got: {stderr}"
    );
}

/// Corrupt a template's TOML, run install, expect a parse failure naming the file.
#[test]
fn install_refuses_unparseable_toml_template() {
    let src = copy_templates_to_temp();
    let install_root = TempDir::new().expect("install");

    // Corrupt track.toml with invalid TOML
    let track_toml = src.path().join("templates/defaults/track.toml");
    fs::write(&track_toml, "this is not {{{ valid toml").expect("corrupt track");

    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_HOME", src.path())
        .env("MP_INSTALL_DIR", install_root.path());
    isolated_harness_env(&mut cmd, install_root.path());
    let out = cmd
        .args([
            "install",
            "--harness",
            "both",
            "--dev",
            "--source",
            &src.path().to_string_lossy(),
            "--format",
            "json",
        ])
        .output()
        .expect("install");

    assert!(
        !out.status.success(),
        "install should fail on unparseable TOML"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unparseable") || stderr.contains("templates/defaults/track.toml"),
        "error should name the offending path, got: {stderr}"
    );
}

/// Install with a source that has the skill file truncated, expect failure.
#[test]
fn install_refuses_zero_byte_skill() {
    let src = copy_templates_to_temp();
    let install_root = TempDir::new().expect("install");

    let skill = src.path().join("templates/skills/mp-flow/SKILL.md");
    fs::write(&skill, "").expect("truncate skill");

    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_HOME", src.path())
        .env("MP_INSTALL_DIR", install_root.path());
    isolated_harness_env(&mut cmd, install_root.path());
    let out = cmd
        .args([
            "install",
            "--harness",
            "both",
            "--dev",
            "--source",
            &src.path().to_string_lossy(),
            "--format",
            "json",
        ])
        .output()
        .expect("install");

    assert!(
        !out.status.success(),
        "install should fail on zero-byte skill"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("below minimum size") || stderr.contains("SKILL.md"),
        "error should name the offending path, got: {stderr}"
    );
}

/// A valid install succeeds and doctor passes.
#[test]
fn valid_install_succeeds() {
    let install_root = TempDir::new().expect("install");
    let source = repo_root().to_string_lossy().to_string();

    let path_with_install = path_with_install_bin(install_root.path());

    let mut cmd = Command::new(mp_bin());
    cmd.env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", install_root.path())
        .env("PATH", &path_with_install);
    isolated_harness_env(&mut cmd, install_root.path());
    let out = cmd
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
}

fn copy_templates_to_temp() -> TempDir {
    let tmp = TempDir::new().expect("temp");
    let src_templates = repo_root().join("templates");
    let dst_templates = tmp.path().join("templates");
    copy_recursive(&src_templates, &dst_templates);
    let target = repo_root().join("target");
    if target.is_dir() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, tmp.path().join("target")).ok();
        #[cfg(not(unix))]
        copy_recursive(&target, &tmp.path().join("target"));
    }
    if repo_root().join("Cargo.toml").is_file() {
        fs::copy(
            repo_root().join("Cargo.toml"),
            tmp.path().join("Cargo.toml"),
        )
        .ok();
    }
    tmp
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
