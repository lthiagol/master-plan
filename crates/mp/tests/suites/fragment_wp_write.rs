//! M93 AC-07: `mp milestone wp remove` fails when any step still references the
//! work package via `work_package`; succeeds when empty.

use crate::common::{lib_api, TestEnv};

#[test]
fn wp_remove_blocks_when_steps_reference_it() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // M03 has WP1 / WP2. WP1 has S1, S2, S3 referencing it. Removing WP1 must fail.
    let out = lib_api::run(
        &env,
        &["milestone", "wp", "remove", "03", "WP1", "--format", "json"],
    );
    assert!(
        !out.status.success(),
        "removing WP1 must fail because S1/S2/S3 reference it"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("WP1"),
        "guard should mention WP1; got: {stderr}"
    );
    // The error should list at least one referencing step.
    assert!(
        stderr.contains("S1") || stderr.contains("S2") || stderr.contains("S3"),
        "guard should list referencing step(s); got: {stderr}"
    );
}

#[test]
fn wp_remove_succeeds_when_empty() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Add an empty WP (no steps reference it).
    let add = lib_api::run(
        &env,
        &[
            "milestone",
            "wp",
            "add",
            "03",
            "--name",
            "Empty WP",
            "--goal",
            "no steps",
            "--id",
            "WP3",
            "--format",
            "json",
        ],
    );
    assert!(
        add.status.success(),
        "wp add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let out = lib_api::run(
        &env,
        &["milestone", "wp", "remove", "03", "WP3", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "empty WP removal failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["removed"], "WP3");
}

#[test]
fn wp_remove_unknown_fails_with_structured_error() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "wp",
            "remove",
            "03",
            "WP99",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("WP99"),
        "error should mention missing WP id; got: {stderr}"
    );
}
