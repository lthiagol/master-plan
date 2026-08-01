use crate::common::TestEnv;

#[test]
fn init_has_empty_backlog() {
    let env = TestEnv::new();
    let out = env.run(&["list", "backlog", "--format", "json"]);
    assert!(
        out.status.success(),
        "list backlog failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = json["backlog"].as_array().unwrap();
    assert!(
        items.is_empty(),
        "expected empty backlog after fresh init, got {} items: {:?}",
        items.len(),
        items
    );
}

#[test]
fn init_has_empty_decisions() {
    let env = TestEnv::new();
    let out = env.run(&["decision", "list", "--format", "json"]);
    assert!(
        out.status.success(),
        "decision list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let decisions = json["decisions"].as_array().unwrap();
    assert!(
        decisions.is_empty(),
        "expected empty decisions after fresh init, got {} items: {:?}",
        decisions.len(),
        decisions
    );
}

#[test]
fn init_has_empty_ideas() {
    let env = TestEnv::new();
    let out = env.run(&["idea", "list", "--format", "json"]);
    assert!(
        out.status.success(),
        "idea list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ideas = json["ideas"].as_array().unwrap();
    assert!(
        ideas.is_empty(),
        "expected empty ideas after fresh init, got {} items: {:?}",
        ideas.len(),
        ideas
    );
}
