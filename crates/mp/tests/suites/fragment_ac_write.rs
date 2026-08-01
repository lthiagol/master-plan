//! M93 AC-04 / AC-05: `mp milestone ac update` returns the changed fragment;
//! `mp milestone ac remove` refuses when a step `covers_ac` includes the target,
//! succeeds with `{ ok, removed }` when uncovered.

use crate::common::{lib_api, TestEnv};

#[test]
fn ac_update_returns_only_changed_fragment() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // First add a new AC so we can mutate it without disturbing covered ACs.
    let add = lib_api::run(
        &env,
        &[
            "milestone",
            "criterion",
            "add",
            "03",
            "--description",
            "initial description",
            "--verification",
            "manual: pending",
            "--format",
            "json",
        ],
    );
    assert!(
        add.status.success(),
        "criterion add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let new_id = added["acceptance_criterion"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(new_id, "AC-04");

    // Update both fields.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "ac",
            "update",
            "03",
            &new_id,
            "--description",
            "updated description",
            "--verification",
            "crates/mp/tests/fragment_ac_write.rs",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "ac update failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);

    // Fragment-only contract (M93 AC-04): the returned acceptance_criterion
    // object contains ONLY the changed fields plus `id` — never status, evidence,
    // or any unchanged field.
    let ac = value
        .get("acceptance_criterion")
        .and_then(|v| v.as_object())
        .expect("acceptance_criterion fragment");
    assert_eq!(
        ac.keys()
            .map(|s| s.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        ["id", "description", "verification"]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        "ac update returned extra/unexpected keys: {:?}",
        ac.keys().collect::<Vec<_>>()
    );
    assert_eq!(ac.get("id").and_then(|v| v.as_str()), Some(new_id.as_str()));
    assert_eq!(
        ac.get("description").and_then(|v| v.as_str()),
        Some("updated description")
    );
    assert_eq!(
        ac.get("verification").and_then(|v| v.as_str()),
        Some("crates/mp/tests/fragment_ac_write.rs")
    );
    assert!(
        ac.get("status").is_none(),
        "status must NOT be in fragment when it was not changed"
    );
    assert!(
        ac.get("evidence").is_none(),
        "evidence must NOT be in fragment when it was not changed"
    );

    // On-disk state actually changed.
    let show = lib_api::run(
        &env,
        &["milestone", "ac", "show", "03", &new_id, "--format", "json"],
    );
    assert!(show.status.success());
    let persisted: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(persisted["description"], "updated description");
    assert_eq!(
        persisted["verification"],
        "crates/mp/tests/fragment_ac_write.rs"
    );

    // Empty update errors clearly.
    let bad = lib_api::run(
        &env,
        &[
            "milestone",
            "ac",
            "update",
            "03",
            &new_id,
            "--format",
            "json",
        ],
    );
    assert!(!bad.status.success(), "empty update should fail");

    // Unknown AC fails.
    let bad2 = lib_api::run(
        &env,
        &[
            "milestone",
            "ac",
            "update",
            "03",
            "AC-99",
            "--description",
            "x",
            "--format",
            "json",
        ],
    );
    assert!(!bad2.status.success(), "update on unknown AC should fail");
}

/// Partial update: only --description. The returned fragment must contain only
/// `id` and `description` — NOT `verification`.
#[test]
fn ac_update_partial_only_returns_changed_field() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let add = lib_api::run(
        &env,
        &[
            "milestone",
            "criterion",
            "add",
            "03",
            "--description",
            "initial desc",
            "--verification",
            "manual: initial verification",
            "--format",
            "json",
        ],
    );
    assert!(add.status.success());
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let new_id = added["acceptance_criterion"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "ac",
            "update",
            "03",
            &new_id,
            "--description",
            "partial update desc",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "partial update failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ac = value["acceptance_criterion"].as_object().expect("object");
    let keys: std::collections::BTreeSet<&str> = ac.keys().map(|s| s.as_str()).collect();
    assert_eq!(
        keys,
        ["description", "id"]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        "partial update returned extra keys: {:?}",
        ac.keys().collect::<Vec<_>>()
    );
    assert_eq!(ac["id"], new_id);
    assert_eq!(ac["description"], "partial update desc");

    // On-disk verification was NOT touched by this partial update.
    let show = lib_api::run(
        &env,
        &["milestone", "ac", "show", "03", &new_id, "--format", "json"],
    );
    assert!(show.status.success());
    let persisted: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        persisted["verification"], "manual: initial verification",
        "partial update must not clobber unchanged verification field"
    );
}

#[test]
fn ac_remove_blocks_when_step_covers_the_ac() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // First add a step that covers AC-01 so the guard has something to fire on
    // (the fixture doesn't model covers_ac on its existing steps).
    let add_step = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            "03",
            "--wp",
            "WP1",
            "--id",
            "S99",
            "--action",
            "Cover AC-01 explicitly",
            "--tests",
            "manual: step that covers AC-01",
            "--done-when",
            "Step exists",
            "--covers-ac",
            "AC-01",
            "--format",
            "json",
        ],
    );
    assert!(
        add_step.status.success(),
        "step add failed: {}",
        String::from_utf8_lossy(&add_step.stderr)
    );

    // AC-01 is now covered by S99 — guard must refuse removal.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "ac",
            "remove",
            "03",
            "AC-01",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success(), "removing covered AC-01 must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("AC-01") && stderr.contains("S99"),
        "guard error should mention AC and the covering step(s); got: {stderr}"
    );

    // Add a fresh, uncovered AC and remove it — success path.
    let add = lib_api::run(
        &env,
        &[
            "milestone",
            "criterion",
            "add",
            "03",
            "--description",
            "uncovered",
            "--verification",
            "manual: covered by no step",
            "--format",
            "json",
        ],
    );
    assert!(add.status.success());
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let new_id = added["acceptance_criterion"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let remove = lib_api::run(
        &env,
        &[
            "milestone",
            "ac",
            "remove",
            "03",
            &new_id,
            "--format",
            "json",
        ],
    );
    assert!(
        remove.status.success(),
        "uncovered AC removal failed: {}",
        String::from_utf8_lossy(&remove.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&remove.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["removed"], new_id);

    // Verify the AC is gone via ac show.
    let show = lib_api::run(
        &env,
        &["milestone", "ac", "show", "03", &new_id, "--format", "json"],
    );
    assert!(!show.status.success(), "removed AC must not be retrievable");
}
