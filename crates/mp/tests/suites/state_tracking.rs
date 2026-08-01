use crate::common::TestEnv;

#[test]
fn all_steps_done_stays_in_progress_until_complete() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "Completion flow",
        "intent": { "outcome": "Test G7-safe completion." },
        "problem": { "description": "Steps done must not set execution done early." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "Complete works", "verification": "manual: accepted — test" }
        ]
    }"#;
    env.run_json(&["milestone", "create", "--json", create_json]);
    env.run_json(&["milestone", "approve", "01"]);
    env.run_json(&["milestone", "decompose", "01"]);
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "step one",
        "--done-when",
        "done",
        "--tests",
        "test_one",
    ]);
    env.run_json(&["milestone", "set-status", "01", "in-progress"]);
    env.run_json(&["milestone", "step", "done", "01", "S1"]);

    let show = env.run_json(&["show", "milestone", "01"]);
    assert_eq!(
        show["milestone"]["execution_status"].as_str(),
        Some("in-progress"),
        "all steps done should stay in-progress until milestone complete"
    );

    let out = env.run(&["validate", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let empty = Vec::new();
    let errors = json["errors"].as_array().unwrap_or(&empty);
    let g7: Vec<_> = errors
        .iter()
        .filter(|e| e["code"].as_str() == Some("G7"))
        .collect();
    assert!(
        g7.is_empty(),
        "G7 must not fire before milestone complete: {g7:?}"
    );
}

#[test]
fn auto_close_when_last_step_done() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "Auto-close test",
        "intent": { "outcome": "Test auto-close." },
        "problem": { "description": "Need auto-close on last step done." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "Auto-close works", "verification": "manual: state tracking setup" }
        ]
    }"#;
    env.run_json(&["milestone", "create", "--json", create_json]);
    env.run_json(&["milestone", "approve", "01"]);
    env.run_json(&["milestone", "decompose", "01"]);
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "step one",
        "--done-when",
        "done",
        "--tests",
        "test_one",
    ]);
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "step two",
        "--done-when",
        "done",
        "--tests",
        "test_two",
    ]);
    env.run_json(&["milestone", "set-status", "01", "in-progress"]);

    // First step done — milestone should stay in-progress
    env.run_json(&["milestone", "step", "done", "01", "S1"]);
    let show = env.run_json(&["show", "milestone", "01"]);
    assert_eq!(
        show["milestone"]["execution_status"].as_str(),
        Some("in-progress")
    );

    // Last step done — stays in-progress until milestone complete (G7-safe)
    env.run_json(&["milestone", "step", "done", "01", "S2"]);
    let show = env.run_json(&["show", "milestone", "01"]);
    assert_eq!(
        show["milestone"]["execution_status"].as_str(),
        Some("in-progress"),
        "expected in-progress until milestone complete, got: {}",
        show["milestone"]["execution_status"]
    );
}

#[test]
fn no_stale_warning_when_no_steps() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "No steps",
        "intent": { "outcome": "Empty milestone." },
        "problem": { "description": "Check W30 doesn't fire on empty." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "No W30", "verification": "manual: state tracking setup" }
        ]
    }"#;
    env.run_json(&["milestone", "create", "--json", create_json]);
    env.run_json(&["milestone", "approve", "01"]);
    env.run_json(&["milestone", "set-status", "01", "in-progress"]);

    let out = env.run(&["validate", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = json["warnings"].as_array().unwrap();
    assert!(warnings.is_empty() || warnings.iter().all(|w| w["code"].as_str() != Some("W31")));
    let w30s: Vec<&serde_json::Value> = warnings
        .iter()
        .filter(|w| w["code"].as_str() == Some("W30"))
        .collect();
    assert!(
        w30s.is_empty(),
        "expected no W30 for empty steps, got: {w30s:?}"
    );
}

#[test]
fn w30_track_drift_done_step_references_pending_track_item() {
    let env = TestEnv::new();

    // Create a tweak track item
    env.run_json(&[
        "track",
        "add",
        "tweak",
        "--title",
        "fix something",
        "--problem",
        "needs fixing",
        "--verification",
        "manual",
    ]);

    let create_json = r#"{
        "title": "Track drift test",
        "intent": { "outcome": "Test W30." },
        "problem": { "description": "Detect track drift." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "W30 fires", "verification": "manual: state tracking setup" }
        ]
    }"#;
    env.run_json(&["milestone", "create", "--json", create_json]);
    env.run_json(&["milestone", "approve", "01"]);
    env.run_json(&["milestone", "decompose", "01"]);
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "Finish fixing TW-01",
        "--done-when",
        "TW-01 closed",
        "--tests",
        "manual",
    ]);
    // Add second step to prevent milestone auto-close
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "second step",
        "--done-when",
        "done",
        "--tests",
        "manual",
    ]);
    env.run_json(&["milestone", "set-status", "01", "in-progress"]);
    env.run_json(&["milestone", "step", "done", "01", "S1"]);

    // The done step references TW-01 which should still be pending
    let out = env.run(&["validate", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = json["warnings"].as_array().unwrap();
    let w30s: Vec<&serde_json::Value> = warnings
        .iter()
        .filter(|w| w["code"].as_str() == Some("W30"))
        .collect();
    assert!(
        !w30s.is_empty(),
        "expected W30 for pending TW-01, got warnings: {warnings:?}"
    );
}

#[test]
fn auto_close_closes_track_refs_on_milestone_auto_close() {
    let env = TestEnv::new();

    // Create a tweak track item
    env.run_json(&[
        "track",
        "add",
        "tweak",
        "--title",
        "auto close test",
        "--problem",
        "needs closing",
        "--verification",
        "manual",
    ]);

    let create_json = r#"{
        "title": "Auto close refs",
        "intent": { "outcome": "Test auto-close." },
        "problem": { "description": "Auto-close TW refs on milestone complete." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "Auto-close works", "verification": "manual: state tracking setup" }
        ]
    }"#;
    env.run_json(&["milestone", "create", "--json", create_json]);
    env.run_json(&["milestone", "approve", "01"]);
    env.run_json(&["milestone", "decompose", "01"]);
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "Close TW-01",
        "--done-when",
        "TW-01 done",
        "--tests",
        "manual",
    ]);
    env.run_json(&["milestone", "set-status", "01", "in-progress"]);

    // Mark step done — auto-closes TW/BF refs when all steps are done
    env.run_json(&["milestone", "step", "done", "01", "S1"]);

    // Check track items to verify TW-01 is done
    let show = env.run_json(&["track", "show", "tweak"]);
    let items = show["items"].as_array().unwrap();
    let tw = items
        .iter()
        .find(|t| t["id"].as_str() == Some("TW-01"))
        .unwrap();
    assert_eq!(
        tw["status"].as_str(),
        Some("done"),
        "TW-01 should be auto-closed when all steps done"
    );
}

#[test]
fn no_w30_when_track_ref_is_already_done() {
    let env = TestEnv::new();

    // Create a tweak and mark it done manually
    env.run_json(&[
        "track",
        "add",
        "tweak",
        "--title",
        "already done",
        "--problem",
        "done",
        "--verification",
        "manual",
    ]);
    env.run_json(&["track", "done", "tweak", "TW-01"]);

    let create_json = r#"{
        "title": "No false W30",
        "intent": { "outcome": "Test no false W30." },
        "problem": { "description": "Done track ref should not trigger W30." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "No W30", "verification": "manual: state tracking setup" }
        ]
    }"#;
    env.run_json(&["milestone", "create", "--json", create_json]);
    env.run_json(&["milestone", "approve", "01"]);
    env.run_json(&["milestone", "decompose", "01"]);
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "Used TW-01 which is done",
        "--done-when",
        "done",
        "--tests",
        "manual",
    ]);
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "second step",
        "--done-when",
        "done",
        "--tests",
        "manual",
    ]);
    env.run_json(&["milestone", "set-status", "01", "in-progress"]);
    env.run_json(&["milestone", "step", "done", "01", "S1"]);

    let out = env.run(&["validate", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = json["warnings"].as_array().unwrap();
    let w30s: Vec<&serde_json::Value> = warnings
        .iter()
        .filter(|w| w["code"].as_str() == Some("W30"))
        .collect();
    assert!(
        w30s.is_empty(),
        "expected no W30 when TW-01 already done, got: {w30s:?}"
    );
}

#[test]
fn w30_track_drift_with_bugfix() {
    let env = TestEnv::new();

    env.run_json(&[
        "track",
        "add",
        "bugfix",
        "--title",
        "critical bug",
        "--problem",
        "crash",
        "--verification",
        "manual",
    ]);

    let create_json = r#"{
        "title": "BF drift test",
        "intent": { "outcome": "Test BF drift." },
        "problem": { "description": "Check BF refs in action." },
        "scope": {
            "in_scope": ["thing"],
            "out_of_scope": ["other1", "other2"]
        },
        "acceptance_criteria": [
            { "description": "BF W30", "verification": "manual: state tracking setup" }
        ]
    }"#;
    env.run_json(&["milestone", "create", "--json", create_json]);
    env.run_json(&["milestone", "approve", "01"]);
    env.run_json(&["milestone", "decompose", "01"]);
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "Fix bug BF-01",
        "--done-when",
        "BF-01 closed",
        "--tests",
        "manual",
    ]);
    env.run_json(&[
        "milestone",
        "step",
        "add",
        "01",
        "--wp",
        "WP1",
        "--action",
        "second step",
        "--done-when",
        "done",
        "--tests",
        "manual",
    ]);
    env.run_json(&["milestone", "set-status", "01", "in-progress"]);
    env.run_json(&["milestone", "step", "done", "01", "S1"]);

    let out = env.run(&["validate", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = json["warnings"].as_array().unwrap();
    let w30s: Vec<&serde_json::Value> = warnings
        .iter()
        .filter(|w| w["code"].as_str() == Some("W30"))
        .collect();
    assert!(
        !w30s.is_empty(),
        "expected W30 for pending BF-01, got warnings: {warnings:?}"
    );
}

#[test]
fn milestone_block_groom_flags_needs_attention() {
    let env = TestEnv::blank();
    assert!(env.run(&["init", "--format", "json"]).status.success());

    let create_json = r#"{
        "title": "Blocked feature",
        "intent": { "outcome": "Works." },
        "problem": { "description": "Need it." },
        "scope": { "in_scope": ["core"], "out_of_scope": ["mobile", "admin"] },
        "acceptance_criteria": [
            { "description": "Works", "verification": "cargo test" }
        ]
    }"#;
    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    for status in ["interview", "review", "ready"] {
        assert!(env
            .run(&[
                "milestone",
                "set-spec-status",
                &id,
                status,
                "--format",
                "json"
            ])
            .status
            .success());
    }

    assert!(env
        .run(&[
            "milestone",
            "block",
            &id,
            "--reason",
            "waiting on design",
            "--format",
            "json",
        ])
        .status
        .success());

    let groom = env.run(&["milestone", "groom", &id, "--format", "json"]);
    assert!(groom.status.success());
    let groom_json: serde_json::Value = serde_json::from_slice(&groom.stdout).unwrap();
    assert_eq!(groom_json["needs_attention"], true);
}
