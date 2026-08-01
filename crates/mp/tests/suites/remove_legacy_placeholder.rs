use crate::common::TestEnv;

fn write_decisions_json(env: &TestEnv, value: serde_json::Value) {
    let dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&dir).unwrap();
    let json = serde_json::to_string_pretty(&value).unwrap();
    std::fs::write(dir.join("decisions.json"), format!("{json}\n")).unwrap();
}

fn write_ideas_json(env: &TestEnv, value: serde_json::Value) {
    let dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&dir).unwrap();
    let json = serde_json::to_string_pretty(&value).unwrap();
    std::fs::write(dir.join("ideas.json"), format!("{json}\n")).unwrap();
}

#[test]
fn decision_remove_legacy_placeholder() {
    // Manually create a decisions.json with legacy id='' placeholder row
    let env = TestEnv::blank();
    env.run(&["init", "--profile", "full", "--format", "json"]);
    write_decisions_json(
        &env,
        serde_json::json!({
            "decisions": [{
                "id": "",
                "date": "",
                "summary": "",
                "context": "",
                "milestone": "",
            }]
        }),
    );

    // Remove the placeholder via CLI
    let remove = env.run(&["decision", "remove", "", "--format", "json"]);
    assert!(
        remove.status.success(),
        "remove legacy placeholder failed: {}",
        String::from_utf8_lossy(&remove.stderr)
    );

    // List should be empty
    let list = env.run(&["decision", "list", "--format", "json"]);
    assert!(list.status.success());
    let json: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert!(
        json["decisions"].as_array().unwrap().is_empty(),
        "decisions should be empty after removing placeholder"
    );
}

#[test]
fn idea_remove_legacy_placeholder() {
    let env = TestEnv::blank();
    env.run(&["init", "--profile", "full", "--format", "json"]);
    write_ideas_json(
        &env,
        serde_json::json!({
            "ideas": [{
                "id": "",
                "title": "",
                "body": "",
                "status": "open",
                "tags": [],
                "source": "conversation",
                "created": "",
                "promoted_to": "",
            }]
        }),
    );

    let remove = env.run(&["idea", "remove", "", "--format", "json"]);
    assert!(
        remove.status.success(),
        "remove legacy idea placeholder failed: {}",
        String::from_utf8_lossy(&remove.stderr)
    );

    let list = env.run(&["idea", "list", "--format", "json"]);
    assert!(list.status.success());
    let json: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert!(
        json["ideas"].as_array().unwrap().is_empty(),
        "ideas should be empty after removing placeholder"
    );
}
