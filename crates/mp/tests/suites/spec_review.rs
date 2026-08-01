//! M80 — agent-facing `mp spec` contract (S4). Verifies the review projection
//! JSON shape and that `--fields` slicing / hard-error parity (M97/M79) holds
//! on the new read commands.

use crate::common::TestEnv;

fn create_milestone(env: &TestEnv) -> String {
    let json = r#"{
        "title": "Spec review fixture",
        "intent": { "outcome": "Agents get review-oriented JSON" },
        "problem": { "description": "No purpose-built review read path." },
        "scope": {
            "in_scope": ["mp spec review", "mp spec diff"],
            "out_of_scope": ["generic projection", "write ops"]
        },
        "acceptance_criteria": [
            {
                "description": "AC1 covered",
                "verification": "cargo test"
            },
            {
                "description": "AC2 uncovered",
                "verification": "manual"
            }
        ]
    }"#;
    let out = env.run(&["milestone", "create", "--json", json, "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

#[test]
fn spec_review_shape() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = env.run(&["spec", "review", &id]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    // Headline section.
    assert_eq!(v["milestone"]["id"].as_str().unwrap(), id);
    assert!(v["outcome"].as_str().unwrap().contains("review-oriented"));
    assert!(v["problem"].as_str().unwrap().contains("read path"));
    assert!(v["scope"]["in_scope"].is_array());
    assert!(v["scope"]["out_of_scope"].is_array());
    assert!(v["force_bypassed"].is_boolean());
    assert!(v["review_state"].is_string());

    // Per-AC coverage: AC1 has no covering step (no steps added), AC2 likewise.
    let acs = v["acceptance_criteria"].as_array().unwrap();
    assert_eq!(acs.len(), 2);
    for ac in acs {
        assert!(ac["covered_by_steps"].is_array());
        assert!(ac["force_bypassed"].is_boolean());
    }

    // coverage_gaps lists every AC with no covering step — both here, since no
    // steps were added.
    let gaps = v["coverage_gaps"].as_array().unwrap();
    assert_eq!(gaps.len(), 2, "both ACs uncovered: {gaps:?}");
}

#[test]
fn spec_review_fields_slices_and_errors() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    // Stable-id projection works on the new read command.
    let out = env.run(&[
        "spec",
        "review",
        &id,
        "--fields",
        "acceptance_criteria[AC-01].id",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["acceptance_criteria"]["AC-01"]["id"].as_str().unwrap(),
        "AC-01"
    );
    // Unrequested top-level keys are absent.
    assert!(v.get("outcome").is_none(), "projection must slice: {v}");

    // Unknown path is a hard error (M97 parity).
    let out = env.run(&["spec", "review", &id, "--fields", "nope.nope"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
    assert!(stderr.contains("unknown path"), "stderr: {stderr}");
}

#[test]
fn spec_review_unknown_milestone_errors() {
    let env = TestEnv::new();
    let out = env.run(&["spec", "review", "9999"]);
    assert!(!out.status.success());
}

#[test]
fn spec_diff_no_prior_review_degrades() {
    let env = TestEnv::new();
    let id = create_milestone(&env);

    let out = env.run(&["spec", "diff", &id]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // No review record → last_review null, baseline_ref null, structured status.
    assert!(v["last_review"].is_null());
    assert!(v["baseline_ref"].is_null());
    let status = v["baseline_status"].as_str().unwrap();
    assert!(
        status.contains("no prior review"),
        "graceful status note: {status}"
    );
    assert!(v["changes"].as_array().unwrap().is_empty());
}

// F-01 regression: design_decisions must appear in the diff when edited. The
// module docstring + review projection both include design_decisions; before
// the fix diff_spec_fields silently omitted them. Mutating only a design
// decision post-baseline must surface a change record.
#[test]
fn spec_diff_reports_design_decisions_change() {
    use crate::common::TestEnv;
    let env = TestEnv::new();
    let dir = env.tmp.path().to_path_buf();

    // Isolated git repo so the rev-list baseline lookup is deterministic.
    let _ = std::process::Command::new("git")
        .current_dir(&dir)
        .args(["init"])
        .output();
    let _ = std::process::Command::new("git")
        .current_dir(&dir)
        .args(["config", "user.email", "t@t"])
        .output();
    let _ = std::process::Command::new("git")
        .current_dir(&dir)
        .args(["config", "user.name", "t"])
        .output();

    // Create with a design decision, then run the milestone to `done` so a
    // review record can be recorded (reviews pass requires execution_status=done).
    let create_json = r#"{
        "title": "DD diff fixture",
        "intent": { "outcome": "outcome" },
        "problem": { "description": "problem" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{ "description": "ac1", "verification": "manual: test fixture" }],
        "design_decisions": [
            { "area": "core", "choice": "pick A", "rationale": "speed" }
        ]
    }"#;
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Drive the milestone to done: approve → decompose → step → complete.
    let approve = env.run(&["milestone", "approve", &id]);
    assert!(
        approve.status.success(),
        "approve failed: {} {}",
        String::from_utf8_lossy(&approve.stderr),
        String::from_utf8_lossy(&approve.stdout)
    );
    let _ = env.run(&["milestone", "decompose", &id]);
    let _ = env.run(&[
        "milestone",
        "step",
        "add",
        &id,
        "--wp",
        "WP1",
        "--action",
        "do it",
        "--done-when",
        "done",
        "--tests",
        "true",
        "--covers-ac",
        "AC-01",
    ]);
    let _ = env.run(&["milestone", "set-status", &id, "in-progress"]);
    let _ = env.run(&["milestone", "step", "done", &id, "S1"]);
    let _ = env.run(&[
        "milestone",
        "criterion",
        "pass",
        &id,
        "AC-01",
        "--evidence",
        "ok",
    ]);
    let complete = env.run(&["milestone", "complete", &id, "--evidence", "done"]);
    assert!(
        complete.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&complete.stderr)
    );
    let review = env.run(&[
        "reviews",
        "pass",
        &id,
        "--verdict",
        "ok",
        "--reviewer",
        "tester",
    ]);
    assert!(
        review.status.success(),
        "review pass failed: {}",
        String::from_utf8_lossy(&review.stderr)
    );
    let _ = std::process::Command::new("git")
        .current_dir(&dir)
        .args(["add", "-A"])
        .output();
    let _ = std::process::Command::new("git")
        .current_dir(&dir)
        .args(["commit", "-m", "baseline", "--no-gpg-sign"])
        .output();

    // Mutate ONLY the design decisions after the baseline: add a new one via
    // the fragment CLI. `design-decision add` appends; with no stable id, the
    // diff compares the (area, choice, rationale) set, so a new tuple registers.
    let _ = env.run(&[
        "milestone",
        "design-decision",
        "add",
        &id,
        "--area",
        "Correctness",
        "--decision",
        "use B for correctness",
        "--rationale",
        "correctness over speed",
    ]);

    let out = env.run(&["spec", "diff", &id]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    // Baseline resolved against the committed snapshot.
    assert!(
        v["baseline_ref"].is_string(),
        "baseline should resolve: {v}"
    );

    let changes = v["changes"].as_array().unwrap();
    let touched_dd: Vec<&serde_json::Value> = changes
        .iter()
        .filter(|c| c["field"].as_str() == Some("design_decisions"))
        .collect();
    assert!(
        !touched_dd.is_empty(),
        "diff must report the design_decisions change (F-01 regression): {changes:?}"
    );
    // The new decision text must appear in the `to` side.
    let tos = touched_dd[0]["to"].as_array().unwrap();
    let has_new = tos.iter().any(|d| {
        d["choice"]
            .as_str()
            .map(|c| c.contains("use B for correctness"))
            .unwrap_or(false)
    });
    assert!(has_new, "new decision should appear in `to`: {tos:?}");
}
