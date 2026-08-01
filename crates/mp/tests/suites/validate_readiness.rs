use crate::common::lib_api;
use crate::common::TestEnv;
use serde_json::Value;

fn create_milestone(env: &TestEnv, title: &str) -> String {
    let create_json = format!(
        r#"{{
        "title": "{title}",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {{ "outcome": "Test {title}" }},
        "problem": {{ "description": "Need to test {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{
                "description": "{title} works",
                "verification": "manual: validate readiness setup"
            }}
        ]
    }}"#
    );
    let out = lib_api::run(
        env,
        &[
            "milestone",
            "create",
            "--json",
            &create_json,
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mid = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let out = lib_api::run(env, &["milestone", "approve", &mid, "--format", "json"]);
    assert!(
        out.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = lib_api::run(env, &["milestone", "decompose", &mid, "--format", "json"]);
    assert!(
        out.status.success(),
        "decompose failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    mid
}

#[test]
fn w40_empty_done_when() {
    let env = TestEnv::new();
    let mid = create_milestone(&env, "W40 test");

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &mid,
            "--wp",
            "WP1",
            "--id",
            "S1",
            "--action",
            "Do something",
            "--done-when",
            "",
            "--tests",
            "echo ok",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "step add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "validate failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let w40s: Vec<_> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "W40")
        .collect();
    assert!(!w40s.is_empty(), "should emit W40 for empty done_when");
}

#[test]
fn w41_bare_manual_accepted_on_step() {
    let env = TestEnv::new();
    let mid = create_milestone(&env, "W41 step test");

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &mid,
            "--wp",
            "WP1",
            "--id",
            "S1",
            "--action",
            "Do a thing",
            "--done-when",
            "works",
            "--tests",
            "echo ok",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "step add: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "validate: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let w41s: Vec<_> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "W41")
        .collect();
    assert!(w41s.is_empty(), "should NOT emit W41 for valid tests value");
}

#[test]
fn w41_step_with_manual_accepted_no_reason() {
    let env = TestEnv::new();
    let mid = create_milestone(&env, "W41 bare");
    // Add an extra AC to cover the step we're about to add
    lib_api::run(
        &env,
        &[
            "milestone",
            "criterion",
            "add",
            &mid,
            "--description",
            "Extra AC",
            "--verification",
            "echo ok",
            "--format",
            "json",
        ],
    );
    let step_update = r#"{"acceptance_criteria": [
            {"description": "W41 bare works", "verification": "echo ok"},
            {"description": "Extra AC", "verification": "echo ok"}
        ]}"#
    .to_string();
    lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            &mid,
            "--json",
            &step_update,
            "--replace-arrays",
            "--format",
            "json",
        ],
    );

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &mid,
            "--wp",
            "WP1",
            "--id",
            "S1",
            "--action",
            "Do a thing",
            "--done-when",
            "works",
            "--tests",
            "manual: accepted",
            "--covers-ac",
            "AC-01,AC-02",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "step add: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "validate: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let w41s: Vec<_> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "W41")
        .collect();
    assert!(
        !w41s.is_empty(),
        "should emit W41 for bare manual: accepted on step"
    );
}

#[test]
fn w42_design_decisions_on_medium_risk() {
    let env = TestEnv::new();
    let mid = create_milestone(&env, "W42 medium");

    let update_json = r#"{"risk": "medium"}"#;
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            &mid,
            "--json",
            update_json,
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "update: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "validate: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let w42s: Vec<_> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "W42")
        .collect();
    assert!(
        !w42s.is_empty(),
        "should emit W42 for risk=medium with no design_decisions"
    );
}

#[test]
fn w42_suppressed_with_design_decision_add() {
    let env = TestEnv::new();
    let mid = create_milestone(&env, "W42 DD test");

    let update_json = r#"{"risk": "high"}"#;
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "update",
            &mid,
            "--json",
            update_json,
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "update: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Add a design decision
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "design-decision",
            "add",
            &mid,
            "--area",
            "Config format",
            "--decision",
            "Use TOML",
            "--rationale",
            "Already used throughout the project",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "design-decision add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "validate: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let w42s: Vec<_> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "W42")
        .collect();
    assert!(
        w42s.is_empty(),
        "should NOT emit W42 when design_decisions exist"
    );
}

#[test]
fn w43_stale_milestone_ref() {
    let env = TestEnv::new();
    let mid = create_milestone(&env, "W43 stale");

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &mid,
            "--wp",
            "WP1",
            "--id",
            "S1",
            "--action",
            "Coordinate with M999 to deliver the feature",
            "--done-when",
            "done",
            "--tests",
            "echo ok",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "step add: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "validate: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let w43s: Vec<_> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "W43")
        .collect();
    assert!(
        !w43s.is_empty(),
        "should emit W43 for reference to non-existent M999"
    );
}

#[test]
fn w43_no_warning_for_valid_ref() {
    let env = TestEnv::new();
    let mid = create_milestone(&env, "W43 valid");

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &mid,
            "--wp",
            "WP1",
            "--id",
            "S1",
            "--action",
            "Implement feature per M01 design",
            "--done-when",
            "done",
            "--tests",
            "echo ok",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "step add: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        out.status.success(),
        "validate: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let w43s: Vec<_> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "W43")
        .collect();
    assert!(
        w43s.is_empty(),
        "should NOT emit W43 for valid milestone reference"
    );
}
