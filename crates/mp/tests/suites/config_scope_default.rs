use crate::common::TestEnv;

#[test]
fn config_set_writes_to_plan_local_config() {
    let env = TestEnv::new();

    let out = env.run(&[
        "config",
        "set",
        "workflow.gates.strictness",
        "full",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "config set failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_path = env.tmp.path().join("master-plan/config.json");
    assert!(
        config_path.is_file(),
        "config.json should exist at the plan dir"
    );
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        content.contains("strictness"),
        "config should contain the set field"
    );
}
