//! M112 S2: `mp show milestone --fields` projects against the on-disk JSON
//! directly, so any field the file carries (even legacy/dropped-ceremony keys
//! like `follow_ups`, `behavior`, or new fields the typed struct doesn't
//! model yet) reads back without "unknown path" errors.

mod common;

use crate::common::TestEnv;

#[test]
fn fields_reads_legacy_ceremony_keys() {
    // The walkthrough-oauth fixture still carries `behavior` and `context`
    // (M82 dropped-ceremony keys). `mp show milestone --fields behavior`
    // must read it back without "unknown path" — the test is on the raw
    // projection path that AC-02 pins.
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&["show", "milestone", "03", "--fields", "behavior"]);
    assert!(
        out.status.success(),
        "--fields behavior must not error on legacy content; got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["behavior"].is_object(),
        "behavior should appear in projection"
    );
}

#[test]
fn fields_reads_schema_known_field() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // change_kind is a known schema field; surface it cleanly.
    let out = env.run(&[
        "show",
        "milestone",
        "03",
        "--fields",
        "milestone.title,milestone.change_kind",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["milestone"]["title"].as_str(), Some("OAuth Login"));
    // change_kind may be empty string for greenfield (default).
    assert!(v["milestone"].get("change_kind").is_some());
}

#[test]
fn fields_combines_top_and_legacy_field() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&[
        "show",
        "milestone",
        "03",
        "--fields",
        "milestone.title,context",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["milestone"]["title"].as_str(), Some("OAuth Login"));
    // context is on disk for this fixture.
    assert!(
        v["context"].is_object(),
        "legacy `context` field must be readable alongside schema-known fields"
    );
}

#[test]
fn fields_unknown_ground_truth_field_still_errors_clearly() {
    // Truly absent fields (not in the on-disk JSON at all) keep the existing
    // "unknown path" error so typos surface immediately.
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&[
        "show",
        "milestone",
        "03",
        "--fields",
        "definitely_not_a_field",
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown path") && stderr.contains("definitely_not_a_field"),
        "absent fields must still error with their name; got: {stderr}"
    );
}
