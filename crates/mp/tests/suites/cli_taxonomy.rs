//! CLI taxonomy: assert the regroup surface is stable.
//! - mp next-step is absent (collapsed into mp next)
//! - --format raw is accepted (verbatim JSON passthrough)
//! - --format toml is rejected (dropped in the TOML→JSON migration)
//! - --format raw produces JSON on read AND list commands
//! - object-group homes exist

use crate::common::TestEnv;

#[test]
fn next_step_is_absent() {
    let env = TestEnv::new();
    let out = env.run(&["next-step", "--format", "json"]);
    assert!(!out.status.success(), "next-step should be absent");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unrecognized") || stderr.contains("not found") || stderr.contains("error"),
        "stderr: {stderr}"
    );
}

#[test]
fn next_is_present() {
    let env = TestEnv::new();
    let out = env.run(&["next", "--format", "json"]);
    assert!(
        out.status.success(),
        "next should be present: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn format_raw_is_accepted() {
    let env = TestEnv::new();
    let out = env.run(&["status", "--format", "raw"]);
    assert!(
        out.status.success(),
        "--format raw should be accepted: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().starts_with('{'),
        "output should be JSON: {stdout}"
    );
}

#[test]
fn format_toml_is_rejected() {
    let env = TestEnv::new();
    let out = env.run(&["status", "--format", "toml"]);
    assert!(!out.status.success(), "--format toml should be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid") || stderr.contains("valid values") || stderr.contains("raw"),
        "stderr: {stderr}"
    );
}

#[test]
fn format_raw_on_read() {
    let env = TestEnv::new();
    // Create a milestone first so we have something to show
    let json = r#"{
        "title": "raw-read-test",
        "intent": { "outcome": "Test" },
        "problem": { "description": "Test" },
        "scope": { "in_scope": ["X"], "out_of_scope": ["A", "B"] },
        "acceptance_criteria": [
            { "description": "AC1", "verification": "echo ok" }
        ]
    }"#;
    let create = env.run(&["milestone", "create", "--json", json, "--format", "json"]);
    assert!(create.status.success());
    let id: String = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()
        ["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let out = env.run(&["show", "milestone", &id, "--format", "raw"]);
    assert!(
        out.status.success(),
        "show --format raw should work: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"title\"") || stdout.contains("\"milestone\""),
        "output should be JSON: {stdout}"
    );
}

#[test]
fn format_raw_on_list() {
    let env = TestEnv::new();
    let out = env.run(&["list", "milestones", "--format", "raw"]);
    assert!(
        out.status.success(),
        "list --format raw should work: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("milestones"),
        "output should list milestones: {stdout}"
    );
}

#[test]
fn object_group_milestone_subcommands_exist() {
    let env = TestEnv::new();
    for sub in &["groom", "challenge", "step", "wp"] {
        let out = env.run(&["milestone", sub, "--help"]);
        assert!(
            out.status.success(),
            "mp milestone {sub} should exist: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn object_group_track_subcommands_exist() {
    let env = TestEnv::new();
    for sub in &["archive", "restore", "purge"] {
        let out = env.run(&["track", sub, "--help"]);
        assert!(
            out.status.success(),
            "mp track {sub} should exist: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn object_group_plan_metrics_exists() {
    let env = TestEnv::new();
    let out = env.run(&["plan", "metrics", "--help"]);
    assert!(
        out.status.success(),
        "mp plan metrics should exist: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn object_group_specs_delta_exists() {
    let env = TestEnv::new();
    let out = env.run(&["specs", "delta", "--help"]);
    assert!(
        out.status.success(),
        "mp specs delta should exist: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
