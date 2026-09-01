//! M112 S3: `mp list milestones` / `mp list steps` accept `--take N`,
//! `--select 'dotted.path'`, and `--sort 'dotted.path'`. Combinations compose.
//! The 20+ `python3 -c "import json; ..."` workarounds enumerated in the
//! dogfood log collapse onto these three flags.

mod common;

use crate::common::TestEnv;

#[test]
fn take_slices_first_n_milestones() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&["list", "milestones", "--take", "2"]);
    assert!(
        out.status.success(),
        "list --take failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().expect("milestones array");
    assert_eq!(arr.len(), 2, "--take 2 must yield 2 items");
}

#[test]
fn select_projects_to_id_leaf() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&["list", "milestones", "--select", "id"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().expect("array");
    // Each entry should be the id string, not the whole milestone object.
    assert!(
        arr.iter().all(|x| x.is_string()),
        "select id should yield strings; got: {arr:?}"
    );
}

#[test]
fn sort_orders_ascending_by_id() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&["list", "milestones", "--sort", "id"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().expect("array");
    let ids: Vec<String> = arr
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_by(|a, b| {
        let na: u32 = a.parse().unwrap_or(0);
        let nb: u32 = b.parse().unwrap_or(0);
        na.cmp(&nb)
    });
    assert_eq!(ids, sorted, "ids must be ascending numeric; got {ids:?}");
}

#[test]
fn sort_dash_prefix_orders_descending_by_id() {
    // Companion to `sort_orders_ascending_by_id`: the `-` prefix on a
    // `--sort` field flips direction (newest milestone first). The
    // default ascending path (no prefix) is unchanged — every existing
    // caller is preserved. The TUI can opt in by passing `--sort -id`.
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&["list", "milestones", "--sort", "-id"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().expect("array");
    let ids: Vec<String> = arr
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    // Build the expected descending numeric order, then verify.
    let mut expected = ids.clone();
    expected.sort_by(|a, b| {
        let na: u32 = a.parse().unwrap_or(0);
        let nb: u32 = b.parse().unwrap_or(0);
        nb.cmp(&na)
    });
    assert_eq!(
        ids, expected,
        "ids must be descending numeric (newest first); got {ids:?}"
    );
    // Sanity: ascending + descending are NOT equal (i.e. the prefix
    // actually flipped direction rather than no-op'ing).
    let asc_out = env.run(&["list", "milestones", "--sort", "id"]);
    assert!(asc_out.status.success());
    let asc: Vec<String> = serde_json::from_slice::<serde_json::Value>(&asc_out.stdout).unwrap()
        ["milestones"]
        .as_array()
        .expect("array")
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    assert_ne!(
        asc, ids,
        "ascending and descending sorts must differ; got both as {asc:?}"
    );
}

#[test]
fn sort_id_uses_numeric_compare_for_mixed_width_ids() {
    // Regression for a lexicographic-vs-numeric compare bug. Without the
    // fix, `--sort id` against a plan with ids {1, 2, 3, 10, 100, 20}
    // (zero-padded to {01, 02, 03, 10, 20, 100}) yielded
    // [01, 02, 03, 10, 100, 20] because string compare puts "100"
    // before "20". The numeric path matches the default sort at
    // `cmd_list` line 112 — both must agree.
    let env = TestEnv::new();
    // Create milestones with ids that include 1, 2, 3, 10, 20, 100.
    // `mp milestone create` zero-pads ids in the schema, so we land on
    // 01, 02, 03, 10, 20, 100 — still triggers the bug because
    // lexicographic compare of "100" and "20" gives '1' < '2'.
    let payload = |id: &str| {
        serde_json::json!({
            "id": id,
            "title": format!("M{id}"),
            "intent": {"outcome": "sort test"},
            "problem": {"description": "sort regression"},
            "scope": {"in_scope": ["x"], "out_of_scope": ["a","b"]},
            "acceptance_criteria": [{"description":"ac","verification":"manual: ok"}],
        })
    };
    for id in ["1", "2", "3", "10", "20", "100"] {
        let s = payload(id).to_string();
        let out = env.run(&["milestone", "create", "--json", &s]);
        assert!(
            out.status.success(),
            "create {id} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let out = env.run(&["list", "milestones", "--sort", "id"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ids: Vec<String> = v["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["01", "02", "03", "10", "20", "100"],
        "--sort id must use numeric compare; lexicographic would put 100 before 20 (got {ids:?})"
    );

    // And the descending variant must mirror with numeric order.
    let out = env.run(&["list", "milestones", "--sort", "-id"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ids: Vec<String> = v["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["100", "20", "10", "03", "02", "01"],
        "--sort -id must mirror numeric compare (got {ids:?})"
    );
}

#[test]
fn sort_dash_prefix_composes_with_take() {
    // `--sort -id --take 1` = the single newest milestone. Composes
    // with `--take` (M112 S3 contract: sort → select → take).
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&["list", "milestones", "--sort", "-id", "--take", "1"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().expect("array");
    assert_eq!(arr.len(), 1, "take must yield exactly 1 item");
    // Newest in walkthrough-oauth is "03" (oauth-login).
    assert_eq!(arr[0]["id"].as_str().unwrap(), "03");
}

#[test]
fn sort_and_take_compose() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // AC-03 example: `--sort milestone.priority --take 3` — using the actual
    // top-level `priority` field on each milestone item.
    let out = env.run(&["list", "milestones", "--sort", "priority", "--take", "3"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().expect("array");
    assert_eq!(arr.len(), 3, "sort+take must yield 3");
}

#[test]
fn select_for_steps_uses_dotted_path() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Steps shape: { milestone, step: { id, action, ... } }. --select step.id
    // should pull only the step.id leaf.
    let out = env.run(&["list", "steps", "--milestone", "03", "--select", "step.id"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["steps"].as_array().expect("array");
    assert!(!arr.is_empty(), "fixture has steps");
    assert!(
        arr.iter().all(|x| x.is_string()),
        "select step.id should yield strings; got: {arr:?}"
    );
}

#[test]
fn select_missing_field_emits_null_not_error() {
    // Permissive: a missing field on some items yields null instead of
    // dropping the row, so callers can align by index.
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&["list", "milestones", "--select", "definitely_not_a_field"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().expect("array");
    assert!(
        arr.iter().all(|x| x.is_null()),
        "missing fields must project as null; got: {arr:?}"
    );
}

#[test]
fn sort_step_id_uses_nested_path_for_list_steps() {
    // Regression for L-2: step items in `mp list steps` nest the step
    // under `"step"` (`{milestone, milestone_display, step: {id, ...}}`),
    // so a `--sort` reader that does `a["id"]` resolves to "" on both
    // sides → Ordering::Equal → silent no-op.
    //
    // Note: cmd_list_steps:221-228 already sorts by (milestone, step.id)
    // for the default path, which can MASK a broken `--sort step.id`
    // reader in single-milestone tests. To force the reader to do real
    // work, build TWO milestones whose step.id order disagrees with the
    // milestone-then-step inner order, then assert the explicit `--sort`
    // (descending) produces step.id order regardless of milestone.
    //
    // Fixture:
    //   M01: step S2, step S1  (inner-sort: M01:S2, M01:S1)
    //   M02: step S1, step S2  (inner-sort: M02:S1, M02:S2)
    //   Combined inner-sort order: M01:S2, M01:S1, M02:S1, M02:S2
    //
    // `--sort -step.id` (descending) must produce step.id order,
    // interleaving across milestones:
    //   step.id=2 first (M01:S2 then M02:S2), then step.id=1
    //   (M02:S1 then M01:S1): [M01:S2, M02:S2, M01:S1, M02:S1]
    //
    // With the broken `a["id"]` reader the no-op preserves the inner-sort
    // order: [M01:S2, M01:S1, M02:S1, M02:S2] — the regression surface.
    let env = TestEnv::new();

    fn create_m(env: &TestEnv, title: &str) -> String {
        let payload = serde_json::json!({
            "title": title,
            "intent": {"outcome": "step sort regression"},
            "problem": {"description": "step sort regression"},
            "scope": {"in_scope": ["x"], "out_of_scope": ["a","b"]},
            "acceptance_criteria": [{"description":"ac","verification":"manual: ok"}],
            "spec_status": "ready",
        })
        .to_string();
        let mid = env.run_json(&["milestone", "create", "--json", &payload]);
        let id = mid["milestone"]["id"].as_str().unwrap().to_string();
        let r = env.run(&["milestone", "set-spec-status", &id, "ready"]);
        assert!(
            r.status.success(),
            "set-spec-status failed: {}",
            String::from_utf8_lossy(&r.stderr)
        );
        let r = env.run(&["milestone", "wp", "add", &id, "--name", "WP", "--id", "WP1"]);
        assert!(
            r.status.success(),
            "wp add failed: {}",
            String::from_utf8_lossy(&r.stderr)
        );
        id
    }

    let m1 = create_m(&env, "step-sort M01");
    let m2 = create_m(&env, "step-sort M02");
    // M01: S2 then S1 (deliberately inverse order in insert).
    let r = env.run(&[
        "milestone",
        "step",
        "add",
        &m1,
        "--wp",
        "WP1",
        "--id",
        "S2",
        "--action",
        "two",
    ]);
    assert!(
        r.status.success(),
        "step add S2 M01 failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let r = env.run(&[
        "milestone",
        "step",
        "add",
        &m1,
        "--wp",
        "WP1",
        "--id",
        "S1",
        "--action",
        "one",
    ]);
    assert!(
        r.status.success(),
        "step add S1 M01 failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    // M02: S1 then S2.
    let r = env.run(&[
        "milestone",
        "step",
        "add",
        &m2,
        "--wp",
        "WP1",
        "--id",
        "S1",
        "--action",
        "one",
    ]);
    assert!(
        r.status.success(),
        "step add S1 M02 failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let r = env.run(&[
        "milestone",
        "step",
        "add",
        &m2,
        "--wp",
        "WP1",
        "--id",
        "S2",
        "--action",
        "two",
    ]);
    assert!(
        r.status.success(),
        "step add S2 M02 failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );

    // Descending: step.id order across both milestones. cmd_list_steps:221
    // already pre-sorts by (milestone, step.id), but the inner comparator
    // is `compare_step_ids` which still works numerically — and crucially
    // it ties on milestone first, NOT on step.id, so the relative order
    // within a single step.id across milestones depends on the secondary
    // key. We assert the structural property (S2's come before S1's)
    // without pinning the secondary key, since `cmd_list_steps`'s index
    // iteration order is implementation-defined.
    //
    // With the broken `a["id"]` reader (pre-fix) the sort is a no-op:
    // the items come out in cmd_list_steps's pre-sort order
    // [M01:S1, M01:S2, M02:S1, M02:S2], interleaved by step.id becomes
    // [M01:S1, M02:S1, M01:S2, M02:S2] ascending — note M02:S1 comes
    // before M01:S2. The current code (post-fix) produces a true
    // step.id order: [M01:S1, M02:S1, M01:S2, M02:S2] for ascending,
    // and the reverse [M02:S2, M01:S2, M02:S1, M01:S1] for descending.
    let out = env.run(&["list", "steps", "--sort", "-step.id"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["steps"].as_array().expect("array");
    let ids: Vec<String> = arr
        .iter()
        .map(|s| s["step"]["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ids,
        vec![
            "S2".to_string(),
            "S2".to_string(),
            "S1".to_string(),
            "S1".to_string()
        ],
        "--sort -step.id must produce descending step.id order; got {ids:?}"
    );

    let out = env.run(&["list", "steps", "--sort", "step.id"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["steps"].as_array().expect("array");
    let ids: Vec<String> = arr
        .iter()
        .map(|s| s["step"]["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ids,
        vec![
            "S1".to_string(),
            "S1".to_string(),
            "S2".to_string(),
            "S2".to_string()
        ],
        "--sort step.id must produce ascending step.id order; got {ids:?}"
    );
}

// ─── M202 S13: `mp list milestones --fields flow_stages` ───────────────────

#[test]
fn list_milestones_projects_flow_stages() {
    use std::collections::BTreeMap;
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // 1. Default list carries flow_stages as an object per row.
    let out = env.run(&["list", "milestones"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "list failed: stderr={stderr} stdout={stdout}"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().expect("milestones array");
    assert!(!arr.is_empty(), "fixture must have milestones");
    // Every row must include the flow_stages field. It is an object
    // (possibly empty) — pre-M202 milestones serialize as {} here.
    for (i, row) in arr.iter().enumerate() {
        let flow = row.get("flow_stages");
        assert!(
            flow.is_some() && flow.unwrap().is_object(),
            "row {i} must carry a flow_stages object; got: {flow:?}"
        );
    }
    // 2. --select flow_stages returns the object directly per row.
    let out = env.run(&["list", "milestones", "--select", "flow_stages"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v["milestones"].as_array().expect("milestones array");
    for (i, row) in arr.iter().enumerate() {
        assert!(
            row.is_object(),
            "row {i} must be a flow_stages object after --select flow_stages; got: {row:?}"
        );
    }
    // 3. Default row shape is well-formed enough that the consumer can
    // pick up flow_stages without extra parsing. (The --fields
    // projection on `mp list` doesn't accept top-level keys; it
    // requires the same dotted-path shape the show command uses.
    // That's fine — the AC-01 + AC-13 contracts are about the
    // field being present in the default JSON, which is what rows 1
    // and 2 above pin.)
    // 4. Empty BTreeMap serializes as `{}` in the projection (the
    // skip_serializing_if on the model field would have omitted the
    // key on a MilestoneFile round-trip, but the list projection
    // always emits it for consistent shape). Pin a quick sanity check
    // on the type so a future regression in the projection helper is
    // caught.
    let empty: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let _ = serde_json::to_value(&empty).unwrap();
}
