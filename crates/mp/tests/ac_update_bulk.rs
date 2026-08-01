//! M118 S1 / AC-01: `mp milestone ac update --bulk @file.json` applies
//! a JSON array of fragment updates through the same per-AC update flow.
//! Tests cover: bulk of 3 ACs lands all three, empty array is a no-op,
//! missing id fails fast with a structured error, single --bulk call
//! stamps per-AC evidence without rerunning the verifier path.

mod common;

use crate::common::TestEnv;

#[test]
fn bulk_update_three_acs_in_one_call_lands() {
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 bulk target",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [
                { "description": "alpha", "verification": "echo a" },
                { "description": "beta", "verification": "echo b" },
                { "description": "gamma", "verification": "echo c" }
            ]
        }"#,
    ]);
    assert!(create.status.success());
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["milestone"]["id"].as_str().unwrap().to_string();

    // Write the bulk file with three fragment updates.
    let payload = serde_json::json!([
        {
            "id": "AC-01",
            "description": "alpha (bulk updated)",
            "verification": "echo alpha-new"
        },
        {
            "id": "AC-02",
            "verification": "echo beta-new"
        },
        {
            "id": "AC-03",
            "evidence": "bulk-stamped 3rd AC evidence"
        }
    ]);
    let bulk_path = env.tmp.path().join("bulk.json");
    std::fs::write(&bulk_path, serde_json::to_string_pretty(&payload).unwrap())
        .expect("write bulk.json");

    let out = env.run(&[
        "milestone",
        "ac",
        "bulk",
        &id,
        "--bulk",
        bulk_path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "bulk update failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(
        v["applied"], 3,
        "applied count should equal input array length"
    );
    let results = v["results"].as_array().expect("results array");
    assert_eq!(results.len(), 3);

    // Read back: each AC should have its bulk-set field.
    let show1 = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "acceptance_criteria[AC-01]",
    ]);
    let v1: serde_json::Value = serde_json::from_slice(&show1.stdout).unwrap();
    let ac1 = &v1["acceptance_criteria"]["AC-01"];
    assert_eq!(ac1["description"], "alpha (bulk updated)");
    assert_eq!(ac1["verification"], "echo alpha-new");

    let show2 = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "acceptance_criteria[AC-02]",
    ]);
    let v2: serde_json::Value = serde_json::from_slice(&show2.stdout).unwrap();
    assert_eq!(
        v2["acceptance_criteria"]["AC-02"]["verification"],
        "echo beta-new"
    );

    let show3 = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "acceptance_criteria[AC-03]",
    ]);
    let v3: serde_json::Value = serde_json::from_slice(&show3.stdout).unwrap();
    assert_eq!(
        v3["acceptance_criteria"]["AC-03"]["evidence"],
        "bulk-stamped 3rd AC evidence"
    );
}

#[test]
fn bulk_update_empty_array_is_a_noop() {
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 empty bulk",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only-ac", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let bulk_path = env.tmp.path().join("empty.json");
    std::fs::write(&bulk_path, "[]").expect("write empty.json");

    let out = env.run(&[
        "milestone",
        "ac",
        "bulk",
        &id,
        "--bulk",
        bulk_path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "empty bulk failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["applied"], 0);
    assert!(v["results"].as_array().unwrap().is_empty());
}

#[test]
fn bulk_update_missing_id_fails_fast() {
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 missing-id",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Entry without required `id` field.
    let payload = serde_json::json!([
        { "description": "missing-id entry" }
    ]);
    let bulk_path = env.tmp.path().join("missing-id.json");
    std::fs::write(&bulk_path, serde_json::to_string(&payload).unwrap()).unwrap();

    let out = env.run(&[
        "milestone",
        "ac",
        "bulk",
        &id,
        "--bulk",
        bulk_path.to_str().unwrap(),
    ]);
    assert!(
        !out.status.success(),
        "missing id must fail fast (without writing)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("missing required `id` field"),
        "error must name the missing field; got: {stderr}"
    );
    assert!(
        stderr.contains("bulk[0]"),
        "error must point at the offending index; got: {stderr}"
    );
}

#[test]
fn bulk_update_unknown_ac_id_fails_fast() {
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 unknown-ac",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let payload = serde_json::json!([
        { "id": "AC-99", "description": "nonexistent" }
    ]);
    let bulk_path = env.tmp.path().join("unknown-ac.json");
    std::fs::write(&bulk_path, serde_json::to_string(&payload).unwrap()).unwrap();

    let out = env.run(&[
        "milestone",
        "ac",
        "bulk",
        &id,
        "--bulk",
        bulk_path.to_str().unwrap(),
    ]);
    assert!(!out.status.success(), "unknown AC id must fail fast");
    let stderr = String::from_utf8_lossy(&out.stderr);
    // M118 CR (F-2): unknown AC id now fails atomically at the
    // preflight phase with a structured "no AC(s) `AC-99`" message
    // listing the known ACs. The prior contract (criterion_update's
    // "AC-99 not found in milestone 01" loop error) fired AFTER
    // partial writes had already hit disk; the new contract fails
    // fast before any write.
    assert!(
        stderr.contains("AC-99") && stderr.contains("no AC"),
        "error must name the missing AC and the preflight verb 'no AC'; got: {stderr}"
    );
    assert!(
        stderr.contains("has no AC") || stderr.contains("no AC(s)"),
        "error should use the canonical preflight phrase; got: {stderr}"
    );
}

#[test]
fn bulk_update_non_array_root_fails_with_kind_label() {
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 non-array",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [{ "description": "only", "verification": "echo ok" }]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Root is an object, not an array.
    let bulk_path = env.tmp.path().join("not-array.json");
    std::fs::write(&bulk_path, r#"{"foo": "bar"}"#).unwrap();

    let out = env.run(&[
        "milestone",
        "ac",
        "bulk",
        &id,
        "--bulk",
        bulk_path.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("must be a JSON array"),
        "error must call out the shape requirement; got: {stderr}"
    );
    assert!(
        stderr.contains("object") || stderr.contains("kind"),
        "error must include the actual JSON kind; got: {stderr}"
    );
}

#[test]
fn bulk_update_unknown_milestone_id_fails_with_clean_error() {
    // M118 findings follow-up (B-59): a missing milestone id must
    // surface ONE clean error rather than N noisy "AC not found in
    // milestone X" errors per element. Pre-fix the inner loop drove
    // each `criterion_update` call against a missing milestone, emitting
    // N redundant errors. Post-fix the failure short-circuits at the
    // top with the canonical "milestone <id> not found" shape.
    let env = TestEnv::new();
    let payload = serde_json::json!([
        { "id": "AC-01", "description": "x" },
        { "id": "AC-02", "description": "y" },
        { "id": "AC-03", "description": "z" }
    ]);
    let bulk_path = env.tmp.path().join("missing-ms.json");
    std::fs::write(&bulk_path, serde_json::to_string(&payload).unwrap()).unwrap();

    let out = env.run(&[
        "milestone",
        "ac",
        "bulk",
        "M9999", // nonexistent
        "--bulk",
        bulk_path.to_str().unwrap(),
    ]);
    assert!(
        !out.status.success(),
        "bulk against nonexistent milestone must fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("M9999") && stderr.contains("not found"),
        "error must name the missing milestone exactly; got: {stderr}"
    );
    // Exactly one occurrence of "not found" — not N redundant copies.
    let occurrences = stderr.matches("not found").count();
    assert_eq!(
        occurrences, 1,
        "exactly one error for the missing milestone; got {occurrences} matches in stderr={stderr:?}"
    );
}

#[test]
fn bulk_update_emits_preflight_warning_per_malformed_verification() {
    // M118 CR (F-1): the bulk path previously bypassed the shell-parse
    // preflight that the single-AC `Update` path emits. Agents using
    // the bulk path to batch-write new verifications would silently
    // accept malformed shell without seeing the diagnostic. Post-fix
    // each `verification` that fails `sh -n` is surfaced in a
    // `preflight_warnings` array alongside the per-element result.
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 bulk preflight",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [
                { "id": "AC-01", "description": "d1", "verification": "echo a" },
                { "id": "AC-02", "description": "d2", "verification": "echo b" }
            ]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    // AC-01 has malformed shell; AC-02 is fine. Both should land but
    // preflight_warnings must name AC-01 specifically.
    let payload = serde_json::json!([
        { "id": "AC-01", "verification": "if then echo broken" },
        { "id": "AC-02", "verification": "echo ok" }
    ]);
    let bulk_path = env.tmp.path().join("preflight-bulk.json");
    std::fs::write(&bulk_path, serde_json::to_string(&payload).unwrap()).unwrap();

    let out = env.run(&[
        "milestone",
        "ac",
        "bulk",
        &id,
        "--bulk",
        bulk_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "bulk update should still succeed (warn-not-reject); stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["applied"], 2);

    let warnings = v["preflight_warnings"]
        .as_array()
        .expect("preflight_warnings array");
    assert_eq!(
        warnings.len(),
        1,
        "exactly one preflight warning expected (AC-01); got: {warnings:?}"
    );
    assert_eq!(warnings[0]["id"], "AC-01");
    assert!(
        warnings[0]["warning"]["warning"]
            .as_str()
            .unwrap_or("")
            .contains("shell-parse"),
        "warning should mention shell-parse; got: {warnings:?}"
    );

    // Both ACs land on disk despite the AC-01 warning (warn-not-reject
    // matches the single-AC contract).
    let show = env.run(&["show", "milestone", &id, "--format", "json"]);
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let acs = shown["acceptance_criteria"].as_array().unwrap();
    assert_eq!(
        acs.iter().find(|a| a["id"] == "AC-01").unwrap()["verification"],
        "if then echo broken"
    );
}

#[test]
fn bulk_update_no_preflight_warnings_when_all_clean() {
    // Mirror of the F-1 regression — the `preflight_warnings` array
    // must be absent (not just empty) when every verification parses,
    // so agents reading the response don't see a "0 warnings" key
    // that has nothing actionable inside it.
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 bulk no-warning",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [
                { "id": "AC-01", "description": "d1", "verification": "echo a" }
            ]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    let bulk_path = env.tmp.path().join("clean-bulk.json");
    std::fs::write(
        &bulk_path,
        r#"[{"id": "AC-01", "verification": "echo clean"}]"#,
    )
    .unwrap();

    let out = env.run(&[
        "milestone",
        "ac",
        "bulk",
        &id,
        "--bulk",
        bulk_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["applied"], 1);
    assert!(
        v.get("preflight_warnings").is_none(),
        "preflight_warnings must be absent when no warnings; got: {v:?}"
    );
}

#[test]
fn bulk_update_unknown_ac_does_not_partial_apply() {
    // M118 CR (F-2): the prior pre-check loop validated JSON shape
    // only; AC id existence was checked inside `criterion_update`,
    // which fires AFTER the first N-1 elements have been written.
    // Re-block with an unknown AC id in the middle of a 3-element
    // batch and assert: zero elements land on disk. Pin the atomic-
    // apply contract.
    let env = TestEnv::new();
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{
            "title": "M118 atomic",
            "intent": { "outcome": "x" },
            "problem": { "description": "x" },
            "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
            "acceptance_criteria": [
                { "id": "AC-01", "description": "first", "verification": "echo a" },
                { "id": "AC-02", "description": "second", "verification": "echo b" }
            ]
        }"#,
    ]);
    assert!(create.status.success());
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Element 0 (AC-01) is valid; element 1 references unknown
    // AC-99. Pre-fix the AC-01 write would land before the unknown-AC
    // error fired; post-fix the whole batch is rejected atomically.
    let payload = serde_json::json!([
        { "id": "AC-01", "evidence": "should not be applied" },
        { "id": "AC-99", "evidence": "causes preflight rejection" },
        { "id": "AC-02", "evidence": "should not be applied either" }
    ]);
    let bulk_path = env.tmp.path().join("atomic-bulk.json");
    std::fs::write(&bulk_path, serde_json::to_string(&payload).unwrap()).unwrap();

    let out = env.run(&[
        "milestone",
        "ac",
        "bulk",
        &id,
        "--bulk",
        bulk_path.to_str().unwrap(),
    ]);
    assert!(!out.status.success(), "bulk with unknown AC must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("AC-99"),
        "error must name the offending AC; got: {stderr}"
    );

    // AC-01 evidence should NOT have been written.
    let show = env.run(&[
        "show",
        "milestone",
        &id,
        "--fields",
        "acceptance_criteria[AC-01]",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let ac1 = &v["acceptance_criteria"]["AC-01"];
    assert_eq!(
        ac1["evidence"], "",
        "AC-01 evidence must be empty after atomic-reject; got: {ac1:?}"
    );
}
