use crate::common::lib_api;
use crate::common::TestEnv;

fn create_milestone(
    env: &TestEnv,
    title: &str,
    priority: &str,
    spec_status: &str,
    depends_on: &[&str],
) -> String {
    let deps_json = serde_json::Value::Array(
        depends_on
            .iter()
            .map(|d| serde_json::Value::String(d.to_string()))
            .collect(),
    );
    let json = serde_json::json!({
        "title": title,
        "depends_on": deps_json,
        "effort": "S",
        "risk": "low",
        "priority": priority,
        "intent": { "outcome": "Bulk test fixture" },
        "problem": { "description": "Used by milestone_bulk.rs integration tests." },
        "scope": {
            "in_scope": ["bulk ops"],
            "out_of_scope": ["x", "y"]
        },
        "acceptance_criteria": [
            {"description": "AC1", "verification": "manual: ok"}
        ],
        "spec_status": spec_status,
    });
    let json_str = serde_json::to_string(&json).unwrap();
    let out = lib_api::run(
        env,
        &[
            "milestone",
            "create",
            "--json",
            &json_str,
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

fn show_priority(env: &TestEnv, id: &str) -> Option<String> {
    let out = lib_api::run(
        env,
        &[
            "show",
            "milestone",
            id,
            "--fields",
            "milestone.priority",
            "--format",
            "json",
        ],
    );
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["priority"].as_str().map(|s| s.to_string())
}

fn show_depends_on(env: &TestEnv, id: &str) -> Vec<String> {
    let out = lib_api::run(
        env,
        &[
            "show",
            "milestone",
            id,
            "--fields",
            "milestone.depends_on",
            "--format",
            "json",
        ],
    );
    if !out.status.success() {
        return Vec::new();
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["depends_on"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

// AC-01: bulk set-priority --ids updates all targets with per-id results.
#[test]
fn bulk_set_priority_by_ids() {
    let env = TestEnv::new();
    let id_a = create_milestone(&env, "Bulk A", "normal", "draft", &[]);
    let id_b = create_milestone(&env, "Bulk B", "normal", "draft", &[]);
    let id_c = create_milestone(&env, "Bulk C", "normal", "draft", &[]);

    let ids_arg = format!("{},{},{}", id_a, id_b, id_c);
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &ids_arg,
            "--priority",
            "high",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["succeeded"].as_u64().unwrap(), 3);
    assert_eq!(v["failed"].as_u64().unwrap(), 0);
    assert_eq!(v["target_count"].as_u64().unwrap(), 3);
    assert_eq!(
        v["succeeded"].as_u64().unwrap() + v["failed"].as_u64().unwrap(),
        v["target_count"].as_u64().unwrap(),
        "succeeded + failed must equal target_count (live run)"
    );
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    for r in results {
        assert_eq!(r["ok"], true);
        assert_eq!(r["after"], "high");
    }

    for id in [&id_a, &id_b, &id_c] {
        assert_eq!(show_priority(&env, id).as_deref(), Some("high"));
    }
}

// AC-02: bulk set-priority --where resolves targets via the same filter as list.
#[test]
fn bulk_set_priority_by_where_filter() {
    let env = TestEnv::new();
    let id_high = create_milestone(&env, "High", "high", "draft", &[]);
    let id_normal = create_milestone(&env, "Normal", "normal", "draft", &[]);

    // Move both to ready so the only differentiator is priority.
    let _ = lib_api::run(&env, &["milestone", "set-spec-status", &id_high, "ready"]);
    let _ = lib_api::run(&env, &["milestone", "set-spec-status", &id_normal, "ready"]);

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--where",
            "priority==normal",
            "--priority",
            "urgent",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["succeeded"].as_u64().unwrap(), 1);
    assert_eq!(v["results"][0]["id"], id_normal);
    assert_eq!(v["results"][0]["after"], "urgent");

    assert_eq!(show_priority(&env, &id_high).as_deref(), Some("high"));
    assert_eq!(show_priority(&env, &id_normal).as_deref(), Some("urgent"));
}

// AC-01 (negative): one of the ids is bogus; others still succeed; failed > 0;
// exit code is non-zero; per-id results include the failure.
#[test]
fn bulk_set_priority_partial_failure_continues() {
    let env = TestEnv::new();
    let id_real = create_milestone(&env, "Real", "normal", "draft", &[]);
    let bogus = "M9999";
    let ids_arg = format!("{},{}", id_real, bogus);

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &ids_arg,
            "--priority",
            "high",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success(), "should fail when any id fails");
    assert_eq!(
        out.status.code(),
        Some(2),
        "bulk partial failure must exit 2, not anyhow's 1"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["succeeded"].as_u64().unwrap(), 1);
    assert_eq!(v["failed"].as_u64().unwrap(), 1);
    let failed_row = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["ok"] == false)
        .expect("expected at least one failed row");
    assert!(failed_row["error"].is_string());

    // Real milestone was still updated.
    assert_eq!(show_priority(&env, &id_real).as_deref(), Some("high"));
}

// AC-02 (set-spec-status): bulk set-spec-status works with --ids and --where.
#[test]
fn bulk_set_spec_status_by_ids_and_where() {
    let env = TestEnv::new();
    let id_a = create_milestone(&env, "Spec A", "normal", "draft", &[]);
    let id_b = create_milestone(&env, "Spec B", "normal", "draft", &[]);
    let id_c = create_milestone(&env, "Spec C", "normal", "draft", &[]);

    let ids_arg = format!("{},{}", id_a, id_b);
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-spec-status",
            "--ids",
            &ids_arg,
            "--status",
            "review",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["succeeded"].as_u64().unwrap(), 2);

    // now use --where
    let out2 = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-spec-status",
            "--where",
            "priority==normal",
            "--status",
            "review",
            "--format",
            "json",
        ],
    );
    assert!(out2.status.success());
    let v2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    // 3 milestones total (id_a, id_b, id_c) all share priority=normal
    assert_eq!(v2["succeeded"].as_u64().unwrap(), 3);
    let _ = id_c;
}

// AC-03: bulk depends-on add appends; remove drops; cycles rejected per-id.
#[test]
fn bulk_depends_on_add_remove_and_cycle() {
    let env = TestEnv::new();
    let id_parent = create_milestone(&env, "Parent", "normal", "draft", &[]);
    let id_child_a = create_milestone(&env, "Child A", "normal", "draft", &[id_parent.as_str()]);
    let id_child_b = create_milestone(&env, "Child B", "normal", "draft", &[id_child_a.as_str()]);

    // Bulk add parent to child_b (child_a already has parent → no-op).
    let ids_arg = format!("{},{}", id_child_a, id_child_b);
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "depends-on",
            "add",
            "--ids",
            &ids_arg,
            "--depends-on",
            &id_parent,
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["succeeded"].as_u64().unwrap(), 2);

    // child_a: still just [parent]
    let deps_a = show_depends_on(&env, &id_child_a);
    assert_eq!(deps_a, vec![id_parent.clone()]);

    // child_b: now [child_a, parent]
    let mut deps_b = show_depends_on(&env, &id_child_b);
    let mut expected_b = vec![id_child_a.clone(), id_parent.clone()];
    deps_b.sort();
    expected_b.sort();
    assert_eq!(deps_b, expected_b);

    // Cycle: adding child_a as a depends_on of parent closes A→Parent→A.
    let cycle_out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "depends-on",
            "add",
            "--ids",
            &id_parent,
            "--depends-on",
            &id_child_a,
            "--format",
            "json",
        ],
    );
    assert!(!cycle_out.status.success(), "cycle should be rejected");
    let cv: serde_json::Value = serde_json::from_slice(&cycle_out.stdout).unwrap();
    assert_eq!(cv["failed"].as_u64().unwrap(), 1);
    let err_msg = cv["results"][0]["error"].as_str().unwrap_or_default();
    assert!(
        err_msg.contains("cycle"),
        "expected cycle error, got: {err_msg}"
    );

    // Bulk remove parent from both children.
    let remove_out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "depends-on",
            "remove",
            "--ids",
            &ids_arg,
            "--depends-on",
            &id_parent,
            "--format",
            "json",
        ],
    );
    assert!(remove_out.status.success());
    let rv: serde_json::Value = serde_json::from_slice(&remove_out.stdout).unwrap();
    assert_eq!(rv["succeeded"].as_u64().unwrap(), 2);

    // child_a is now empty (its only dep was parent).
    assert!(show_depends_on(&env, &id_child_a).is_empty());
    // child_b retains its original child_a dep — that's expected; bulk remove only
    // strips the targeted entry, not the rest of the list.
    let deps_b_after = show_depends_on(&env, &id_child_b);
    assert_eq!(deps_b_after, vec![id_child_a.clone()]);
}

// AC-04: --dry-run previews without writing.
#[test]
fn bulk_dry_run_does_not_persist() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "Dry", "normal", "draft", &[]);
    let ids_arg = id.clone();

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &ids_arg,
            "--priority",
            "high",
            "--dry-run",
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["target_count"].as_u64().unwrap(), 1);
    assert_eq!(
        v["succeeded"].as_u64().unwrap(),
        1,
        "dry-run counts would-be successes; the dry_run flag says nothing was written"
    );
    assert_eq!(v["failed"].as_u64().unwrap(), 0);
    assert_eq!(
        v["succeeded"].as_u64().unwrap() + v["failed"].as_u64().unwrap(),
        v["target_count"].as_u64().unwrap(),
        "succeeded + failed must equal target_count in dry-run"
    );
    let row = &v["results"][0];
    assert_eq!(row["ok"], true);
    assert_eq!(row["dry_run"], true);

    // On-disk priority unchanged — dry-run must not write.
    assert_eq!(show_priority(&env, &id).as_deref(), Some("normal"));

    // Re-run without --dry-run applies it.
    let out2 = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &ids_arg,
            "--priority",
            "high",
            "--format",
            "json",
        ],
    );
    assert!(out2.status.success());
    let v2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(v2["dry_run"], false);
    assert_eq!(v2["succeeded"].as_u64().unwrap(), 1);
    assert_eq!(show_priority(&env, &id).as_deref(), Some("high"));
}

// AC-06: empty target set (no --ids and no --where) errors out clearly.
#[test]
fn bulk_rejects_empty_targets() {
    let env = TestEnv::new();
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--priority",
            "high",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("at least one target"),
        "expected empty-target error, got: {stderr}"
    );

    // Same for set-spec-status.
    let out2 = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-spec-status",
            "--status",
            "draft",
            "--format",
            "json",
        ],
    );
    assert!(!out2.status.success());

    // Same for depends-on add.
    let out3 = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "depends-on",
            "add",
            "--depends-on",
            "M1",
            "--format",
            "json",
        ],
    );
    assert!(!out3.status.success());
}

// AC-01 + AC-02: --ids and --where are unioned (deduped).
#[test]
fn bulk_unions_ids_and_where() {
    let env = TestEnv::new();
    let id_a = create_milestone(&env, "Union A", "normal", "draft", &[]);
    let id_b = create_milestone(&env, "Union B", "normal", "draft", &[]);

    // --where priority==normal matches both; --ids only id_a (dedup).
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &id_a,
            "--where",
            "priority==normal",
            "--priority",
            "high",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // 2 unique targets (id_a appears in both but is deduped).
    assert_eq!(v["target_count"].as_u64().unwrap(), 2);
    assert_eq!(v["succeeded"].as_u64().unwrap(), 2);

    assert_eq!(show_priority(&env, &id_a).as_deref(), Some("high"));
    assert_eq!(show_priority(&env, &id_b).as_deref(), Some("high"));
}

// F-01: bulk set-spec-status must enforce the same gates single-id does.
// A milestone with no acceptance_criteria cannot reach `review` (G3).
#[test]
fn bulk_set_spec_status_blocks_on_gates() {
    let env = TestEnv::new();
    // No ACs → G3 fires on `review`.
    let json = serde_json::json!({
        "title": "No AC",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "priority": "high",
        "intent": { "outcome": "x" },
        "problem": { "description": "y" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [],
        "spec_status": "draft",
    });
    let json_str = serde_json::to_string(&json).unwrap();
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            &json_str,
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = v["milestone"]["id"].as_str().unwrap().to_string();

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-spec-status",
            "--ids",
            &id,
            "--status",
            "review",
            "--format",
            "json",
        ],
    );
    assert!(
        !out.status.success(),
        "bulk must reject gate-blocked status change"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["failed"].as_u64().unwrap(), 1);
    let err = v["results"][0]["error"].as_str().unwrap_or_default();
    assert!(err.contains("G3"), "expected G3 in error, got: {err}");
}

// F-02: dry-run on depends-on add must preview cycle failures, not lie ok.
#[test]
fn bulk_depends_on_dry_run_previews_cycle() {
    let env = TestEnv::new();
    let id_parent = create_milestone(&env, "P", "normal", "draft", &[]);
    let id_child = create_milestone(&env, "C", "normal", "draft", &[id_parent.as_str()]);

    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "depends-on",
            "add",
            "--ids",
            &id_parent,
            "--depends-on",
            &id_child,
            "--dry-run",
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["failed"].as_u64().unwrap(), 1);
    let row = &v["results"][0];
    assert_eq!(row["ok"], false);
    let err = row["error"].as_str().unwrap_or_default();
    assert!(err.contains("cycle"), "expected cycle error, got: {err}");

    // And nothing on disk was actually mutated.
    assert!(show_depends_on(&env, &id_parent).is_empty());
}

// F-03: invalid --priority and bogus --depends_on fail once, not per id.
#[test]
fn bulk_validates_operation_level_args_up_front() {
    let env = TestEnv::new();
    let id_a = create_milestone(&env, "V A", "normal", "draft", &[]);
    let id_b = create_milestone(&env, "V B", "normal", "draft", &[]);
    let ids_arg = format!("{},{}", id_a, id_b);

    // Invalid priority: single error, not N duplicates.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &ids_arg,
            "--priority",
            "critical",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid priority"), "stderr: {stderr}");
    assert!(stderr.contains("urgent"), "stderr: {stderr}");

    // Invalid spec_status: same.
    let out2 = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-spec-status",
            "--ids",
            &ids_arg,
            "--status",
            "bogus",
            "--format",
            "json",
        ],
    );
    assert!(!out2.status.success());
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(stderr2.contains("invalid spec_status"), "stderr: {stderr2}");

    // Bogus depends_on target on add: fail before iterating.
    let out3 = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "depends-on",
            "add",
            "--ids",
            &ids_arg,
            "--depends-on",
            "M9999",
            "--format",
            "json",
        ],
    );
    assert!(!out3.status.success());
    let stderr3 = String::from_utf8_lossy(&out3.stderr);
    assert!(
        stderr3.contains("does not match any milestone"),
        "stderr: {stderr3}"
    );
}

// F-06: when the milestone can't be loaded for the per-id before snapshot,
// the row should omit `before` rather than emit null.
#[test]
fn bulk_omits_before_when_target_unreachable() {
    let env = TestEnv::new();
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            "M9999",
            "--priority",
            "high",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let row = &v["results"][0];
    assert!(
        row.get("before").is_none(),
        "before must be omitted, got row: {row}"
    );
    assert_eq!(row["error"], "milestone 9999 not found");
}

// F-07: depends-on remove must accept a non-existent --depends_on target
// (it's a no-op, not an error).
#[test]
fn bulk_depends_on_remove_allows_nonexistent_target() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "R", "normal", "draft", &[]);
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "depends-on",
            "remove",
            "--ids",
            &id,
            "--depends-on",
            "M9999",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["succeeded"].as_u64().unwrap(), 1);
    // After-state should be unchanged (no-op).
    assert!(show_depends_on(&env, &id).is_empty());
}

// F-08: --where '' must error rather than silently match every milestone.
#[test]
fn bulk_where_blank_is_rejected() {
    let env = TestEnv::new();
    let _ = create_milestone(&env, "W", "normal", "draft", &[]);
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--where",
            "",
            "--priority",
            "high",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no valid --where entries"),
        "expected clear error, got: {stderr}"
    );
}

// M124 review L-4: `--where` bogus + `--ids ""` (single empty string
// in the slice) — pre-fix this produced both a warning ("--where
// entries did not parse") AND a redundant "no valid --where entries"
// bail at the bottom of resolve_targets, because `!v.is_empty()` was
// true for a slice of length 1 (regardless of content). Post-fix the
// check matches the trim-and-skip semantics: only treat `--ids` as
// "real targets present" if at least one entry is non-blank.
#[test]
fn bulk_where_bogus_with_empty_ids_is_rejected_not_warned() {
    let env = TestEnv::new();
    let _ = create_milestone(&env, "W", "normal", "draft", &[]);
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            // clap splits comma-delimited --ids; a single empty string is
            // the user-typed `--ids ""`.
            "--ids",
            "",
            "--where",
            "no operator here",
            "--priority",
            "high",
            "--format",
            "json",
        ],
    );
    // No real targets → bail with the existing error. Crucially the
    // warning path must NOT fire (would be misleading: the user
    // supplied no real --ids either).
    assert!(
        !out.status.success(),
        "expected bail when both --ids and --where are bogus; got {out:?}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no valid --where entries"),
        "expected clear error, got: {stderr}"
    );
    assert!(
        !stderr.contains("falling back to --ids targets"),
        "the M124 ER-4 'fall back to --ids' warning must NOT fire when --ids is empty; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// M124 (M94 ER-1..ER-4) regression pins
// ---------------------------------------------------------------------------

// ER-1: idempotent no-op bulk op (priority already at the requested value)
// must NOT bump `updated` or rewrite the file. Pre-fix even no-op bulk
// fan-outs touched every targeted file, polluting `mp milestone log` with
// synthetic entries.
//
// Companion to `idempotent_no_op_skips_write` (which checks the in-band
// `updated` field). This test checks the file's mtime directly — the
// surface that `mp milestone log` actually consults. If a future
// serialization-formatting change breaks the byte-equality check in
// `with_milestone_mut_unlocked` (e.g. key reordering or whitespace drift
// between `load_milestone` and `serde_json::to_string_pretty`), the
// `updated`-field check might still pass (because the post-write content
// is logically equivalent) while the file is silently rewritten — and
// `git` would still show a diff. The mtime check catches that class of
// regression directly.
#[test]
fn idempotent_no_op_does_not_touch_mtime() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "mtime", "normal", "draft", &[]);

    // Resolve the milestone file path and capture its mtime.
    let path = env.tmp.path().join("master-plan/milestones");
    let entries: Vec<_> = std::fs::read_dir(&path)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(&id))
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one milestone file");
    let mtime0 = entries[0].metadata().unwrap().modified().unwrap();

    // Sleep enough for the filesystem mtime resolution to discriminate
    // a subsequent rewrite. CI is APFS (macOS) / ext4 (Linux), both
    // at 1ns resolution — 50ms is overkill on those targets but cheap.
    // NOT portable to FAT32/exFAT/HFS+ (≥1s/2s resolution); the test
    // would pass spuriously on those filesystems regardless of whether
    // the rewrite happened. Local runs on those filesystems should bump
    // the sleep to ≥2.1s.
    std::thread::sleep(std::time::Duration::from_millis(50));

    // No-op bulk op: priority already equals "normal".
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &id,
            "--priority",
            "normal",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // mtime must be unchanged — no rewrite happened.
    let mtime1 = entries[0].metadata().unwrap().modified().unwrap();
    assert_eq!(
        mtime0, mtime1,
        "no-op bulk op must NOT touch the milestone file's mtime; \
         pre={mtime0:?} post={mtime1:?}"
    );
}

// M124 review F-02 follow-up: with a WP + step present, the byte-equality
// check in `with_milestone_mut_unlocked` MUST round-trip identically
// across no-op bulk ops. The sibling `idempotent_no_op_does_not_touch_mtime`
// test pins the mtime surface for a bare milestone; this test pins the
// mtime surface for a milestone carrying a work package and a step —
// the shape where `prepare_for_disk`'s step-stripping side-effect
// COULD have produced asymmetry, but doesn't in practice because:
//
//   * `mp milestone step add` writes to top-level `m.steps` only
//     (`crates/mp/src/step.rs` populates `m.steps`); the new step is
//     never inserted into `m.work_packages[].steps`.
//   * On read, `normalize_steps_from_disk` drains `work_packages[].steps`
//     into `m.steps`. So in-memory `m.work_packages[].steps` is always
//     empty after a normal CLI round-trip, even when the source had a
//     step under a WP.
//
// So this test is structurally a sibling of the mtime test, not the
// direct asymmetric-normalize pin it claims to be in the comment. To
// pin the asymmetric WP.steps case you'd need to bypass the public
// CLI (e.g., write a fixture file with `work_packages[].steps`
// populated, then call `with_milestone_mut_unlocked`). The honest
// value here is "a milestone with WPs and steps is no-op-safe under
// repeated bulk ops" — a regression guard for any future shape
// change that disturbs the prepare_for_disk / skip_serializing
// invariants. The strong asymmetric pin is `prepare_for_disk_clears_wp_steps`
// in `crates/mp-model` (or equivalent unit test in mp) — out of scope
// for this integration test.
#[test]
fn idempotent_no_op_skips_write_with_steps_present() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "idem-steps", "normal", "ready", &[]);
    // `create_milestone` hardcodes spec_status="" (lifecycle draft); advance
    // to "ready" so the wp-add gate (effective_spec_status ready or later) passes.
    let _ = lib_api::run(&env, &["milestone", "set-spec-status", &id, "ready"]);

    // Add a work package and a step under it so the on-disk file carries
    // a top-level `[[steps]]` entry whose `work_package` points at the WP.
    // This is the shape where `prepare_for_disk` is non-trivial.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "wp",
            "add",
            &id,
            "--name",
            "Implementation",
            "--id",
            "WP01",
        ],
    );
    assert!(
        out.status.success(),
        "wp add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP01",
            "--action",
            "Do the thing",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "step add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Resolve the milestone file path and capture its mtime AFTER setup
    // (the wp/step adds above legitimately rewrote it).
    let path = env.tmp.path().join("master-plan/milestones");
    let entries: Vec<_> = std::fs::read_dir(&path)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(&id))
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one milestone file");
    let mtime0 = entries[0].metadata().unwrap().modified().unwrap();

    // Sleep enough for filesystem mtime resolution to discriminate a
    // subsequent rewrite. CI is APFS (macOS) / ext4 (Linux), both
    // at 1ns resolution — 50ms is overkill but cheap. NOT portable
    // to FAT32/exFAT/HFS+ (≥1s/2s resolution); see sibling
    // `idempotent_no_op_does_not_touch_mtime` for the same caveat.
    std::thread::sleep(std::time::Duration::from_millis(50));

    // No-op bulk op: priority already equals "normal".
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &id,
            "--priority",
            "normal",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // mtime must be unchanged — no rewrite happened, even though the
    // milestone carries a WP + step (the prepare_for_disk surface).
    let mtime1 = entries[0].metadata().unwrap().modified().unwrap();
    assert_eq!(
        mtime0, mtime1,
        "no-op bulk op must NOT touch the milestone file's mtime when \
         WPs/steps are present (F-02 regression surface); \
         pre={mtime0:?} post={mtime1:?}"
    );
}

#[test]
fn idempotent_no_op_skips_write() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "idem", "normal", "draft", &[]);

    // Capture the initial `updated` value.
    let updated0 = {
        let out = lib_api::run(
            &env,
            &[
                "show",
                "milestone",
                &id,
                "--fields",
                "milestone.updated",
                "--format",
                "json",
            ],
        );
        assert!(out.status.success());
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["milestone"]["updated"].as_str().unwrap().to_string()
    };

    // Bulk set the same priority that's already there.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &id,
            "--priority",
            "normal",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["succeeded"].as_u64().unwrap(),
        1,
        "no-op must report success: {v}"
    );

    // `updated` must be unchanged because the file was not rewritten.
    let updated1 = {
        let out = lib_api::run(
            &env,
            &[
                "show",
                "milestone",
                &id,
                "--fields",
                "milestone.updated",
                "--format",
                "json",
            ],
        );
        assert!(out.status.success());
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["milestone"]["updated"].as_str().unwrap().to_string()
    };
    assert_eq!(
        updated0, updated1,
        "idempotent no-op must not bump `updated` (M94 ER-1); was {updated0} now {updated1}"
    );
}

// ER-4: a bogus --where paired with valid --ids must NOT abort the whole
// command. Pre-fix this combination bailed out at `resolve_targets` before
// any apply_* ran, killing legitimate bulk dispatches. Post-fix the
// command warns on stderr and falls back to --ids alone.
#[test]
fn bogus_where_with_valid_ids_does_not_abort() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "bogus-where", "normal", "draft", &[]);

    // A --where entry with no operator (no `==` or `!=`) is rejected
    // by `parse_where_filters` — the filter is dropped and a warning
    // is emitted on stderr. Combined with --ids, the bulk must still
    // apply via --ids and not abort.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "bulk",
            "set-priority",
            "--ids",
            &id,
            "--where",
            "no operator here",
            "--priority",
            "high",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "bogus --where + valid --ids must succeed (M94 ER-4); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("warning") && stderr.contains("--where"),
        "expected warning on stderr mentioning --where; got: {stderr}"
    );

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["succeeded"].as_u64().unwrap(),
        1,
        "--ids targets must still be applied when --where is bogus; got: {v}"
    );
    // Priority should have actually been applied via --ids fallback.
    assert_eq!(show_priority(&env, &id).as_deref(), Some("high"));
}

// ER-2: depends_on_creates_cycle_in_graph must not clone the input
// graph. Pre-fix the function did `by_id.clone()` per call, which made
// bulk add-depends-on O(N²) in plan size for callers that loop over
// targets. The cycle walk now uses the input read-only.
#[test]
fn cycle_check_does_not_mutate_input_graph() {
    use mp::milestone::{build_depends_on_graph, depends_on_creates_cycle_in_graph};
    use mp::paths::PlanContext;
    use std::collections::HashMap;

    let env = TestEnv::new();
    // mp milestone create auto-assigns numeric ids; capture them.
    let id_a = create_milestone(&env, "alpha", "normal", "draft", &[]);
    let id_b = create_milestone(&env, "beta", "normal", "draft", &[id_a.as_str()]);

    let plan_dir = env.tmp.path().join("master-plan");
    let ctx = PlanContext::discover(
        Some(plan_dir.clone()),
        Some(plan_dir.parent().unwrap().to_path_buf()),
    )
    .unwrap();
    let graph = build_depends_on_graph(&ctx).unwrap();
    assert_eq!(
        graph.get(&id_b).cloned().unwrap_or_default(),
        vec![id_a.clone()],
        "fixture setup: beta must depend on alpha; graph={graph:?}"
    );

    // Snapshot the graph before the cycle check.
    let snapshot: HashMap<String, Vec<String>> = graph.clone();

    // Ask: would adding id_b as a depends_on of id_a create a cycle?
    // id_a → id_b → id_a → ... is a cycle, so this returns true.
    let prospective = vec![id_b.clone()];
    let result = depends_on_creates_cycle_in_graph(&graph, &id_a, &prospective);
    assert!(
        result,
        "cycle must be detected; graph={graph:?} a={id_a} b={id_b}"
    );

    // ER-2: input graph must be untouched. (Pre-fix `by_id.clone()` would
    // have inserted the prospective dep into a local copy — fine — but
    // the broader complaint was per-call allocation cost, not mutation.
    // This snapshot equality pins the no-mutation invariant.)
    assert_eq!(
        graph, snapshot,
        "depends_on_creates_cycle_in_graph must not mutate the input graph"
    );

    // Also: a non-cycle case should not panic and should return false.
    // Adding a dep on a brand-new unknown id cannot cycle.
    let non_cycle = vec!["nonexistent-zzz".to_string()];
    assert!(!depends_on_creates_cycle_in_graph(
        &graph, &id_a, &non_cycle
    ));
}
