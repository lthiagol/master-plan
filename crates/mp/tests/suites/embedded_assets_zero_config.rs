//! M29: the binary is self-contained. With no `MP_HOME`, no `CARGO_MANIFEST_DIR`
//! crutch, and no templates/schemas on disk, core commands must still work,
//! served entirely from embedded assets.

use std::process::Command;

use crate::common::mp_bin;
use tempfile::TempDir;

fn clean_mp(tmp: &TempDir) -> Command {
    let mut cmd = Command::new(mp_bin());
    cmd.env("HOME", tmp.path());
    cmd.env_remove("MP_HOME");
    cmd.env_remove("MPH_HOME");
    cmd.env_remove("CARGO_MANIFEST_DIR");
    cmd
}

#[test]
fn zero_config_doctor_init_validate() {
    let tmp = TempDir::new().expect("temp");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    let doctor = clean_mp(&tmp)
        .args(["doctor", "--format", "json"])
        .output()
        .expect("doctor");
    assert!(doctor.status.success(), "mp doctor should exit 0");
    let doc: serde_json::Value = serde_json::from_slice(&doctor.stdout).expect("doctor json");
    assert_eq!(doc["ok"], true, "doctor ok via embedded assets");
    assert_eq!(doc["templates"], true);
    assert_eq!(doc["schemas"], true);

    let init = clean_mp(&tmp)
        .args([
            "init",
            "--project-root",
            project.to_str().unwrap(),
            "--profile",
            "full",
            "--quiet",
        ])
        .output()
        .expect("init");
    assert!(
        init.status.success(),
        "mp init should succeed with only embedded assets: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(
        project.join("master-plan/plan.json").is_file(),
        "init should create a plan from embedded defaults"
    );

    let validate = clean_mp(&tmp)
        .args([
            "validate",
            "--project-root",
            project.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("validate");
    assert!(validate.status.success(), "mp validate should exit 0");
    let val: serde_json::Value = serde_json::from_slice(&validate.stdout).expect("validate json");
    assert_eq!(val["ok"], true, "validate ok on freshly-initialized plan");
}

/// AC-04: when `MP_HOME` is set, a file on disk there overrides the embedded
/// copy. We plant a uniquely-marked `AGENTS-TEMPLATE.md` under MP_HOME (written
/// verbatim by `mp init`, so the marker survives) and confirm it wins over the
/// embedded default.
#[test]
fn mp_home_overrides_embedded_assets() {
    let tmp = TempDir::new().expect("temp");
    let toolkit = tmp.path().join("toolkit");
    std::fs::create_dir_all(toolkit.join("templates")).unwrap();
    std::fs::write(
        toolkit.join("templates/AGENTS-TEMPLATE.md"),
        "# MP_HOME-OVERRIDE-MARKER\n",
    )
    .unwrap();

    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    let init = clean_mp(&tmp)
        .env("MP_HOME", toolkit)
        .args([
            "init",
            "--project-root",
            project.to_str().unwrap(),
            "--profile",
            "full",
            "--quiet",
        ])
        .output()
        .expect("init");
    assert!(
        init.status.success(),
        "mp init with MP_HOME override should succeed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let written =
        std::fs::read_to_string(project.join("master-plan/AGENTS.md")).expect("AGENTS.md written");
    assert!(
        written.contains("MP_HOME-OVERRIDE-MARKER"),
        "MP_HOME AGENTS-TEMPLATE.md should override the embedded default"
    );
}
