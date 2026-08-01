use crate::common::TestEnv;

#[test]
fn goals_add_appends() {
    let env = TestEnv::new();
    let json = env.run_json(&["plan", "goals", "add", "First goal", "--format", "json"]);
    let goals = json["goals"].as_array().unwrap();
    assert!(goals.iter().any(|g| g.as_str() == Some("First goal")));
}

#[test]
fn goals_remove_by_text() {
    let env = TestEnv::new();
    env.run_json(&["plan", "goals", "add", "Goal to remove", "--format", "json"]);
    env.run_json(&["plan", "goals", "add", "Keep me", "--format", "json"]);
    let json = env.run_json(&[
        "plan",
        "goals",
        "remove",
        "Goal to remove",
        "--format",
        "json",
    ]);
    let goals: Vec<&str> = json["goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    assert!(!goals.contains(&"Goal to remove"));
    assert!(goals.contains(&"Keep me"));
}

#[test]
fn goals_remove_by_index() {
    let env = TestEnv::new();
    env.run_json(&["plan", "goals", "add", "First", "--format", "json"]);
    env.run_json(&["plan", "goals", "add", "Second", "--format", "json"]);
    let json = env.run_json(&["plan", "goals", "remove", "1", "--format", "json"]);
    let goals: Vec<&str> = json["goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    assert!(!goals.contains(&"First"));
    assert!(goals.contains(&"Second"));
}

#[test]
fn goals_remove_nonexistent_errors() {
    let env = TestEnv::new();
    let out = env.run(&["plan", "goals", "remove", "nonexistent", "--format", "json"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not found") || stderr.contains("nonexistent"));
}

#[test]
fn goals_remove_index_out_of_range_errors() {
    let env = TestEnv::new();
    let out = env.run(&["plan", "goals", "remove", "5", "--format", "json"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("out of range"));
}

#[test]
fn goals_set_replaces_all() {
    let env = TestEnv::new();
    env.run_json(&["plan", "goals", "add", "Stale", "--format", "json"]);
    let json = env.run_json(&[
        "plan",
        "goals",
        "set",
        r#"["New A", "New B"]"#,
        "--format",
        "json",
    ]);
    let goals: Vec<&str> = json["goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    assert_eq!(goals, vec!["New A", "New B"]);
}

#[test]
fn nongoals_remove_by_text() {
    let env = TestEnv::new();
    env.run_json(&["plan", "nongoals", "add", "Non goal X", "--format", "json"]);
    let json = env.run_json(&[
        "plan",
        "nongoals",
        "remove",
        "Non goal X",
        "--format",
        "json",
    ]);
    let list: Vec<&str> = json["non_goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    assert!(!list.contains(&"Non goal X"));
}

#[test]
fn nongoals_set_replaces_all() {
    let env = TestEnv::new();
    env.run_json(&["plan", "nongoals", "add", "Old", "--format", "json"]);
    let json = env.run_json(&[
        "plan",
        "nongoals",
        "set",
        r#"["New only"]"#,
        "--format",
        "json",
    ]);
    let list: Vec<&str> = json["non_goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    assert_eq!(list, vec!["New only"]);
}

#[test]
fn principles_remove_by_text() {
    let env = TestEnv::new();
    env.run_json(&["plan", "principles", "add", "Be good", "--format", "json"]);
    let json = env.run_json(&[
        "plan",
        "principles",
        "remove",
        "Be good",
        "--format",
        "json",
    ]);
    let list: Vec<&str> = json["principles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    assert!(!list.contains(&"Be good"));
}

#[test]
fn principles_set_replaces_all() {
    let env = TestEnv::new();
    env.run_json(&[
        "plan",
        "principles",
        "add",
        "Old principle",
        "--format",
        "json",
    ]);
    let json = env.run_json(&[
        "plan",
        "principles",
        "set",
        r#"["New principle"]"#,
        "--format",
        "json",
    ]);
    let list: Vec<&str> = json["principles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    assert_eq!(list, vec!["New principle"]);
}

#[test]
fn principles_remove_nonexistent_errors() {
    let env = TestEnv::new();
    let out = env.run(&[
        "plan",
        "principles",
        "remove",
        "does not exist",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not found"));
}
