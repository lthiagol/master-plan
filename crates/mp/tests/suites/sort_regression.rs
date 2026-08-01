//! M104 AC-01 (B-42): every mp CLI surface that emits milestone ids in a
//! list/order must place 100, 101, 102... after 99 (numeric), not between 10
//! and 11 (lexicographic).
//!
//! Fixture: `tests/fixtures/projects/sort_regression/` has milestones
//! `09, 10, 11, 99, 100, 101`. Lex sort would emit
//! `[09, 10, 100, 101, 11, 99]`; numeric sort emits
//! `[09, 10, 11, 99, 100, 101]`. This test exercises four user-visible CLI
//! surfaces and asserts numeric ordering on each.

use crate::common::TestEnv;
use std::cmp::Ordering;

/// Numeric sort key mirroring `paths::compare_milestone_ids`: parse id into
/// `Vec<u32>` honoring dotted sub-ids, strip optional `M`/`m` prefix.
fn sort_key(id: &str) -> Vec<u32> {
    id.trim_start_matches(['M', 'm'])
        .split('.')
        .filter_map(|p| p.parse().ok())
        .collect()
}

/// A sequence is numerically sorted iff adjacent pairs (a, b) satisfy a <= b
/// under the `sort_key` comparator. Empty / single-element sequences pass.
fn is_numeric_sorted(ids: &[&str]) -> bool {
    ids.windows(2).all(|w| {
        let ka = sort_key(w[0]);
        let kb = sort_key(w[1]);
        ka.cmp(&kb) != Ordering::Greater
    })
}

#[test]
fn list_milestones_orders_ids_numerically() {
    let env = TestEnv::from_fixture("sort_regression");

    let out = env.run(&["list", "milestones", "--format", "json"]);
    assert!(
        out.status.success(),
        "mp list milestones failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let ids: Vec<String> = value["milestones"]
        .as_array()
        .expect("milestones is array")
        .iter()
        .map(|m| m["id"].as_str().unwrap_or("").to_string())
        .collect();
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    assert_eq!(
        ids,
        vec!["09", "10", "11", "99", "100", "101"],
        "milestones order; got {ids:?}"
    );
    assert!(
        is_numeric_sorted(&id_refs),
        "milestones not in numeric order: {ids:?}"
    );
}

#[test]
fn list_steps_orders_milestones_numerically() {
    let env = TestEnv::from_fixture("sort_regression");

    let out = env.run(&["list", "steps", "--format", "json"]);
    assert!(
        out.status.success(),
        "mp list steps failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    // mp list steps emits { milestone, milestone_display, step } rows; dedup
    // while preserving order to get the milestone id sequence.
    let mut seen: Vec<String> = Vec::new();
    for row in value["steps"].as_array().expect("steps is array") {
        let mid = row["milestone"].as_str().unwrap_or("").to_string();
        if !seen.last().map(|l| l == &mid).unwrap_or(false) {
            seen.push(mid);
        }
    }
    assert_eq!(
        seen,
        vec!["09", "10", "11", "99", "100", "101"],
        "steps ordering by milestone; got {seen:?}"
    );
    let refs: Vec<&str> = seen.iter().map(String::as_str).collect();
    assert!(
        is_numeric_sorted(&refs),
        "steps not in numeric order: {seen:?}"
    );
}

#[test]
fn path_baseline_orders_milestones_numerically() {
    let env = TestEnv::from_fixture("sort_regression");

    let out = env.run(&["path", "--format", "json"]);
    assert!(
        out.status.success(),
        "mp path failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let baseline: Vec<String> = value["baseline_milestone_order"]
        .as_array()
        .expect("baseline_milestone_order is array")
        .iter()
        .map(|v| v.as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        baseline,
        vec!["09", "10", "11", "99", "100", "101"],
        "mp path baseline order; got {baseline:?}"
    );
    let refs: Vec<&str> = baseline.iter().map(String::as_str).collect();
    assert!(
        is_numeric_sorted(&refs),
        "mp path baseline not in numeric order: {baseline:?}"
    );
}

#[test]
fn reviews_pending_orders_milestones_numerically() {
    let env = TestEnv::from_fixture("sort_regression");

    let out = env.run(&["reviews", "status", "--format", "json"]);
    assert!(
        out.status.success(),
        "mp reviews status failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pending: Vec<String> = value["execution_review"]["pending"]
        .as_array()
        .expect("execution_review.pending is array")
        .iter()
        .map(|v| v["milestone_id"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        pending,
        vec!["09", "10", "11", "99", "100", "101"],
        "review pending order; got {pending:?}"
    );
    let refs: Vec<&str> = pending.iter().map(String::as_str).collect();
    assert!(
        is_numeric_sorted(&refs),
        "review pending not in numeric order: {pending:?}"
    );
}

#[test]
fn numeric_sort_key_round_trips_through_100_boundary() {
    // Belt-and-suspenders: probe the sort key directly so a regression in the
    // sort comparator (independent of any caller) fails this test loudly.
    let pairs: &[(&str, &str, Ordering)] = &[
        ("09", "10", Ordering::Less),
        ("10", "11", Ordering::Less),
        ("11", "99", Ordering::Less),
        ("99", "100", Ordering::Less),
        ("100", "101", Ordering::Less),
        ("101", "102", Ordering::Less),
        // Lex would have given: 100 < 11 < 99 < 10. Numeric inverts all three.
        ("100", "11", Ordering::Greater),
        ("100", "10", Ordering::Greater),
        ("10", "100", Ordering::Less),
    ];
    for (a, b, expected) in pairs {
        let actual = sort_key(a).cmp(&sort_key(b));
        assert_eq!(
            actual, *expected,
            "sort_key({a:?}, {b:?}) = {actual:?}, expected {expected:?}"
        );
    }
}

// M107 AC-04 (S5): regression coverage for `mp plan diff`'s
// `changed_milestones` output ordering. The pre-M104 lex-sort bug
// emitted `[09, 10, 100, 101, 11, 99]` here (because the
// `changed_milestones` array was sorted via `out.sort()` lexicographic
// at the time). M104 AC-01 fixed the source-level comparator, but
// `crates/mp/src/plan_diff.rs` has *four* `out.sort_by(|a, b|
// paths::compare_milestone_ids(...))` sites (lines ~170, ~176, ~349,
// ~410, ~451 per the M104 design_audit) and any new site that uses
// plain `.sort()` would regress silently. This test catches that.
#[test]
fn plan_diff_orders_milestones_numerically() {
    let env = TestEnv::from_fixture("sort_regression");

    // `--since 1970-01-01` gives us "all milestones are changed" so we
    // get the full id sequence in the output, regardless of whether
    // any handoff has been recorded. The output list is the canonical
    // surface we want to validate.
    let out = env.run(&["plan", "diff", "--since", "1970-01-01", "--format", "json"]);
    assert!(
        out.status.success(),
        "mp plan diff failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let ids: Vec<String> = value["changed_milestones"]
        .as_array()
        .expect("changed_milestones is array")
        .iter()
        .map(|m| m["id"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["09", "10", "11", "99", "100", "101"],
        "plan diff changed_milestones order; got {ids:?}"
    );
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    assert!(
        is_numeric_sorted(&refs),
        "plan diff changed_milestones not in numeric order: {ids:?}"
    );
}
