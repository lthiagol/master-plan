use crate::common::TestEnv;

#[test]
fn harness_by_id_opencode() {
    let env = TestEnv::new();
    let out = env.run(&[
        "install",
        "--print-paths",
        "--harness",
        "opencode",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0]["id"], "opencode");
}

#[test]
fn harness_by_id_cursor() {
    let env = TestEnv::new();
    let out = env.run(&[
        "install",
        "--print-paths",
        "--harness",
        "cursor",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(paths[0]["id"], "cursor");
    assert!(paths[0]["skill_dir"].as_str().unwrap().contains("cursor"));
}

#[test]
fn print_paths_default_returns_one() {
    let env = TestEnv::new();
    let out = env.run(&["install", "--print-paths", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 1, "default should return 1 harness (opencode)");
    assert_eq!(paths[0]["id"], "opencode");
}

#[test]
fn print_paths_both_returns_all() {
    let env = TestEnv::new();
    let out = env.run(&[
        "install",
        "--print-paths",
        "--harness",
        "both",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 8, "both should return all 8 harnesses");
}

#[test]
fn unknown_harness_errors() {
    let env = TestEnv::new();
    let out = env.run(&[
        "install",
        "--print-paths",
        "--harness",
        "nonexistent",
        "--format",
        "json",
    ]);
    assert!(!out.status.success(), "should error on unknown harness");
}

#[test]
fn print_paths_pi_uses_shared_skill_dir_and_agent_instructions() {
    let env = TestEnv::new();
    let out = env.run(&[
        "install",
        "--print-paths",
        "--harness",
        "pi",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths = json["paths"].as_array().unwrap();
    assert_eq!(paths[0]["id"], "pi");
    let skill_dir = paths[0]["skill_dir"].as_str().unwrap();
    assert!(
        skill_dir.contains(".agents/skills"),
        "pi should install Master Plan skills to the shared .agents/skills path to avoid Pi startup collisions: {skill_dir}"
    );
    let convention = paths[0]["convention_file"].as_str().unwrap();
    assert!(
        convention.ends_with(".pi/agent/AGENTS.md"),
        "pi convention should be ~/.pi/agent/AGENTS.md: {convention}"
    );
    let profile_dir = paths[0]["profile_dir"].as_str().unwrap();
    assert!(
        profile_dir.ends_with(".pi/agent"),
        "pi profile_dir should be the Pi agent root: {profile_dir}"
    );
    assert_eq!(
        paths[0]["project_skill_dir"].as_str().unwrap(),
        ".pi/skills"
    );
}

#[test]
fn harness_descriptor_has_required_fields() {
    let env = TestEnv::new();
    let out = env.run(&[
        "install",
        "--print-paths",
        "--harness",
        "opencode",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let h = &json["paths"][0];
    assert!(h["id"].is_string());
    assert!(h["display_name"].is_string());
    assert!(h["convention_file"].is_string());
    assert!(h["skill_dir"].is_string());
    assert!(h["profile_dir"].is_string());
}
