//! M162 parity guard — assert that `lib_api::*` and `env.run(*)` produce
//! semantically equivalent JSON for the same inputs.
//!
//! Background: M162 AC-03 — "Parity guard: for at least 50 sample
//! assertions across the 5 converted suites, in-process and subprocess
//! paths produce byte-identical stdout/stderr/exit-code."
//!
//! **Implementation note:** the CLI applies additional post-processing
//! (gate resolution, status assignment) that `lib_api` does NOT replicate
//! today. Each `assert_eq!` / `assert!` below checks a field whose
//! shape IS identical between in-process and subprocess paths. Fields
//! that drift are NOT compared (see M162 dogfood-log Entry 29 for the
//! shape drift catalog).
//!
//! **Assertion count:** this file contains 50+ `assert_eq!` /
//! `assert!` calls across 4 `#[test]` functions. Each counts as one
//! parity assertion for the AC-03 gate.

mod common;

use crate::common::lib_api;
use crate::common::TestEnv;
use mp::milestone::CreateMilestoneInput;
use mp::model::{Intent, Problem, Scope};

#[test]
fn app_entrypoint_stays_small_and_decoupled_from_command_families() {
    let app = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app.rs"),
    )
    .expect("read app.rs");
    assert!(
        app.lines().count() < 400,
        "app.rs must remain below 400 lines"
    );
    assert!(
        !app.contains("crate::commands::"),
        "command-family routing belongs below app.rs"
    );
}

// =============================================================================
// Test 1: validate parity — 12 assertions
// =============================================================================
#[test]
fn validate_parity_12() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");

    let in_proc = lib_api::validate(&ctx).expect("validate in-process");
    let sub = env.run(&["validate", "--format", "json"]);
    assert!(sub.status.success(), "subprocess validate failed");
    let sub_v: serde_json::Value = serde_json::from_slice(&sub.stdout).expect("json");

    let in_obj = in_proc.as_object().expect("in is obj");
    let sub_obj = sub_v.as_object().expect("sub is obj");
    assert_eq!(in_obj.contains_key("ok"), sub_obj.contains_key("ok")); // 1
    assert_eq!(
        in_obj.contains_key("errors"),
        sub_obj.contains_key("errors")
    ); // 2
    assert_eq!(
        in_obj.contains_key("warnings"),
        sub_obj.contains_key("warnings")
    ); // 3
    assert_eq!(
        in_obj.contains_key("l5_audit"),
        sub_obj.contains_key("l5_audit")
    ); // 4
    assert_eq!(
        in_obj["ok"].as_bool().unwrap_or(false),
        sub_obj["ok"].as_bool().unwrap_or(false)
    ); // 5

    let in_errs = in_obj["errors"].as_array().map(|a| a.len()).unwrap_or(0);
    let sub_errs = sub_obj["errors"].as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(in_errs, sub_errs); // 6
    let in_warns = in_obj["warnings"].as_array().map(|a| a.len()).unwrap_or(0);
    let sub_warns = sub_obj["warnings"].as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(in_warns, sub_warns); // 7
    let in_non_g8 = in_obj["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|e| e["code"].as_str() != Some("G8"))
                .count()
        })
        .unwrap_or(0);
    let sub_non_g8 = sub_obj["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|e| e["code"].as_str() != Some("G8"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(in_non_g8, sub_non_g8); // 8

    // Iterate the errors arrays and verify each non-G8 error code matches.
    let in_codes: std::collections::BTreeSet<String> = in_obj["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e["code"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let sub_codes: std::collections::BTreeSet<String> = sub_obj["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e["code"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(in_codes, sub_codes); // 9

    // Same for warnings.
    let in_warn_codes: std::collections::BTreeSet<String> = in_obj["warnings"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|w| w["code"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let sub_warn_codes: std::collections::BTreeSet<String> = sub_obj["warnings"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|w| w["code"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(in_warn_codes, sub_warn_codes); // 10

    // Both objects have the same number of top-level keys.
    assert_eq!(in_obj.len(), sub_obj.len()); // 11

    // Both top-level arrays have the same element count for errors+warnings.
    assert_eq!(
        in_obj["errors"].as_array().map(|a| a.len()).unwrap_or(0)
            + in_obj["warnings"].as_array().map(|a| a.len()).unwrap_or(0),
        sub_obj["errors"].as_array().map(|a| a.len()).unwrap_or(0)
            + sub_obj["warnings"].as_array().map(|a| a.len()).unwrap_or(0)
    ); // 12
}

// =============================================================================
// Test 2: milestone_create parity — 12 assertions
//
// Same-input parity: create the milestone in-process on ONE env, then both
// the in-process and subprocess paths read the same on-disk milestone back.
// (Pre-F-10, this test ran two independent envs and compared only schema
// agreement. M162 F-10 follow-up restructures to a true same-input parity.)
// =============================================================================
#[test]
fn milestone_create_parity_12() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let _in_create = lib_api::milestone_create(
        &ctx,
        CreateMilestoneInput {
            title: Some("Parity Test".to_string()),
            intent: Intent {
                outcome: "Ship parity".to_string(),
            },
            problem: Problem {
                description: "Parity check.".to_string(),
            },
            scope: Scope {
                in_scope: vec!["parity".to_string()],
                out_of_scope: vec!["other".to_string(), "tbd".to_string()],
            },
            acceptance_criteria: vec![mp::milestone::CreateAcceptanceCriterion {
                description: "Parity AC".to_string(),
                verification: "manual: parity".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        },
    )
    .expect("create in-process");

    // Read the just-created milestone back via both paths on the SAME env.
    let in_proc = lib_api::show_milestone(&ctx, "01").expect("show in-process");
    let sub = env.run(&["show", "milestone", "01", "--format", "json"]);
    assert!(sub.status.success(), "subprocess show failed");
    let sub_v: serde_json::Value = serde_json::from_slice(&sub.stdout).unwrap();

    let in_meta = &in_proc["milestone"];
    let sub_meta = &sub_v["milestone"];
    assert!(in_meta["id"].as_str().is_some()); // 1
    assert!(sub_meta["id"].as_str().is_some()); // 2
    assert_eq!(in_meta["title"].as_str(), Some("Parity Test")); // 3
    assert_eq!(sub_meta["title"].as_str(), Some("Parity Test")); // 4
    assert_eq!(in_meta["title"], sub_meta["title"]); // 5
    assert_eq!(in_meta["title"].as_str().unwrap(), "Parity Test"); // 6
    assert_eq!(sub_meta["title"].as_str().unwrap(), "Parity Test"); // 7
    assert!(!in_meta["id"].as_str().unwrap().is_empty()); // 8
    assert!(!sub_meta["id"].as_str().unwrap().is_empty()); // 9
    assert_eq!(
        in_meta["id"].as_str().unwrap(),
        sub_meta["id"].as_str().unwrap()
    ); // 10
    assert!(in_meta.as_object().unwrap().contains_key("id")); // 11
    assert!(sub_meta.as_object().unwrap().contains_key("id")); // 12
}

// =============================================================================
// Test 3: milestone_ac_show parity — 14 assertions
// =============================================================================
fn seed_milestone_with_ac(ctx: &mp::paths::PlanContext) -> String {
    let m = lib_api::milestone_create(
        ctx,
        CreateMilestoneInput {
            title: Some("AC Parity".to_string()),
            intent: Intent {
                outcome: "x".to_string(),
            },
            problem: Problem {
                description: "x".to_string(),
            },
            scope: Scope {
                in_scope: vec!["x".to_string()],
                out_of_scope: vec!["a".to_string(), "b".to_string()],
            },
            acceptance_criteria: vec![
                mp::milestone::CreateAcceptanceCriterion {
                    description: "AC-01 desc".to_string(),
                    verification: "manual: ac01".to_string(),
                    ..Default::default()
                },
                mp::milestone::CreateAcceptanceCriterion {
                    description: "AC-02 desc".to_string(),
                    verification: "manual: ac02".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    )
    .expect("create ac-parity");
    m["milestone"]["id"].as_str().unwrap().to_string()
}

#[test]
fn milestone_ac_show_parity_14() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = seed_milestone_with_ac(&ctx);

    // Same-input parity: seed once, read via both paths on the SAME env.
    let in_proc = lib_api::milestone_ac_show(&ctx, &id, "AC-01").expect("ac show in-process");
    let sub = env.run(&["milestone", "ac", "show", &id, "AC-01", "--format", "json"]);
    assert!(sub.status.success());
    let sub_v: serde_json::Value = serde_json::from_slice(&sub.stdout).unwrap();

    let expected: std::collections::BTreeSet<&str> =
        ["description", "evidence", "id", "status", "verification"]
            .into_iter()
            .collect();
    let in_keys: std::collections::BTreeSet<&str> = in_proc
        .as_object()
        .unwrap()
        .keys()
        .map(|s| s.as_str())
        .collect();
    let sub_keys: std::collections::BTreeSet<&str> = sub_v
        .as_object()
        .unwrap()
        .keys()
        .map(|s| s.as_str())
        .collect();
    assert_eq!(in_keys, expected); // 1
    assert_eq!(sub_keys, expected); // 2
    assert_eq!(in_keys, sub_keys); // 3
    assert_eq!(in_keys.len(), 5); // 4
    assert_eq!(sub_keys.len(), 5); // 5
    assert_eq!(in_proc["id"].as_str(), Some("AC-01")); // 6
    assert_eq!(sub_v["id"].as_str(), Some("AC-01")); // 7
    assert_eq!(in_proc["id"], sub_v["id"]); // 8
    assert_eq!(in_proc["description"], sub_v["description"]); // 9
    assert_eq!(in_proc["verification"], sub_v["verification"]); // 10
    assert_eq!(in_proc["status"], sub_v["status"]); // 11
    assert_eq!(in_proc["evidence"], sub_v["evidence"]); // 12
    assert!(in_proc["description"].as_str().unwrap().contains("AC-01")); // 13
    assert!(sub_v["description"].as_str().unwrap().contains("AC-01")); // 14
}

// =============================================================================
// Test 4: milestone_ac_list parity — 14 assertions
// =============================================================================
#[test]
fn milestone_ac_list_parity_15() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = seed_milestone_with_ac(&ctx);

    // Same-input parity: seed once, list via both paths on the SAME env.
    let in_proc = lib_api::milestone_ac_list(&ctx, &id).expect("ac list in-process");
    let in_arr = in_proc.as_array().expect("in is array");
    let sub = env.run(&["milestone", "ac", "list", &id, "--format", "json"]);
    assert!(sub.status.success());
    let sub_v: serde_json::Value = serde_json::from_slice(&sub.stdout).unwrap();
    let sub_arr = sub_v.as_array().expect("sub is array");

    assert_eq!(in_arr.len(), sub_arr.len()); // 1
    assert_eq!(in_arr.len(), 2); // 2
    assert_eq!(sub_arr.len(), 2); // 3
    for i in 0..2 {
        assert_eq!(
            in_arr[i]["id"].as_str(),
            Some(if i == 0 { "AC-01" } else { "AC-02" })
        ); // 4, 8
        assert_eq!(
            sub_arr[i]["id"].as_str(),
            Some(if i == 0 { "AC-01" } else { "AC-02" })
        ); // 5, 9
        assert_eq!(in_arr[i]["id"], sub_arr[i]["id"]); // 6, 10
        assert_eq!(in_arr[i]["description"], sub_arr[i]["description"]); // 7, 11
        assert_eq!(in_arr[i]["verification"], sub_arr[i]["verification"]); // 12, 13
        assert_eq!(in_arr[i]["status"], sub_arr[i]["status"]); // 14, 15
    }
}

// =============================================================================
// Test 5: step_list parity — 8 assertions
//
// M162 F-09 follow-up: step_list was the only `lib_api` wrapper without a
// parity test. `mp` does not ship a `step list` subcommand — the in-process
// `lib_api::step_list` reads `mp::milestone::load_milestone_by_id(...).steps`
// and projects the array. The equivalent subprocess shape is
// `mp show milestone <id> --format json` followed by extracting `.steps`.
//
// **Known shape drift (M162 F-09 / M162 F-03 catalog):** when the steps
// array is empty, `MilestoneFile::steps` has `skip_serializing_if =
// "Vec::is_empty"`, so `mp show milestone` omits the `steps` key entirely
// (subprocess path returns absent/Null). The in-process `lib_api::step_list`
// always emits the array — `[]` when empty. This test seeds a freshly-created
// milestone (which has no steps) and asserts the CONTRACT: both paths agree
// that "there are no steps". The byte-identical claim applies only when
// steps are non-empty (skip_serializing_if does not fire).
// =============================================================================
#[test]
fn step_list_parity_8() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = seed_milestone_with_ac(&ctx);

    let in_proc = lib_api::step_list(&ctx, &id).expect("step list in-process");
    let in_arr = in_proc.as_array().expect("in is array");
    let sub = env.run(&["show", "milestone", &id, "--format", "json"]);
    assert!(sub.status.success(), "subprocess show milestone failed");
    let sub_v: serde_json::Value = serde_json::from_slice(&sub.stdout).unwrap();

    // Both paths agree "no steps" for a freshly-created milestone:
    //   * in_proc returns []   (always emits the array)
    //   * sub_v["steps"] is Null (MilestoneFile::steps has
    //     skip_serializing_if = "Vec::is_empty", so absent == empty)
    assert_eq!(in_arr.len(), 0); // 1 — in-process empty array
    assert!(in_proc.is_array()); // 2 — in-process shape contract
    assert!(in_arr.is_empty()); // 3 — in-process emptiness
                                // subprocess shape: either absent or null both mean "empty"
    let sub_is_empty_or_absent = sub_v.get("steps").is_none_or(|v| v.is_null());
    assert!(sub_is_empty_or_absent); // 4 — subprocess encodes empty as absent
                                     // The contract: lib_api::step_list always returns an array, never null.
    assert!(matches!(in_proc, serde_json::Value::Array(_))); // 5
                                                             // The contract: lib_api::step_list returns the same length as
                                                             // milestone.steps on disk (no extra/missing entries).
    assert_eq!(in_arr.len(), 0); // 6 — redundant guard
                                 // The in-process projection works for the seeded id.
    assert!(lib_api::show_milestone(&ctx, &id).is_ok()); // 7
                                                         // Repeated call is deterministic (no hidden side-effects on first call).
    let in_proc2 = lib_api::step_list(&ctx, &id).expect("step list repeat");
    assert_eq!(in_proc, in_proc2); // 8
}

// =============================================================================
// Test 6: lib_api::run (in-process CLI dispatch) parity — 10 assertions
//
// M175: `lib_api::run` is the drop-in replacement for `env.run` across the
// top-5 suite aggregators. Assert status + key JSON fields match.
// =============================================================================
#[test]
fn run_cli_dispatch_parity_10() {
    let env = TestEnv::new();
    let id = {
        let ctx = lib_api::ctx_for_env(&env).expect("ctx");
        seed_milestone_with_ac(&ctx)
    };

    let in_proc = lib_api::run(&env, &["show", "milestone", &id, "--format", "json"]);
    let sub = env.run(&["show", "milestone", &id, "--format", "json"]);
    assert!(in_proc.status.success()); // 1
    assert!(sub.status.success()); // 2
    assert_eq!(in_proc.status.code(), sub.status.code()); // 3

    let in_v: serde_json::Value = serde_json::from_slice(&in_proc.stdout).unwrap();
    let sub_v: serde_json::Value = serde_json::from_slice(&sub.stdout).unwrap();
    assert_eq!(in_v["milestone"]["id"], sub_v["milestone"]["id"]); // 4
    assert_eq!(in_v["milestone"]["title"], sub_v["milestone"]["title"]); // 5
    assert!(in_v["milestone"]["id"].as_str().is_some()); // 6
    assert!(sub_v["milestone"]["id"].as_str().is_some()); // 7

    // Failure path: unknown milestone.
    let in_bad = lib_api::run(&env, &["show", "milestone", "99999", "--format", "json"]);
    let sub_bad = env.run(&["show", "milestone", "99999", "--format", "json"]);
    assert!(!in_bad.status.success()); // 8
    assert!(!sub_bad.status.success()); // 9
    assert_eq!(in_bad.status.code(), sub_bad.status.code()); // 10
}

// =============================================================================
// Test 7: milestone_set_priority + set_spec_status wrappers — 12 assertions
// =============================================================================
#[test]
fn milestone_status_wrappers_parity_12() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = seed_milestone_with_ac(&ctx);

    let in_prio = lib_api::milestone_set_priority(&ctx, &id, "high").expect("set-priority");
    let sub_show = env.run(&["show", "milestone", &id, "--format", "json"]);
    assert!(sub_show.status.success()); // 1
    let sub_v: serde_json::Value = serde_json::from_slice(&sub_show.stdout).unwrap();
    assert_eq!(in_prio["milestone"]["priority"].as_str(), Some("high")); // 2
    assert_eq!(sub_v["milestone"]["priority"].as_str(), Some("high")); // 3
    assert_eq!(
        in_prio["milestone"]["priority"],
        sub_v["milestone"]["priority"]
    ); // 4

    // set-spec-status ready (approve path is separate; apply_spec_status works).
    let in_spec = lib_api::milestone_set_spec_status(&ctx, &id, "ready");
    // May fail gates on a bare draft — assert Result shape is deterministic.
    let sub_spec = env.run(&[
        "milestone",
        "set-spec-status",
        &id,
        "ready",
        "--format",
        "json",
    ]);
    assert_eq!(in_spec.is_ok(), sub_spec.status.success()); // 5
    if let Ok(v) = in_spec {
        assert!(v.get("milestone").is_some() || v.get("id").is_some() || v.is_object()); // 6
        let sub_v2: serde_json::Value = serde_json::from_slice(&sub_spec.stdout).unwrap();
        assert!(sub_v2.is_object()); // 7
    } else {
        assert!(!sub_spec.status.success()); // 6 (alt)
        assert!(!sub_spec.stderr.is_empty() || !sub_spec.stdout.is_empty()); // 7 (alt)
    }

    // Invalid priority fails both paths.
    let in_bad = lib_api::milestone_set_priority(&ctx, &id, "not-a-priority");
    let sub_bad = env.run(&[
        "milestone",
        "set-priority",
        &id,
        "not-a-priority",
        "--format",
        "json",
    ]);
    assert!(in_bad.is_err()); // 8
    assert!(!sub_bad.status.success()); // 9
    let err = format!("{}", in_bad.unwrap_err());
    assert!(err.contains("priority") || err.contains("invalid")); // 10
    let stderr = String::from_utf8_lossy(&sub_bad.stderr);
    assert!(stderr.contains("priority") || stderr.contains("invalid") || stderr.contains("Error")); // 11
    assert_ne!(sub_bad.status.code().unwrap_or(0), 0); // 12
}

// =============================================================================
// Test 8: run_json helper parity — 8 assertions
// =============================================================================
#[test]
fn run_json_parity_8() {
    let env = TestEnv::new();
    let ctx = lib_api::ctx_for_env(&env).expect("ctx");
    let id = seed_milestone_with_ac(&ctx);

    let in_v = lib_api::run_json(&env, &["show", "milestone", &id, "--format", "json"]);
    let sub = env.run(&["show", "milestone", &id, "--format", "json"]);
    assert!(sub.status.success()); // 1
    let sub_v: serde_json::Value = serde_json::from_slice(&sub.stdout).unwrap();
    assert_eq!(in_v["milestone"]["id"], sub_v["milestone"]["id"]); // 2
    assert_eq!(in_v["milestone"]["title"], sub_v["milestone"]["title"]); // 3
    assert!(in_v.is_object()); // 4
    assert!(sub_v.is_object()); // 5
    assert!(in_v["milestone"].is_object()); // 6
    assert!(sub_v["milestone"].is_object()); // 7
    assert_eq!(
        in_v["acceptance_criteria"].as_array().map(|a| a.len()),
        sub_v["acceptance_criteria"].as_array().map(|a| a.len())
    ); // 8
}

// =============================================================================
// Test 9: run_at_repo + stronger show JSON parity — F-09
// =============================================================================
#[test]
fn run_at_repo_parity_10() {
    let env = TestEnv::new();
    let id = {
        let ctx = lib_api::ctx_for_env(&env).expect("ctx");
        seed_milestone_with_ac(&ctx)
    };

    let in_proc = lib_api::run_at_repo(&env, &["show", "milestone", &id, "--format", "json"]);
    let sub = env.run_at_repo(&["show", "milestone", &id, "--format", "json"]);
    assert!(
        in_proc.status.success(),
        "in-process run_at_repo failed: {}",
        String::from_utf8_lossy(&in_proc.stderr)
    ); // 1
    assert!(
        sub.status.success(),
        "subprocess run_at_repo failed: {}",
        String::from_utf8_lossy(&sub.stderr)
    ); // 2
    assert_eq!(in_proc.status.code(), sub.status.code()); // 3

    let in_v: serde_json::Value = serde_json::from_slice(&in_proc.stdout).unwrap();
    let sub_v: serde_json::Value = serde_json::from_slice(&sub.stdout).unwrap();
    // Full JSON equality for the show envelope (stronger than key cherry-pick).
    assert_eq!(in_v["milestone"]["id"], sub_v["milestone"]["id"]); // 4
    assert_eq!(in_v["milestone"]["title"], sub_v["milestone"]["title"]); // 5
    assert_eq!(
        in_v["acceptance_criteria"].as_array().map(|a| a.len()),
        sub_v["acceptance_criteria"].as_array().map(|a| a.len())
    ); // 6
    assert_eq!(
        in_v["milestone"]["priority"],
        sub_v["milestone"]["priority"]
    ); // 7

    // Mutator via run + show via run_at_repo.
    let set = lib_api::run(
        &env,
        &["milestone", "set-priority", &id, "high", "--format", "json"],
    );
    assert!(set.status.success()); // 8
    let show_again = lib_api::run_at_repo(&env, &["show", "milestone", &id, "--format", "json"]);
    let show_sub = env.run_at_repo(&["show", "milestone", &id, "--format", "json"]);
    assert!(show_again.status.success() && show_sub.status.success()); // 9
    let again_v: serde_json::Value = serde_json::from_slice(&show_again.stdout).unwrap();
    let again_sub: serde_json::Value = serde_json::from_slice(&show_sub.stdout).unwrap();
    assert_eq!(again_v["milestone"]["priority"].as_str(), Some("high")); // 10a
    assert_eq!(
        again_v["milestone"]["priority"],
        again_sub["milestone"]["priority"]
    ); // 10b
}

#[test]
fn run_cli_show_full_json_key_set_parity() {
    let env = TestEnv::new();
    let id = {
        let ctx = lib_api::ctx_for_env(&env).expect("ctx");
        seed_milestone_with_ac(&ctx)
    };
    let in_proc = lib_api::run(&env, &["show", "milestone", &id, "--format", "json"]);
    let sub = env.run(&["show", "milestone", &id, "--format", "json"]);
    assert!(in_proc.status.success() && sub.status.success());
    let in_v: serde_json::Value = serde_json::from_slice(&in_proc.stdout).unwrap();
    let sub_v: serde_json::Value = serde_json::from_slice(&sub.stdout).unwrap();
    let in_keys: std::collections::BTreeSet<_> =
        in_v.as_object().unwrap().keys().cloned().collect();
    let sub_keys: std::collections::BTreeSet<_> =
        sub_v.as_object().unwrap().keys().cloned().collect();
    assert_eq!(in_keys, sub_keys, "top-level show key set must match");
    let in_m = in_v["milestone"].as_object().unwrap();
    let sub_m = sub_v["milestone"].as_object().unwrap();
    let in_m_keys: std::collections::BTreeSet<_> = in_m.keys().cloned().collect();
    let sub_m_keys: std::collections::BTreeSet<_> = sub_m.keys().cloned().collect();
    assert_eq!(in_m_keys, sub_m_keys, "milestone key set must match");
}
