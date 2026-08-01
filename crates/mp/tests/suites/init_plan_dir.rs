use crate::common::TestEnv;

fn config_location(env: &TestEnv) -> String {
    let plan_dir = env.tmp.path().join("master-plan");
    let config_path = plan_dir.join("config.json");
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
    v["workflow"]["plan"]["location"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

#[test]
fn default_init_uses_master_plan() {
    let env = TestEnv::new();
    assert_eq!(config_location(&env), "master-plan");
}

#[test]
fn init_with_plan_dir() {
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
    assert!(
        init.status.success(),
        "init with --plan-dir .mp failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    // Config should have location = ".mp"
    let config_path = env.tmp.path().join(".mp/config.json");
    assert!(
        config_path.is_file(),
        "config.json should be at .mp/config.json"
    );
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        content.contains("\"location\": \".mp\""),
        "config should have location=\".mp\", got:\n{content}"
    );
}

#[test]
fn init_with_plan_dir_custom_name() {
    let env = TestEnv::blank();
    let init = env.run(&[
        "init",
        "--profile",
        "full",
        "--plan-dir",
        "my-plan",
        "--format",
        "json",
    ]);
    assert!(
        init.status.success(),
        "init with --plan-dir my-plan failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let config_path = env.tmp.path().join("my-plan/config.json");
    assert!(
        config_path.is_file(),
        "config.json should be at my-plan/config.json"
    );
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        content.contains("\"location\": \"my-plan\""),
        "config should have location=\"my-plan\", got:\n{content}"
    );
}

#[test]
fn hybrid_init_with_plan_dir() {
    let env = TestEnv::blank();
    let init = env.run(&[
        "init",
        "--profile",
        "hybrid",
        "--plan-dir",
        ".mp",
        "--format",
        "json",
    ]);
    assert!(
        init.status.success(),
        "hybrid init with --plan-dir .mp failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let config_path = env.tmp.path().join(".mp/config.json");
    assert!(
        config_path.is_file(),
        "config.json should be at .mp/config.json"
    );
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        content.contains("\"location\": \".mp\""),
        "hybrid config should have location=\".mp\", got:\n{content}"
    );
    assert!(env.tmp.path().join(".mp/ideas.json").is_file());
    assert!(!env.tmp.path().join(".mp/brief.json").exists());
}
