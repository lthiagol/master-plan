//! M100 ER-9 idempotency regression: a re-run of
//! `mp edit migrate-lifecycle` against a fully-migrated plan must be
//! a no-op. Specifically:
//!  - `legacy_count` stays 0
//!  - `files_rewritten` is 0
//!
//! Pin against a future refactor that drops the
//! `obj.get("lifecycle").is_none()` guard or the on-disk text-equality
//! check on plan.json.
//!
//! Catches the regression mode where someone removes the "if missing,
//! set lifecycle" guard and the migration would otherwise re-stamp
//! every plan.json entry on every run (slow + churns the audit log).

mod common;
use common::TestEnv;
use serde_json::json;

#[test]
fn migrate_plan_json_is_idempotent_on_rerun() {
    let env = TestEnv::new();
    env.run(&[
        "milestone",
        "create",
        "--json",
        &json!({
            "title": "ER-9 idempotency pin",
            "intent": {"outcome": "no-op on second migration run"},
            "problem": {"description": "test fixture"},
            "scope": {"in_scope": ["idempotency"], "out_of_scope": ["other", "follow-up"]},
            "acceptance_criteria": [{"description": "pin", "verification": "manual: accepted"}]
        })
        .to_string(),
    ]);

    // First run. Should set lifecycle on plan.json + maybe migrate the
    // newly-created milestone file.
    let r1 = env.run(&["edit", "migrate-lifecycle", "--yes"]);
    assert!(r1.status.success());
    let r1_json: serde_json::Value = serde_json::from_slice(&r1.stdout).unwrap();

    // Second run. Must be a no-op: the plan is already migrated.
    let r2 = env.run(&["edit", "migrate-lifecycle", "--yes"]);
    assert!(r2.status.success(), "second migration should succeed");
    let r2_json: serde_json::Value = serde_json::from_slice(&r2.stdout).unwrap();

    assert_eq!(r2_json["legacy_count"], json!(0));
    assert_eq!(
        r2_json["files_rewritten"],
        json!(0),
        "second migration must not rewrite any file (was: {}); first run wrote {} files",
        r2_json["files_rewritten"],
        r1_json["files_rewritten"]
    );
}

#[test]
fn migrate_plan_json_writes_lifecycle_on_first_run() {
    // Companion to the idempotency test above: confirm the first run
    // DID touch plan.json (so the idempotency test isn't trivially
    // passing because nothing ever happens).
    let env = TestEnv::new();
    env.run(&[
        "milestone",
        "create",
        "--json",
        &json!({
            "title": "ER-9 first-run pin",
            "intent": {"outcome": "first run touches plan.json"},
            "problem": {"description": "test fixture"},
            "scope": {"in_scope": ["first-run"], "out_of_scope": ["other", "follow-up"]},
            "acceptance_criteria": [{"description": "pin", "verification": "manual: accepted"}]
        })
        .to_string(),
    ]);

    let r1 = env.run(&["edit", "migrate-lifecycle", "--yes"]);
    assert!(r1.status.success());
    let r1_json: serde_json::Value = serde_json::from_slice(&r1.stdout).unwrap();

    let rewrote = r1_json["files_rewritten"].as_u64().unwrap_or(0);
    // plan.json rewrites on first run for fixture plans (where index
    // entries need lifecycle). It may also rewrite the milestone
    // file if it was on the legacy shape. Confirm at least one file
    // got rewritten.
    assert!(
        rewrote >= 1,
        "first migration run should rewrite at least one file (milestone file and/or plan.json); got: {}",
        rewrote
    );
}
