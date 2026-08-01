//! M111 S1: `mp milestone ac update --evidence` and
//! `mp milestone step update --evidence` write the fragment's evidence field
//! in place. Pre-M111, agents had to fall back to
//! `mp milestone update --json --replace-arrays` (4-step jq dance) to stamp
//! per-AC/per-step evidence.

mod common;

use crate::common::TestEnv;

#[test]
fn ac_evidence_round_trips_via_fragment_update() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Add a fresh AC so the existing covered ACs (AC-01..AC-03) aren't disturbed.
    let add = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "ev-flag ac",
        "--verification",
        "manual: ev-flag verification",
    ]);
    assert!(
        add.status.success(),
        "criterion add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let ac_id = added["acceptance_criterion"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Stamp evidence on the AC via the fragment command.
    let out = env.run(&[
        "milestone",
        "ac",
        "update",
        "03",
        &ac_id,
        "--evidence",
        "manual run on 2026-07-05; cargo test -p mp passed",
    ]);
    assert!(
        out.status.success(),
        "ac update --evidence failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    let fragment = value["acceptance_criterion"].as_object().expect("fragment");
    assert_eq!(
        fragment.get("id").and_then(|v| v.as_str()),
        Some(ac_id.as_str())
    );
    assert_eq!(
        fragment.get("evidence").and_then(|v| v.as_str()),
        Some("manual run on 2026-07-05; cargo test -p mp passed")
    );
    // Fragment contract: only the changed field + id are returned.
    let keys: std::collections::BTreeSet<&str> = fragment.keys().map(|s| s.as_str()).collect();
    assert_eq!(
        keys,
        ["evidence", "id"]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        "ac update --evidence returned extra keys: {:?}",
        fragment.keys().collect::<Vec<_>>()
    );

    // Read back via ac show -- fields evidence was persisted.
    let show = env.run(&[
        "milestone",
        "ac",
        "show",
        "03",
        &ac_id,
        "--fields",
        "evidence",
    ]);
    assert!(show.status.success(), "ac show --fields evidence failed");
    let persisted: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        persisted["evidence"],
        "manual run on 2026-07-05; cargo test -p mp passed"
    );
}

#[test]
fn step_evidence_round_trips_via_fragment_update() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Add a fresh step so the existing ones (S1..S5) aren't disturbed.
    let add = env.run(&[
        "milestone",
        "step",
        "add",
        "03",
        "--wp",
        "WP1",
        "--id",
        "S99",
        "--action",
        "ev-flag step",
        "--tests",
        "manual: ev-flag step tests",
        "--done-when",
        "step is implemented",
    ]);
    assert!(
        add.status.success(),
        "step add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let out = env.run(&[
        "milestone",
        "step",
        "update",
        "03",
        "S99",
        "--evidence",
        "manual run on 2026-07-05; cargo test -p mp passed",
    ]);
    assert!(
        out.status.success(),
        "step update --evidence failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    let step = value["step"].as_object().expect("step fragment");
    assert_eq!(step.get("id").and_then(|v| v.as_str()), Some("S99"));
    assert_eq!(
        step.get("evidence").and_then(|v| v.as_str()),
        Some("manual run on 2026-07-05; cargo test -p mp passed")
    );

    // Read back via step show.
    let show = env.run(&["milestone", "step", "show", "03", "S99"]);
    assert!(show.status.success(), "step show failed");
    let persisted: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        persisted["evidence"],
        "manual run on 2026-07-05; cargo test -p mp passed"
    );
}

#[test]
fn ac_update_partial_evidence_does_not_clobber_other_fields() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let add = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "partial-ev description",
        "--verification",
        "manual: partial-ev verification",
    ]);
    assert!(add.status.success());
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let ac_id = added["acceptance_criterion"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Stamp evidence only.
    let update = env.run(&[
        "milestone",
        "ac",
        "update",
        "03",
        &ac_id,
        "--evidence",
        "ev-note",
    ]);
    assert!(update.status.success());

    // Description + verification remain as-authored.
    let show = env.run(&["milestone", "ac", "show", "03", &ac_id]);
    assert!(show.status.success());
    let persisted: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(persisted["description"], "partial-ev description");
    assert_eq!(persisted["verification"], "manual: partial-ev verification");
    assert_eq!(persisted["evidence"], "ev-note");
}
