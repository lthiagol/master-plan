//! M147 AC-03: `auto_remediate` threshold semantics — the value names
//! the MINIMUM severity to auto-remediate; ordering is
//! `none < low < medium < high`, with `all` aliasing `low`.
//!
//! The `should_remediate(sev)` helper on [`AgentAutomationConfig`] is
//! what the `mp-coordinator` skill reads at stage 8 (external review)
//! and stage 9 (remediation); it MUST be the single source of truth for
//! "act or record" so the threshold interpretation cannot drift between
//! the CLI, the test suite, and the skill contract.

mod common;

use crate::common::TestEnv;
use mp::config::{AgentAutomationConfig, SeverityRank, AUTO_REMEDIATE_VALUES};
use serde_json::Value;
use std::fs;

fn config_path(env: &TestEnv) -> std::path::PathBuf {
    env.tmp.path().join("master-plan/config.json")
}

/// Helper: re-read the project's agent.automation.auto_remediate via
/// the official accessor (the one agents consume at runtime). Returns
/// the deserialized [`AgentAutomationConfig`] if the field is present.
fn load_automation(env: &TestEnv) -> AgentAutomationConfig {
    let cfg_path = config_path(env);
    let v: Value = serde_json::from_slice(&fs::read(&cfg_path).unwrap()).unwrap();
    serde_json::from_value(v["agent"]["automation"].clone()).unwrap()
}

/// Severity ordering: `none < low < medium < high` — loaded via
/// `SeverityRank::from_config_value` so a regression here breaks the
/// ordering contract before `should_remediate` even runs.
#[test]
fn auto_remediate_threshold_severity_rank_ordering_is_locked() {
    let none = SeverityRank::from_config_value("none");
    let low = SeverityRank::from_config_value("low");
    let medium = SeverityRank::from_config_value("medium");
    let high = SeverityRank::from_config_value("high");
    let all = SeverityRank::from_config_value("all");

    assert!(none < low, "none must sort below low");
    assert!(low < medium, "low must sort below medium");
    assert!(medium < high, "medium must sort below high");
    assert_eq!(
        all, low,
        "`all` is an alias for `low` so configs read clearly; must be equal"
    );
}

/// Threshold semantics (the headline test): every config value x
/// finding-severity pair maps to the documented boolean. This is the
/// `done_when` test named in the milestone's S2 step.
#[test]
fn auto_remediate_threshold_should_remediate_truth_table() {
    let cases: &[(&str, &[(&str, bool)])] = &[
        // threshold = "none" — record only, never remediate
        (
            "none",
            &[("low", false), ("medium", false), ("high", false)],
        ),
        // threshold = "low" (or "all") — remediate all severities
        ("low", &[("low", true), ("medium", true), ("high", true)]),
        ("all", &[("low", true), ("medium", true), ("high", true)]),
        // threshold = "medium" — leave low as record-only
        (
            "medium",
            &[("low", false), ("medium", true), ("high", true)],
        ),
        // threshold = "high" — only the strongest findings
        ("high", &[("low", false), ("medium", false), ("high", true)]),
    ];
    for (threshold, expectations) in cases {
        for (severity, expected) in expectations.iter() {
            let cfg = AgentAutomationConfig {
                auto_remediate: Some((*threshold).to_string()),
                ..Default::default()
            };
            assert_eq!(
                cfg.should_remediate(severity),
                *expected,
                "threshold={threshold:?} severity={severity:?} should be {expected}; \
                 AUTO_REMEDIATE_VALUES={AUTO_REMEDIATE_VALUES:?}"
            );
        }
    }
}

/// Unknown severity strings (from a finding log with a stale or
/// typo'd label) must NOT trigger auto-remediation. Returning `false`
/// is the safe default — at worst the operator files one round-trip
/// manually; at worst `true` would silently apply the remediation loop
/// to findings the project never asked for.
#[test]
fn auto_remediate_threshold_unknown_severity_treated_as_record_only() {
    for threshold in ["low", "medium", "high"] {
        let cfg = AgentAutomationConfig {
            auto_remediate: Some(threshold.to_string()),
            ..Default::default()
        };
        for unknown in ["urgent", "blocker", "info", "", "LOW"] {
            assert!(
                !cfg.should_remediate(unknown),
                "threshold={threshold:?} finding={unknown:?} must be record-only \
                 (unknown severity never auto-remediates)"
            );
        }
    }
}

/// Unknown threshold values (defense in depth — `set` already rejects
/// them via `AUTO_REMEDIATE_VALUES`, but `should_remediate` may be
/// called on hand-edited configs) must default to "record only".
/// Coverage mirrors the headline case: every severity is tested.
#[test]
fn auto_remediate_threshold_unknown_threshold_treated_as_record_only() {
    let cfg = AgentAutomationConfig {
        auto_remediate: Some("bogus".to_string()),
        ..Default::default()
    };
    for severity in ["low", "medium", "high"] {
        assert!(
            !cfg.should_remediate(severity),
            "unknown threshold must not auto-remediate {severity:?}"
        );
    }
    // Explicit `"none"` threshold must equal the unset threshold for
    // every severity (not just `high`). A regression where
    // `auto_remediate_threshold()` returned a different rank for the
    // `None` sentinel vs the `"none"` string would slip past a
    // single-severity check — exercise all three.
    let explicit_none = AgentAutomationConfig {
        auto_remediate: Some("none".to_string()),
        ..Default::default()
    };
    for severity in ["low", "medium", "high"] {
        assert!(
            !explicit_none.should_remediate(severity),
            "auto_remediate=\"none\" with severity={severity:?} must be record-only"
        );
    }

    // Unset threshold behaves identically to `"none"` for every
    // severity, and the rank accessor confirms both paths.
    let blank = AgentAutomationConfig::default();
    for severity in ["low", "medium", "high"] {
        assert!(
            !blank.should_remediate(severity),
            "unset auto_remediate with severity={severity:?} must be record-only"
        );
    }
    assert_eq!(blank.auto_remediate_threshold(), SeverityRank::None);
    assert_eq!(
        explicit_none.auto_remediate_threshold(),
        SeverityRank::None,
        "auto_remediate=\"none\" and unset must parse to the same SeverityRank"
    );
}

/// End-to-end check: setting `auto_remediate = "medium"` via the CLI
/// makes `should_remediate("low")` return `false` for the agent that
/// reads the config. This is the contract the `mp-coordinator` skill
/// relies on at stage 8 — if the round-trip breaks, the threshold
/// silently becomes a no-op or, worse, an always-on trigger.
#[test]
fn auto_remediate_threshold_round_trip_via_mp_config() {
    let env = TestEnv::new();
    let out = env.run(&[
        "config",
        "set",
        "agent.automation.auto_remediate",
        "medium",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let cfg = load_automation(&env);
    assert_eq!(cfg.auto_remediate.as_deref(), Some("medium"));
    assert_eq!(cfg.auto_remediate_threshold(), SeverityRank::Medium);

    // medium threshold → low is record-only, medium and high remediate.
    assert!(!cfg.should_remediate("low"));
    assert!(cfg.should_remediate("medium"));
    assert!(cfg.should_remediate("high"));
}
