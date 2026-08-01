//! M111 S4: design-decision add records a non-empty `area`; update and remove
//! subcommands exist so DDs are no longer append-only. Pre-M111, DDs had no
//! `--area` flag (CLI defaulted to empty string), and the only fix path was
//! `--replace-arrays` on the whole milestone.

mod common;

use crate::common::TestEnv;

#[test]
fn design_decision_add_records_area() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&[
        "milestone",
        "design-decision",
        "add",
        "03",
        "--area",
        "Linter port",
        "--decision",
        "Use rustfmt",
        "--rationale",
        "Default formatting avoids local divergence",
    ]);
    assert!(
        out.status.success(),
        "design-decision add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    let dd = value["design_decision"].as_object().expect("fragment");
    assert_eq!(dd.get("area").and_then(|v| v.as_str()), Some("Linter port"));
    // The serde field name is `choice`; the CLI flags it as `--decision`.
    assert_eq!(
        dd.get("choice").and_then(|v| v.as_str()),
        Some("Use rustfmt")
    );
    assert_eq!(
        dd.get("rationale").and_then(|v| v.as_str()),
        Some("Default formatting avoids local divergence")
    );
}

#[test]
fn design_decision_update_changes_area_via_index_and_area() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Add two DDs with distinct areas.
    env.run(&[
        "milestone",
        "design-decision",
        "add",
        "03",
        "--area",
        "Linter port",
        "--decision",
        "A",
        "--rationale",
        "r-A",
    ]);
    env.run(&[
        "milestone",
        "design-decision",
        "add",
        "03",
        "--area",
        "Adapter",
        "--decision",
        "B",
        "--rationale",
        "r-B",
    ]);

    // Update by index 0: area → "Linter".
    let upd_idx = env.run(&[
        "milestone",
        "design-decision",
        "update",
        "03",
        "--index",
        "0",
        "--new-area",
        "Linter",
    ]);
    assert!(
        upd_idx.status.success(),
        "update --index failed: {}",
        String::from_utf8_lossy(&upd_idx.stderr)
    );
    let v0: serde_json::Value = serde_json::from_slice(&upd_idx.stdout).unwrap();
    assert_eq!(v0["design_decision"]["area"], "Linter");

    // Update by area: change the second's decision.
    let upd_area = env.run(&[
        "milestone",
        "design-decision",
        "update",
        "03",
        "--area",
        "Adapter",
        "--decision",
        "B2",
    ]);
    assert!(
        upd_area.status.success(),
        "update --area failed: {}",
        String::from_utf8_lossy(&upd_area.stderr)
    );
    let v1: serde_json::Value = serde_json::from_slice(&upd_area.stdout).unwrap();
    assert_eq!(v1["design_decision"]["choice"], "B2");
    // Area was untouched (we didn't pass --new-area).
    assert_eq!(v1["design_decision"]["area"], "Adapter");
}

#[test]
fn design_decision_remove_by_index_and_area() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // The fixture already ships with one design decision. Use that as a known
    // baseline so the test stays robust to other fixtures.
    let initial = env.run(&["show", "milestone", "03", "--fields", "design_decisions"]);
    let initial_value: serde_json::Value = serde_json::from_slice(&initial.stdout).unwrap();
    let initial_count = initial_value["design_decisions"]
        .as_array()
        .expect("array")
        .len();
    assert!(initial_count >= 1, "fixture must have at least one DD");

    let add1 = env.run(&[
        "milestone",
        "design-decision",
        "add",
        "03",
        "--area",
        "X-test-area",
        "--decision",
        "x",
        "--rationale",
        "rx",
    ]);
    assert!(add1.status.success());
    let add2 = env.run(&[
        "milestone",
        "design-decision",
        "add",
        "03",
        "--area",
        "Y-test-area",
        "--decision",
        "y",
        "--rationale",
        "ry",
    ]);
    assert!(add2.status.success());

    // Remove by area Y-test-area.
    let rm = env.run(&[
        "milestone",
        "design-decision",
        "remove",
        "03",
        "--area",
        "Y-test-area",
    ]);
    assert!(
        rm.status.success(),
        "remove --area failed: {}",
        String::from_utf8_lossy(&rm.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&rm.stdout).unwrap();
    assert_eq!(value["ok"], true);

    // After the area-based removal, only the original DDs + X remain.
    let show = env.run(&["show", "milestone", "03", "--fields", "design_decisions"]);
    assert!(show.status.success());
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let dds = v["design_decisions"].as_array().expect("array");
    assert_eq!(dds.len(), initial_count + 1);
    assert!(
        dds.iter().any(|dd| dd["area"] == "X-test-area"),
        "X-test-area must remain after removing by area Y-test-area"
    );
    assert!(
        !dds.iter().any(|dd| dd["area"] == "Y-test-area"),
        "Y-test-area must be gone"
    );

    // Remove by index 0 wipes the first DD.
    let rm2 = env.run(&[
        "milestone",
        "design-decision",
        "remove",
        "03",
        "--index",
        "0",
    ]);
    assert!(rm2.status.success());
    let show2 = env.run(&["show", "milestone", "03", "--fields", "design_decisions"]);
    let v2: serde_json::Value = serde_json::from_slice(&show2.stdout).unwrap();
    let dds2 = v2["design_decisions"].as_array().expect("array");
    assert_eq!(dds2.len(), initial_count);
}

#[test]
fn design_decision_remove_unknown_area_fails() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    env.run(&[
        "milestone",
        "design-decision",
        "add",
        "03",
        "--area",
        "exists",
        "--decision",
        "d",
        "--rationale",
        "r",
    ]);
    let bad = env.run(&[
        "milestone",
        "design-decision",
        "remove",
        "03",
        "--area",
        "no-such",
    ]);
    assert!(!bad.status.success(), "removing by unknown area must fail");
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("no-such"),
        "error must echo the unknown area; got: {stderr}"
    );
}
