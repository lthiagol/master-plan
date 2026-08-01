use crate::common::TestEnv;

#[test]
fn config_set_with_plan_dir_writes_only_inside_plan_dir() {
    let env = TestEnv::blank();
    let init = env.run(&[
        "init",
        "--profile",
        "full",
        "--plan-dir",
        ".mp",
        "--format",
        "json",
    ]);
    assert!(init.status.success());

    let set = env.run(&[
        "config",
        "set",
        "workflow.gates.strictness",
        "relaxed",
        "--plan-dir",
        ".mp",
        "--format",
        "json",
    ]);
    assert!(
        set.status.success(),
        "config set failed: {}",
        String::from_utf8_lossy(&set.stderr)
    );

    let config_path = env.tmp.path().join(".mp/config.json");
    assert!(
        config_path.is_file(),
        "config.json should be at .mp/config.json"
    );

    let master_plan_path = env.tmp.path().join("master-plan");
    assert!(
        !master_plan_path.is_dir(),
        "master-plan/ should not exist as a side effect"
    );
}
