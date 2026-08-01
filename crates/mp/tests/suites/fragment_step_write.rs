//! M93 AC-06: `mp milestone step remove` fails when another step
//! `depends_on_steps` includes the target; succeeds when safe; stdout returns
//! `{ ok, removed: "<step-id>" }`.

use crate::common::{lib_api, TestEnv};

#[test]
fn step_remove_blocks_when_depended_on() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Add a step that explicitly depends on S1 so the guard has a target.
    let add = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            "03",
            "--wp",
            "WP1",
            "--id",
            "S98",
            "--action",
            "Depends on S1",
            "--tests",
            "manual: depends on S1",
            "--done-when",
            "Step exists",
            "--format",
            "json",
        ],
    );
    assert!(
        add.status.success(),
        "step add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    // Add S1 -> S98 dependency via update.
    let dep = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "update",
            "03",
            "S98",
            "--depends-on-steps",
            "S1",
            "--format",
            "json",
        ],
    );
    assert!(
        dep.status.success(),
        "depends_on_steps update failed: {}",
        String::from_utf8_lossy(&dep.stderr)
    );

    // Now removing S1 must fail because S98 depends on it.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "remove",
            "03",
            "S1",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success(), "removing depended-on S1 must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("S1") && stderr.contains("S98"),
        "guard should mention S1 and dependent S98; got: {stderr}"
    );

    // After clearing S98's dependency, removing S1 succeeds.
    let clear = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "update",
            "03",
            "S98",
            "--depends-on-steps",
            "",
            "--format",
            "json",
        ],
    );
    assert!(clear.status.success(), "clear deps failed");

    let remove = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "remove",
            "03",
            "S1",
            "--format",
            "json",
        ],
    );
    assert!(
        remove.status.success(),
        "unblocked remove failed: {}",
        String::from_utf8_lossy(&remove.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&remove.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["removed"], "S1");

    // Verify S1 is gone.
    let show = lib_api::run(
        &env,
        &["milestone", "step", "show", "03", "S1", "--format", "json"],
    );
    assert!(
        !show.status.success(),
        "removed step must not be retrievable"
    );
}

#[test]
fn step_remove_blocks_when_split_children_exist() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Split S2 to create S2.1 + S2.2 (children).
    let split = lib_api::run(
        &env,
        &["milestone", "step", "split", "03", "S2", "--format", "json"],
    );
    assert!(
        split.status.success(),
        "split failed: {}",
        String::from_utf8_lossy(&split.stderr)
    );

    // Removing S2 must fail because S2.1 / S2.2 would be orphaned.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "remove",
            "03",
            "S2",
            "--format",
            "json",
        ],
    );
    assert!(
        !out.status.success(),
        "removing step with split children must fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("S2"),
        "guard should mention S2; got: {stderr}"
    );
}

#[test]
fn step_remove_unknown_step_fails_with_structured_error() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "remove",
            "03",
            "S99",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("S99"),
        "error should mention missing step id; got: {stderr}"
    );
}
