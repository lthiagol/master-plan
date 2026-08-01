//! M93 AC-09: end-to-end grooming scenario exercising the fragment-first flow:
//!
//!   1. ac add          → new AC appears in list
//!   2. step add covers AC → AC now has a covering step
//!   3. ac remove       → guarded, refuses because step covers it
//!   4. step remove     → succeeds once the covering step is gone
//!   5. ac remove       → succeeds, AC gone
//!
//! Throughout, mp validate must remain green and no `milestone update --json`
//! array rebuild is used.

use crate::common::{lib_api, TestEnv};

#[test]
fn groom_flow_uses_only_fragment_commands() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Baseline: validate green.
    assert!(lib_api::run_validate(&env), "baseline validate failed");

    // 1. ac add — a brand-new acceptance criterion.
    let add = lib_api::run(
        &env,
        &[
            "milestone",
            "criterion",
            "add",
            "03",
            "--description",
            "Transient criterion (will be removed)",
            "--verification",
            "crates/mp/tests/fragment_groom_scenario.rs",
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
    let new_ac = added["acceptance_criterion"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // validate still green.
    assert!(lib_api::run_validate(&env), "validate after ac add failed");

    // 2. step add covers the new AC.
    let step_add = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            "03",
            "--wp",
            "WP1",
            "--id",
            "S77",
            "--action",
            "Covers the transient AC",
            "--tests",
            "manual: fragment_first_groom",
            "--done-when",
            "Step exists and references the AC",
            "--covers-ac",
            &new_ac,
            "--format",
            "json",
        ],
    );
    assert!(
        step_add.status.success(),
        "step add failed: {}",
        String::from_utf8_lossy(&step_add.stderr)
    );
    assert!(
        lib_api::run_validate(&env),
        "validate after step add failed"
    );

    // 3. ac remove — guarded, must fail because S77 covers it.
    let blocked = lib_api::run(
        &env,
        &[
            "milestone",
            "ac",
            "remove",
            "03",
            &new_ac,
            "--format",
            "json",
        ],
    );
    assert!(
        !blocked.status.success(),
        "ac remove must be blocked while a step covers the AC"
    );
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(
        stderr.contains(&new_ac) && stderr.contains("S77"),
        "guard error must mention AC and covering step; got: {stderr}"
    );
    assert!(
        lib_api::run_validate(&env),
        "validate after blocked ac remove failed"
    );

    // 4. step remove — succeeds (no deps point at S77).
    let step_remove = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "remove",
            "03",
            "S77",
            "--format",
            "json",
        ],
    );
    assert!(
        step_remove.status.success(),
        "step remove failed: {}",
        String::from_utf8_lossy(&step_remove.stderr)
    );
    let removed: serde_json::Value = serde_json::from_slice(&step_remove.stdout).unwrap();
    assert_eq!(removed["ok"], true);
    assert_eq!(removed["removed"], "S77");
    assert!(
        lib_api::run_validate(&env),
        "validate after step remove failed"
    );

    // 5. ac remove — now succeeds.
    let ac_remove = lib_api::run(
        &env,
        &[
            "milestone",
            "ac",
            "remove",
            "03",
            &new_ac,
            "--format",
            "json",
        ],
    );
    assert!(
        ac_remove.status.success(),
        "ac remove (after step removal) failed: {}",
        String::from_utf8_lossy(&ac_remove.stderr)
    );
    let ac_removed: serde_json::Value = serde_json::from_slice(&ac_remove.stdout).unwrap();
    assert_eq!(ac_removed["ok"], true);
    assert_eq!(ac_removed["removed"], new_ac);

    // Final state: AC is gone, S77 is gone.
    let show_ac = lib_api::run(
        &env,
        &["milestone", "ac", "show", "03", &new_ac, "--format", "json"],
    );
    assert!(
        !show_ac.status.success(),
        "removed AC must not be retrievable"
    );
    let show_step = lib_api::run(
        &env,
        &["milestone", "step", "show", "03", "S77", "--format", "json"],
    );
    assert!(
        !show_step.status.success(),
        "removed step must not be retrievable"
    );

    // Final validate green — full plan still consistent.
    assert!(lib_api::run_validate(&env), "final validate failed");
}
