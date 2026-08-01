use crate::common::TestEnv;

fn assert_valid_json(stdout: &[u8], key: &str) {
    let json: serde_json::Value = serde_json::from_slice(stdout).expect("valid JSON");
    assert!(json.get(key).is_some(), "JSON should contain {key}: {json}");
}

/// Core reads emit valid JSON without --format (default contract).
#[test]
fn status_default_json() {
    let env = TestEnv::new();
    let out = env.run(&["status"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "planning_status");
}

#[test]
fn next_default_json() {
    let env = TestEnv::new();
    let out = env.run(&["next"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
}

#[test]
fn list_milestones_default_json() {
    let env = TestEnv::new();
    let out = env.run(&["list", "milestones"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "milestones");
}

#[test]
fn validate_default_json() {
    let env = TestEnv::new();
    let out = env.run(&["validate"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "ok");
}

/// Explicit --format json remains valid (backward compatible).
#[test]
fn list_milestones_format_json() {
    let env = TestEnv::new();
    let out = env.run(&["list", "milestones", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "milestones");
}

/// M144 regression pin: every milestone item carries `lifecycle` AND
/// `lifecycle_at` (the latter may be `null` for pre-M144 milestones).
/// Without this test, the emitter could silently drop a field without
/// any failing assertion (the contract was already broken before this
/// test was added).
#[test]
fn list_milestones_emits_lifecycle_and_lifecycle_at() {
    let env = TestEnv::new();
    // Build a milestone through the CLI so we exercise the real write
    // path that sets lifecycle_at. The create payload includes the
    // minimum required fields (intent.outcome + scope.out_of_scope
    // >= 2) to satisfy the schema gate.
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"id":"01","title":"verify-m144","slug":"verify-m144","effort":"S","risk":"low","intent":{"outcome":"outcome"},"problem":{"description":"d"},"scope":{"in_scope":["x"],"out_of_scope":["a","b"]}}"#,
    ]);
    assert!(
        create.status.success(),
        "milestone create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );

    let out = env.run(&["list", "milestones", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid JSON from mp list milestones");
    let items = json["milestones"]
        .as_array()
        .expect("milestones must be an array");
    assert!(!items.is_empty(), "fixture must contain a milestone");
    let item = &items[0];
    assert!(
        item.get("lifecycle").is_some(),
        "milestone item must carry `lifecycle`; got: {item}"
    );
    assert!(
        item.get("lifecycle_at").is_some(),
        "milestone item must carry `lifecycle_at` (M144); got: {item}"
    );
    let at = item["lifecycle_at"].as_str();
    if let Some(at) = at {
        // Must be parseable RFC3339 (length >= 19 + 'T' between date and time).
        assert!(
            at.len() >= 19 && (at.as_bytes()[10] == b'T' || at.as_bytes()[10] == b' '),
            "lifecycle_at must be RFC3339-shaped; got: {at}"
        );
    }
}

#[test]
fn status_format_json() {
    let env = TestEnv::new();
    let out = env.run(&["status", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "planning_status");
}

#[test]
fn path_format_json() {
    let env = TestEnv::new();
    let out = env.run(&["path", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "baseline_milestone_order");
}

#[test]
fn show_milestone_default_json() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "default-json-show",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [{ "description": "AC1", "verification": "echo ok" }]
    }"#;
    let create = env.run(&["milestone", "create", "--json", create_json]);
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    let id: String = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()
        ["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let out = env.run(&["show", "milestone", &id]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert!(
        json.get("milestone").is_some(),
        "expected milestone object: {json}"
    );
}

#[test]
fn format_human_is_rejected() {
    let env = TestEnv::new();
    let out = env.run(&["status", "--format", "human"]);
    assert!(!out.status.success(), "--format human should be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid") || stderr.contains("valid values") || stderr.contains("human"),
        "stderr: {stderr}"
    );
}

/// mp list milestones --format raw returns valid (pretty) JSON.
#[test]
fn list_milestones_format_raw() {
    let env = TestEnv::new();
    let out = env.run(&["list", "milestones", "--format", "raw"]);
    assert!(
        out.status.success(),
        "list milestones --format raw: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "milestones");
}

/// mp list tracks --format raw returns valid (pretty) JSON.
#[test]
fn list_tracks_format_raw() {
    let env = TestEnv::new();
    let out = env.run(&["list", "tracks", "--format", "raw"]);
    assert!(
        out.status.success(),
        "list tracks --format raw: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "tracks");
}

/// mp list backlog --format raw returns valid (pretty) JSON.
#[test]
fn list_backlog_format_raw() {
    let env = TestEnv::new();
    let out = env.run(&["list", "backlog", "--format", "raw"]);
    assert!(
        out.status.success(),
        "list backlog --format raw: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "backlog");
}

/// mp list decisions --format raw returns valid (pretty) JSON.
#[test]
fn list_decisions_format_raw() {
    let env = TestEnv::new();
    let out = env.run(&["list", "decisions", "--format", "raw"]);
    assert!(
        out.status.success(),
        "list decisions --format raw: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_json(&out.stdout, "decisions");
}
