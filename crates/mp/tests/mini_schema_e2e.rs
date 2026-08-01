//! End-to-end schema checks belong to the package that owns the `mp` binary.

mod common;

use crate::common::{mp_bin, repo_root, TestEnv};

#[test]
fn repo_plan_validates_via_mini_schema() {
    let out = std::process::Command::new(mp_bin())
        .current_dir(repo_root())
        .args(["validate", "--format", "json"])
        .output()
        .expect("mp validate");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let non_g8: Vec<&serde_json::Value> = v["errors"]
        .as_array()
        .map(|errors| {
            errors
                .iter()
                .filter(|error| error["code"].as_str() != Some("G8"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        non_g8.is_empty(),
        "mp validate produced non-G8 errors: {non_g8:?}"
    );
}

#[test]
fn valid_milestone_create_passes_schema_gate() {
    let env = TestEnv::new();
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"x","intent":{"outcome":"o"},"problem":{"description":"p"},"scope":{"in_scope":["x"],"out_of_scope":["a","b"]}}"#,
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "valid create should pass schema gate: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
