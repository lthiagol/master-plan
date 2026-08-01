//! M92 AC-03: store.rs plan I/O uses JSON only — no `toml::` parse/serialize on
//! plan-artifact load/write paths. This is a source-level invariant test: it
//! reads `crates/mp/src/store.rs` and asserts no `toml::from_str` /
//! `toml::to_string` / `toml::to_string_pretty` calls remain on plan I/O.
//!
//! (`mp` still depends on the `toml` crate for Cargo.toml parsing in
//! `install.rs` and the one-time `migrate` module — both out of scope here.
//! This test scopes the claim to store.rs specifically, matching AC-03.)

use std::fs;

#[test]
fn store_rs_has_no_toml_codec_calls() {
    let store = env!("CARGO_MANIFEST_DIR").to_string() + "/src/store.rs";
    let src = fs::read_to_string(&store).unwrap_or_else(|e| panic!("read {store}: {e}"));

    // No TOML parse/serialize may remain in the plan store layer.
    assert!(
        !src.contains("toml::from_str"),
        "store.rs must not deserialize TOML (M92 AC-03): {}",
        store
    );
    assert!(
        !src.contains("toml::to_string"),
        "store.rs must not serialize TOML (M92 AC-03): {}",
        store
    );
}

/// A round-trip through the store (write then load) preserves data and the
/// on-disk file is JSON (parses with serde_json, not toml).
#[test]
fn store_round_trips_json() {
    use crate::common::TestEnv;
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    // The init-scaffolded plan index must be valid JSON on disk.
    let plan_path = env.tmp.path().join("master-plan/plan.json");
    assert!(plan_path.exists(), "plan.json should be scaffolded by init");
    let raw = fs::read_to_string(&plan_path).unwrap();
    assert!(
        serde_json::from_str::<serde_json::Value>(&raw).is_ok(),
        "plan.json must be valid JSON, got: {raw}"
    );

    // Creating a milestone writes a .json file, and it round-trips through load.
    let create = r#"{
        "title": "RT",
        "intent": {"outcome": "round trip"},
        "problem": {"description": "p"},
        "scope": {"in_scope": ["x"], "out_of_scope": ["a", "b"]},
        "acceptance_criteria": [{"description": "ac", "verification": "manual: accepted — t"}]
    }"#;
    let out = env.run(&["milestone", "create", "--json", create]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // mp show reads it back via the store (JSON load path).
    let show = env.run(&["show", "milestone", &id]);
    assert!(show.status.success());
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(v["milestone"]["title"].as_str().unwrap(), "RT");
}
