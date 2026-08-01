//! Tests for mp milestone dependents/deps and related dependency queries.

use crate::common::TestEnv;

fn create_milestone(env: &TestEnv, title: &str, deps: &[&str]) -> String {
    let deps_json: Vec<String> = deps.iter().map(|d| format!("\"{d}\"")).collect();
    let deps_str = deps_json.join(",");
    let json = format!(
        r#"{{
        "title": "{title}",
        "depends_on": [{deps_str}],
        "intent": {{ "outcome": "Ship {title}" }},
        "problem": {{ "description": "Need {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{ "description": "{title} works", "verification": "cargo test" }}
        ]
    }}"#
    );
    let out = env.run(&["milestone", "create", "--json", &json, "--format", "json"]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn deps_returns_forward_dependencies() {
    let env = TestEnv::new();
    let dep_id = create_milestone(&env, "dep-ms", &[]);
    let _child = create_milestone(&env, "child-ms", &[&dep_id]);

    assert!(env
        .run(&["milestone", "approve", &dep_id, "--format", "json"])
        .status
        .success());

    let out = env.run(&["milestone", "deps", &dep_id, "--format", "json"]);
    assert!(
        out.status.success(),
        "deps failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(result["deps"].is_array(), "deps should be an array");
}

#[test]
fn dependents_returns_reverse_dependencies() {
    let env = TestEnv::new();
    let dep_id = create_milestone(&env, "dep-ms2", &[]);
    let child_id = create_milestone(&env, "child-ms2", &[&dep_id]);

    assert!(env
        .run(&["milestone", "approve", &dep_id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&["milestone", "approve", &child_id, "--format", "json"])
        .status
        .success());

    let out = env.run(&["milestone", "dependents", &dep_id, "--format", "json"]);
    assert!(
        out.status.success(),
        "dependents failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let dependents = result["dependents"].as_array().unwrap();
    assert!(!dependents.is_empty(), "should have at least one dependent");
    assert!(
        dependents.iter().any(|d| d.as_str() == Some(&child_id)),
        "dependents should include {child_id}, got {dependents:?}"
    );
}

#[test]
fn deps_on_empty_returns_no_deps() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "no-dep-ms", &[]);
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());

    let out = env.run(&["milestone", "deps", &id, "--format", "json"]);
    assert!(out.status.success());
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let deps = result["deps"].as_array().unwrap();
    assert!(
        deps.is_empty(),
        "milestone with no deps should return empty array"
    );
}

#[test]
fn impact_returns_blast_radius() {
    let env = TestEnv::new();
    let dep = create_milestone(&env, "impact-dep", &[]);
    let _child = create_milestone(&env, "impact-child", &[&dep]);

    assert!(env
        .run(&["milestone", "approve", &dep, "--format", "json"])
        .status
        .success());

    let out = env.run(&["milestone", "impact", &dep, "--format", "json"]);
    assert!(
        out.status.success(),
        "impact failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        result["transitive_dependents"].is_array(),
        "transitive_dependents should be an array"
    );
    assert!(
        result["path_pins"].is_array(),
        "path_pins should be an array"
    );
    assert!(
        result["position_in_path"].is_null() || result["position_in_path"].is_number(),
        "position_in_path should be null or number"
    );
}

#[test]
fn create_warns_on_dangling_dep() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "dangling-ms",
        "depends_on": ["99"],
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [
            { "description": "AC1", "verification": "echo ok" }
        ]
    }"#;
    let out = env.run(&["milestone", "create", "--json", json, "--format", "json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "create should succeed with dangling dep: {stderr}"
    );
    assert!(
        stderr.contains("warning"),
        "should warn about dangling dep: {stderr}"
    );
}
