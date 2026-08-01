//! M170 S3 / TW-03: `mp backlog add` prints the assigned id as the first
//! line of stdout (`Assigned: B-<n>`), then the usual JSON payload.

mod common;

use crate::common::TestEnv;

#[test]
fn backlog_add_prints_assigned_id_as_first_line() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&[
        "backlog",
        "add",
        "--desc",
        "tw-03 smoke",
        "--source",
        "dogfood",
        "--priority",
        "low",
    ]);
    assert!(
        out.status.success(),
        "backlog add should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout.lines().next().expect("stdout non-empty");
    let id = first
        .strip_prefix("Assigned: ")
        .unwrap_or_else(|| panic!("first line must be 'Assigned: <id>', got: {first:?}"));
    assert!(
        id.starts_with("B-")
            && id
                .strip_prefix("B-")
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())),
        "assigned id must match B-[0-9]+, got: {id:?}"
    );

    // Remainder after the first line is still valid JSON.
    let json_start = stdout.find('{').expect("JSON body after Assigned line");
    let v: serde_json::Value = serde_json::from_str(&stdout[json_start..]).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["item"]["id"], id);
    assert_eq!(v["item"]["description"], "tw-03 smoke");
}
