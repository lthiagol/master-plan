use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn plan_relocate_renames_dir_and_updates_location() {
    let env = TestEnv::blank();
    let init = lib_api::run(
        &env,
        &[
            "init",
            "--profile",
            "full",
            "--plan-dir",
            ".mp",
            "--format",
            "json",
        ],
    );
    assert!(init.status.success());

    // Verify .mp/ exists and master-plan/ does not
    assert!(env.tmp.path().join(".mp").is_dir());
    assert!(!env.tmp.path().join("master-plan").is_dir());

    // Relocate .mp → master-plan
    let relocate = lib_api::run(
        &env,
        &["plan", "relocate", ".mp", "master-plan", "--format", "json"],
    );
    assert!(
        relocate.status.success(),
        "relocate failed: {}",
        String::from_utf8_lossy(&relocate.stderr)
    );

    // Now master-plan/ should exist with a config.json
    let new_config = env.tmp.path().join("master-plan/config.json");
    assert!(
        new_config.is_file(),
        "config.json should be at new location"
    );
    let content = std::fs::read_to_string(&new_config).unwrap();
    assert!(
        content.contains("\"location\": \"master-plan\""),
        "config should have updated location"
    );

    // Old location should not exist
    assert!(!env.tmp.path().join(".mp").is_dir());

    // doctor should be green (MP_HOME and project root need pointing)
    //
    // CI runners ship without `herdr` on PATH; doctor gates
    // `report.ok` on the herdr shape check, so a bare doctor call
    // would exit non-zero under CI even though the plan itself is
    // healthy. Stub a herdr that satisfies the `which_herdr` +
    // `agent start --help` / `pane split --help` shape probes so
    // the test stays self-contained.
    let path = crate::common::fake_herdr::install_fake_herdr_for_doctor(&env);
    let doctor = env.run_with_env(&[("PATH", &path)], &["doctor", "--format", "json"]);
    assert!(
        doctor.status.success(),
        "doctor should pass after relocate: {}",
        String::from_utf8_lossy(&doctor.stderr)
    );
}

#[test]
fn plan_relocate_fails_if_target_exists() {
    let env = TestEnv::new();
    // master-plan/ already exists from TestEnv::new()
    let relocate = lib_api::run(
        &env,
        &[
            "plan",
            "relocate",
            "master-plan",
            "master-plan",
            "--format",
            "json",
        ],
    );
    assert!(
        !relocate.status.success(),
        "relocating to existing dir should fail"
    );
}

#[test]
fn plan_relocate_fails_if_source_missing() {
    let env = TestEnv::blank();
    lib_api::run(&env, &["init", "--profile", "full", "--format", "json"]);
    let relocate = lib_api::run(
        &env,
        &[
            "plan",
            "relocate",
            "nonexistent",
            "other",
            "--format",
            "json",
        ],
    );
    assert!(
        !relocate.status.success(),
        "relocating non-existent dir should fail"
    );
}
