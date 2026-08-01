//! M97 — read ergonomics parity tests.
//!
//! Covers: `--fields` projection on `path` / `inbox` (AC-01/02), `--summary`
//! on `status` / `reviews finding list` / `reviews lifecycle` (AC-03/04/05),
//! and the cross-command parity regression (unknown `--fields` path is a
//! hard error everywhere — AC-06). Also covers the design_decisions
//! create-round-trip / update-rejection (AC-10/AC-11).

use crate::common::TestEnv;

fn create_milestone(env: &TestEnv) -> String {
    let json = r#"{
        "title": "Read Ergonomics Fixture",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "Fixture for read ergonomics tests" },
        "problem": { "description": "Fixture." },
        "scope": {
            "in_scope": ["fixture"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            {
                "description": "AC1",
                "verification": "cargo test"
            }
        ],
        "design_decisions": [
            {
                "area": "core",
                "choice": "decide A over B",
                "rationale": "perf"
            }
        ]
    }"#;

    let out = env.run(&["milestone", "create", "--json", json, "--format", "json"]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// AC-01: mp path --fields
// ---------------------------------------------------------------------------

#[test]
fn path_fields_projects_slice() {
    let env = TestEnv::new();
    let _id = create_milestone(&env);

    let out = env.run(&["path", "--fields", "strategy,ready_milestones"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["strategy"].is_string(), "projected strategy: {v}");
    assert!(
        v["ready_milestones"].is_array(),
        "projected ready_milestones: {v}"
    );
    // unprojected top-level keys are absent
    assert!(
        v.get("actions").is_none(),
        "projection must omit unrequested keys: {v}"
    );
}

#[test]
fn path_fields_unknown_is_hard_error() {
    let env = TestEnv::new();
    create_milestone(&env);

    let out = env.run(&["path", "--fields", "bogus.path"]);
    assert!(
        !out.status.success(),
        "expected non-zero exit, got {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("unknown path"),
        "stderr should mention unknown path: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// AC-02: mp inbox --fields
// ---------------------------------------------------------------------------

#[test]
fn inbox_fields_projects_slice() {
    let env = TestEnv::new();
    let _id = create_milestone(&env);

    let out = env.run(&["inbox", "--fields", "count"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["count"].is_number(), "projected count: {v}");
}

#[test]
fn inbox_fields_unknown_is_hard_error() {
    let env = TestEnv::new();
    create_milestone(&env);

    let out = env.run(&["inbox", "--fields", "bogus"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("unknown path"),
        "stderr should mention unknown path: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// AC-03: mp status --summary
// ---------------------------------------------------------------------------

#[test]
fn status_summary_omits_path_block() {
    let env = TestEnv::new();
    create_milestone(&env);

    let out = env.run(&["status", "--summary"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // headline metrics present
    assert!(v["planning_status"].is_string());
    assert!(
        v["planning_phase"].is_string(),
        "summary must include planning_phase: {v}"
    );
    assert!(v["milestones"]["total"].is_number());
    assert!(v["execution"]["mode"].is_string());
    // path block / suggested_path nesting absent
    assert!(
        v.get("suggested_path").is_none(),
        "summary must not nest suggested_path: {v}"
    );
    let suggested = v.get("suggested_path");
    assert!(suggested.is_none());
}

// ---------------------------------------------------------------------------
// AC-04: mp reviews finding list --summary
// ---------------------------------------------------------------------------

#[test]
fn reviews_finding_list_summary_counts_only() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = env.run(&["reviews", "finding", "list", &id, "--summary"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let summary = &v["summary"];
    assert!(summary["open"].is_number(), "summary.open: {v}");
    assert!(summary["fixed"].is_number());
    assert!(summary["total"].is_number());
    // Spec AC-04: summary is exactly {open, fixed, total} — no `other` leak.
    let keys: Vec<&str> = summary
        .as_object()
        .map(|o| o.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();
    assert!(
        !keys.contains(&"other"),
        "summary must not leak 'other': {summary}"
    );
    // results array absent in summary mode
    assert!(
        v.get("findings").is_none(),
        "summary must omit the findings array: {v}"
    );
}

// ---------------------------------------------------------------------------
// AC-05: mp reviews lifecycle --summary
// ---------------------------------------------------------------------------

#[test]
fn reviews_lifecycle_summary_buckets_only() {
    let env = TestEnv::new();
    create_milestone(&env);

    let out = env.run(&["reviews", "lifecycle", "--summary"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let buckets = v["lifecycle"].as_array().expect("lifecycle array");
    for b in buckets {
        assert!(b["review_state"].is_string(), "review_state: {b}");
        assert!(b["count"].is_number(), "count: {b}");
        // no milestone detail leaked in summary mode
        assert!(
            b.get("milestones").is_none(),
            "summary must omit milestones detail: {b}"
        );
    }
}

// ---------------------------------------------------------------------------
// AC-06: parity regression — unknown --fields path is a hard error everywhere
// ---------------------------------------------------------------------------

#[test]
fn unknown_fields_path_hard_error_all_read_commands() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    // Each entry: (args, label). Every read command that accepts --fields,
    // including the `path` read subcommands (list-pins, suggest). Write
    // subcommands (pin/unpin/focus/clear-focus) are intentionally excluded.
    let cases: &[&[&str]] = &[
        &["status", "--fields", "nope.nope"],
        &["path", "--fields", "nope.nope"],
        &["path", "list-pins", "--fields", "nope"],
        &["path", "suggest", "--fields", "nope.nope"],
        &["inbox", "--fields", "nope"],
        &["list", "milestones", "--fields", "nope.nope"],
        &["reviews", "finding", "list", &id, "--fields", "nope.nope"],
        &["reviews", "lifecycle", "--fields", "nope.nope"],
        &["show", "milestone", &id, "--fields", "nope.nope"],
    ];

    for args in cases {
        let out = env.run(args);
        let label = args.join(" ");
        assert!(
            !out.status.success(),
            "expected non-zero exit for `{label}` (got {:?})",
            out.status
        );
        let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
        assert!(
            stderr.contains("unknown path"),
            "`{label}` stderr should mention unknown path: {stderr}"
        );
    }
}

// ---------------------------------------------------------------------------
// AC-10: design_decisions round-trips through create
// ---------------------------------------------------------------------------

#[test]
fn design_decisions_round_trips_at_create() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = env.run(&["show", "milestone", &id]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let dd = v["design_decisions"]
        .as_array()
        .expect("design_decisions array");
    assert_eq!(dd.len(), 1, "design_decisions should be populated: {v}");
    assert_eq!(dd[0]["choice"].as_str().unwrap(), "decide A over B");
}

// ---------------------------------------------------------------------------
// AC-11: design_decisions rejected on update (fragment-first preserved)
// ---------------------------------------------------------------------------

#[test]
fn design_decisions_rejected_on_update() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let payload = r#"{"design_decisions":[{"area":"","choice":"x","rationale":"y"}]}"#;
    let out = env.run(&["milestone", "update", &id, "--json", payload]);
    assert!(
        !out.status.success(),
        "update with design_decisions must fail: {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
    assert!(
        stderr.contains("design-decision"),
        "stderr should hint at design-decision add: {stderr}"
    );
}
