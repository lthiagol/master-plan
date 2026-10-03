//! Shared seeding helpers for tests that need a handoff baseline.
//!
//! Previously duplicated in:
//!   - crates/mp/tests/suites/plan_diff.rs:33
//!   - crates/mp/tests/suites/digest.rs:6
//!
//! Both versions were byte-identical except for the call mechanism
//! (`env.run` vs `lib_api::run`); this shared copy picks the in-process
//! path so callers don't have to import `lib_api` purely for setup.
//! The two callers (`suites/plan_diff.rs`, `suites/digest.rs`) now
//! `use crate::common::seed::seed_handoff_gate;`.

use crate::common::lib_api;

/// Seed the test plan with a minimal approved+decomposed milestone `01`,
/// so subsequent handoff / plan-diff / digest calls have a baseline
/// "last handoff" to diff against.
pub fn seed_handoff_gate(env: &crate::common::TestEnv) {
    let create_json = r#"{
        "title": "Handoff gate",
        "intent": { "outcome": "Enable handoff." },
        "problem": { "description": "Need handoff gate." },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [
            { "description": "works", "verification": "manual: ok" }
        ]
    }"#;
    assert!(
        lib_api::run(env, &["milestone", "create", "--json", create_json])
            .status
            .success(),
        "create handoff gate"
    );
    assert!(lib_api::run(env, &["milestone", "approve", "01"])
        .status
        .success());
    assert!(lib_api::run(env, &["milestone", "decompose", "01"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "step",
            "add",
            "01",
            "--wp",
            "WP1",
            "--action",
            "step",
            "--done-when",
            "done",
            "--tests",
            "manual: ok",
            "--covers-ac",
            "AC-01",
        ])
        .status
        .success());
}
