//! M111 S6: `sh -n` shell-parse pre-flight on `--verification` (and step
//! `--tests`) is a warn-not-reject gate at authoring time. Catches the
//! "validation prose in verification string" bug class at write time, not
//! at `mp milestone complete` time. M106/M110/M117 own the gate runner; this
//! is authoring-side only.

mod common;

use crate::common::TestEnv;

#[test]
fn ac_add_with_unparseable_verification_emits_warning_but_writes() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Mixed prose in `verification` is exactly the M107 shape that prompted
    // M111: `(new) passes; see lib.rs`. `sh -n` rejects unmatched parens.
    let out = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "Pre-flight AC (deliberately awkward)",
        "--verification",
        "cargo test (new) passes; see lib.rs",
    ]);
    assert!(
        out.status.success(),
        "ac add must NOT be blocked by pre-flight; got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert!(
        value["preflight_warning"].is_object(),
        "expected a preflight_warning object; got: {value}"
    );
    assert_eq!(
        value["preflight_warning"]["warning"].as_str(),
        Some("verification shell-parse failed")
    );
    assert!(
        value["preflight_warning"]["exit_code"].is_i64()
            || value["preflight_warning"]["exit_code"].is_null(),
        "exit_code should be a number or null; got: {:?}",
        value["preflight_warning"]["exit_code"]
    );
    // The AC must still be persisted (warn-not-reject).
    let list = env.run(&["milestone", "ac", "list", "03"]);
    assert!(list.status.success());
    let arr: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let arr = arr.as_array().expect("array");
    assert!(
        arr.iter()
            .any(|ac| ac["description"] == "Pre-flight AC (deliberately awkward)"),
        "AWKWARD AC must still be persisted"
    );
}

#[test]
fn ac_add_with_clean_verification_emits_no_warning() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "Clean AC",
        "--verification",
        "cargo test -p mp ac_add_appends",
    ]);
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert!(
        value.get("preflight_warning").is_none() || value["preflight_warning"].is_null(),
        "clean verification must NOT emit a preflight_warning; got: {value}"
    );
}

#[test]
fn ac_update_changes_verification_emits_warning_when_invalid() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Add a clean AC first.
    let add = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "update-target",
        "--verification",
        "cargo test -p mp x",
    ]);
    assert!(add.status.success());
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let ac_id = added["acceptance_criterion"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Now update to a broken verification. Update must still succeed.
    let out = env.run(&[
        "milestone",
        "ac",
        "update",
        "03",
        &ac_id,
        "--verification",
        "cargo test (new) passes; see lib.rs",
    ]);
    assert!(
        out.status.success(),
        "ac update must NOT be blocked by pre-flight; got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert!(
        value["preflight_warning"].is_object(),
        "update --verification with broken string must surface a warning"
    );
}

#[test]
fn step_update_with_unparseable_tests_emits_warning() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&[
        "milestone",
        "step",
        "update",
        "03",
        "S1",
        "--tests",
        "cargo test (new) passes; see lib.rs",
    ]);
    assert!(
        out.status.success(),
        "step update must NOT be blocked by pre-flight"
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert!(
        value["preflight_warning"].is_object(),
        "step update --tests with broken string must surface a warning"
    );
}

/// M177 S2: prose-classified verification (no `manual:` prefix) surfaces
/// `prose_warning` on ac update; write still succeeds (warn-not-reject).
#[test]
fn ac_update_emits_prose_warn_on_parenthetical_verification() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let add = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "prose-target",
        "--verification",
        "cargo test -p mp x",
    ]);
    assert!(add.status.success());
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let ac_id = added["acceptance_criterion"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = env.run(&[
        "milestone",
        "ac",
        "update",
        "03",
        &ac_id,
        "--verification",
        "crates/raul/tests/tui_view_state.rs (grep-based test)",
    ]);
    assert!(
        out.status.success(),
        "ac update must NOT be blocked by prose warn; got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert!(
        value["prose_warning"].is_object(),
        "prose verification must surface prose_warning; got: {value}"
    );
    assert_eq!(
        value["prose_warning"]["warning"].as_str(),
        Some("non-runnable verification string")
    );
    assert_eq!(
        value["prose_warning"]["classified_as"].as_str(),
        Some("manual")
    );
    let suggested = value["prose_warning"]["suggested"].as_str().unwrap_or("");
    assert!(
        suggested.starts_with("manual: "),
        "suggested value must auto-prefix manual:; got {suggested}"
    );
}

/// M177 S2: already-`manual:`-prefixed values suppress the prose warn.
#[test]
fn ac_update_suppresses_prose_warn_when_manual_prefixed() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let add = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "manual-prefixed",
        "--verification",
        "cargo test -p mp x",
    ]);
    assert!(add.status.success());
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let ac_id = added["acceptance_criterion"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = env.run(&[
        "milestone",
        "ac",
        "update",
        "03",
        &ac_id,
        "--verification",
        "manual: crates/raul/tests/tui_view_state.rs (grep-based test)",
    ]);
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert!(
        value.get("prose_warning").is_none() || value["prose_warning"].is_null(),
        "manual: prefix must suppress prose_warning; got: {value}"
    );
}

/// M177 S2: clean runnable commands emit neither prose nor preflight warn.
#[test]
fn verification_warns_on_non_runnable_only_for_prose() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&[
        "milestone",
        "criterion",
        "add",
        "03",
        "--description",
        "runnable AC",
        "--verification",
        "cargo test -p mp ac_add_appends",
    ]);
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert!(
        value.get("prose_warning").is_none() || value["prose_warning"].is_null(),
        "runnable verification must not emit prose_warning; got: {value}"
    );
}
