use crate::common::TestEnv;

/// Helper: create a milestone with one AC and one WP (via plan) and one step
/// under the auto-created WP. Returns (milestone_id, ac_id, wp_id, step_id).
fn setup_milestone_with_fragments(env: &TestEnv) -> (String, String, String, String) {
    let create_json = r#"{
        "title": "Markdown rendering robustness",
        "intent": { "outcome": "Markdown previews are crisp across viewports" },
        "problem": { "description": "Markdown rendering has layout issues in narrow terminals" },
        "scope": {
            "in_scope": ["Markdown rendering", "Robustness across widths"],
            "out_of_scope": ["Print stylesheet", "PDF export"]
        },
        "acceptance_criteria": [
            {
                "id": "AC-01",
                "description": "OAuth login flow completes end-to-end",
                "verification": "cargo test oauth_flow"
            },
            {
                "id": "AC-02",
                "description": "Markdown preview handles headings without overflow",
                "verification": "manual review at 80-col and 120-col"
            }
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
    assert!(
        create.status.success(),
        "create: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = created["milestone"]["id"].as_str().unwrap().to_string();

    env.run(&[
        "milestone",
        "set-spec-status",
        &id,
        "ready",
        "--format",
        "json",
    ]);
    let plan = env.run(&["milestone", "plan", &id, "--format", "json"]);
    assert!(
        plan.status.success(),
        "plan: {}",
        String::from_utf8_lossy(&plan.stderr)
    );

    // Add a step under WP1 with searchable terms.
    let step = env.run(&[
        "milestone",
        "step",
        "add",
        &id,
        "--wp",
        "WP1",
        "--action",
        "Harden markdown renderer for narrow terminals",
        "--tests",
        "cargo test markdown",
        "--done-when",
        "All markdown tests pass",
        "--format",
        "json",
    ]);
    assert!(
        step.status.success(),
        "step add: {}",
        String::from_utf8_lossy(&step.stderr)
    );

    (id, "AC-01".to_string(), "WP1".to_string(), "S1".to_string())
}

// AC-01: source paths reference .json plan files.
#[test]
fn search_returns_json_source_paths() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&["search", "markdown", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(!results.is_empty());
    for r in results {
        let source = r["source"].as_str().unwrap_or("");
        assert!(
            source.ends_with(".json"),
            "source must be a .json path (M92 layout), got: {source}"
        );
    }
}

// AC-02: --type ac returns acceptance_criterion hits with parent_milestone_id.
#[test]
fn search_type_ac_returns_only_acs() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&["search", "OAuth", "--type", "ac", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(!results.is_empty(), "expected AC hits for 'OAuth'");
    for r in results {
        assert_eq!(r["artifact_type"], "acceptance_criterion");
        assert!(
            r["parent_milestone_id"].is_string(),
            "missing parent_milestone_id: {r}"
        );
        assert!(
            r["matched_field"]
                .as_str()
                .unwrap()
                .starts_with("description")
                || r["matched_field"]
                    .as_str()
                    .unwrap()
                    .starts_with("verification"),
            "matched_field should be description or verification: {r}"
        );
    }
}

// AC-03: --type wp returns work_package hits.
#[test]
fn search_type_wp_returns_only_wps() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&["search", "rendering", "--type", "wp", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    for r in results {
        assert_eq!(r["artifact_type"], "work_package");
        assert!(r["parent_milestone_id"].is_string());
        assert!(
            r["matched_field"].as_str().unwrap().starts_with("name")
                || r["matched_field"].as_str().unwrap().starts_with("goal"),
            "matched_field should be name or goal: {r}"
        );
    }
}

// AC-04: --type step matches action, done_when, or tests with qualified id.
#[test]
fn search_type_step_extended_fields() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&["search", "markdown", "--type", "step", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(!results.is_empty());
    let mut fields = std::collections::HashSet::new();
    for r in results {
        assert_eq!(r["artifact_type"], "step");
        let id = r["id"].as_str().unwrap();
        assert!(
            id.contains('/'),
            "step id must be milestone-qualified: {id}"
        );
        let field = r["matched_field"].as_str().unwrap().to_string();
        assert!(
            field == "action" || field == "done_when" || field == "tests",
            "matched_field must be action|done_when|tests, got {field}"
        );
        fields.insert(field);
    }
    // The fixture covers action + tests + done_when for "markdown".
    assert!(
        fields.contains("action") || fields.contains("tests"),
        "expected at least one of action/tests to match, got {fields:?}"
    );
}

// AC-05: --type title returns milestone title hits only (not intent/problem body).
#[test]
fn search_type_title_filters_to_titles() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&[
        "search",
        "robustness",
        "--type",
        "title",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    for r in results {
        assert_eq!(r["artifact_type"], "milestone");
        assert_eq!(r["matched_field"], "title");
    }
}

// AC-05 negative: --type title for a word only present in intent.outcome
// (not the title) must return nothing.
#[test]
fn search_type_title_does_not_match_intent_body() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    // "previews" is in intent.outcome, not in title.
    let out = env.run(&["search", "previews", "--type", "title", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(
        results.is_empty(),
        "title scope must not match intent body, got {results:?}"
    );
}

// AC-06: --include object embeds full matched fragment; default omits it.
#[test]
fn search_include_object_embeds_full_fragment() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);

    // Default (snippet) — no `object` key on any hit.
    let out = env.run(&["search", "OAuth", "--type", "ac", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    for r in v["results"].as_array().unwrap() {
        assert!(r.get("object").is_none(), "default must omit object: {r}");
    }

    // --include object — first hit must carry a full AC fragment.
    let out2 = env.run(&[
        "search",
        "OAuth",
        "--type",
        "ac",
        "--include",
        "object",
        "--format",
        "json",
    ]);
    assert!(out2.status.success());
    let v2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    let results = v2["results"].as_array().unwrap();
    assert!(!results.is_empty());
    let first = &results[0];
    let obj = first
        .get("object")
        .expect("object must be present with --include object");
    let obj = obj.as_object().expect("object should be a JSON object");
    assert!(obj.contains_key("id"), "AC fragment must carry id: {obj:?}");
    assert!(obj.contains_key("description"));
    assert!(obj.contains_key("verification"));
    assert_eq!(obj["id"].as_str().unwrap(), "AC-01");
}

// AC-06 negative: invalid --include value errors clearly.
#[test]
fn search_include_rejects_unknown_value() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&[
        "search",
        "OAuth",
        "--type",
        "ac",
        "--include",
        "fulltext",
        "--format",
        "json",
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid --include"),
        "expected clear error, got: {stderr}"
    );
}

// AC-07: hits carry a suggested_action mapping to a M93 fragment command.
#[test]
fn search_hits_carry_suggested_action() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);

    // Acceptance criterion.
    let out = env.run(&["search", "OAuth", "--type", "ac", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let r = &v["results"][0];
    let sa = r["suggested_action"].as_str().unwrap();
    assert!(
        sa.contains("mp milestone ac show"),
        "AC suggested_action should map to ac show, got: {sa}"
    );

    // Step.
    let out2 = env.run(&["search", "markdown", "--type", "step", "--format", "json"]);
    let v2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    let r2 = &v2["results"][0];
    let sa2 = r2["suggested_action"].as_str().unwrap();
    assert!(
        sa2.contains("mp milestone step show"),
        "step suggested_action should map to step show, got: {sa2}"
    );

    // Work package.
    let out3 = env.run(&["search", "rendering", "--type", "wp", "--format", "json"]);
    let v3: serde_json::Value = serde_json::from_slice(&out3.stdout).unwrap();
    let r3 = &v3["results"][0];
    let sa3 = r3["suggested_action"].as_str().unwrap();
    assert!(
        sa3.contains("mp show milestone") && sa3.contains("--fields"),
        "WP suggested_action should map to show milestone with fields, got: {sa3}"
    );
}

// AC-08: nonexistent and empty queries return empty results with exit 0.
#[test]
fn search_nonexistent_and_empty_queries() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);

    let out = env.run(&["search", "zzz-nonexistent", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["results"].as_array().unwrap().len(), 0);

    let out2 = env.run(&["search", "", "--format", "json"]);
    assert!(out2.status.success());
    let v2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(v2["results"].as_array().unwrap().len(), 0);
}

// Bonus: --group-by milestone returns grouped structure.
#[test]
fn search_group_by_milestone() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&[
        "search",
        "markdown",
        "--type",
        "ac",
        "--group-by",
        "milestone",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let groups = v["groups"].as_array().expect("groups array");
    assert!(!groups.is_empty());
    for g in groups {
        assert!(g["milestone"].is_string());
        assert!(g["hits"].is_array());
        for h in g["hits"].as_array().unwrap() {
            assert_eq!(h["artifact_type"], "acceptance_criterion");
        }
    }
}

// F-11: --type milestone must include title (regression from M53).
#[test]
fn search_type_milestone_includes_title() {
    let env = TestEnv::new();
    let (_id, _ac, _wp, _step) = setup_milestone_with_fragments(&env);
    let out = env.run(&[
        "search",
        "Markdown",
        "--type",
        "milestone",
        "--format",
        "json",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(
        results.iter().any(|r| r["matched_field"] == "title"),
        "--type milestone must include title hits, got: {results:?}"
    );
    // High score (substring match on title).
    let title_hit = results
        .iter()
        .find(|r| r["matched_field"] == "title")
        .unwrap();
    assert!(
        title_hit["score"].as_f64().unwrap() >= 0.85,
        "title hit should score ≥ 0.85 (substring match), got {}",
        title_hit["score"]
    );
}

// F-11: --type milestone must include scope.in_scope and scope.out_of_scope.
#[test]
fn search_type_milestone_includes_scope_lines() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    // "Markdown" is in scope.in_scope; "Print" is in scope.out_of_scope.
    let out = env.run(&[
        "search",
        "Markdown",
        "--type",
        "milestone",
        "--format",
        "json",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(
        results
            .iter()
            .any(|r| r["matched_field"] == "scope.in_scope"),
        "--type milestone must include scope.in_scope hits, got: {results:?}"
    );

    let out2 = env.run(&["search", "Print", "--type", "milestone", "--format", "json"]);
    let v2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    let results2 = v2["results"].as_array().unwrap();
    assert!(
        results2
            .iter()
            .any(|r| r["matched_field"] == "scope.out_of_scope"),
        "--type milestone must include scope.out_of_scope hits, got: {results2:?}"
    );
}

// F-12: --type all is normalized to no filter (matches every artifact type).
#[test]
fn search_type_all_normalizes_to_no_filter() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out_no_type = env.run(&["search", "markdown", "--format", "json"]);
    let out_all = env.run(&["search", "markdown", "--type", "all", "--format", "json"]);
    let v_no: serde_json::Value = serde_json::from_slice(&out_no_type.stdout).unwrap();
    let v_all: serde_json::Value = serde_json::from_slice(&out_all.stdout).unwrap();
    assert_eq!(
        v_no["results"].as_array().unwrap().len(),
        v_all["results"].as_array().unwrap().len(),
        "--type all must produce the same results as omitting --type"
    );
}

// F-13: --type with an unknown value errors clearly.
#[test]
fn search_type_rejects_unknown_value() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&["search", "markdown", "--type", "bogus", "--format", "json"]);
    assert!(!out.status.success(), "unknown --type must error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid --type"), "stderr: {stderr}");
    assert!(
        stderr.contains("milestone") && stderr.contains("title") && stderr.contains("ac"),
        "stderr should list valid types: {stderr}"
    );
}

// F-14: milestone hits do NOT carry parent_milestone_id (it's redundant
// with id since milestones are the parent themselves).
#[test]
fn search_milestone_hits_omit_parent_milestone_id() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&[
        "search",
        "Markdown",
        "--type",
        "milestone",
        "--format",
        "json",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    for r in v["results"].as_array().unwrap() {
        assert_eq!(r["artifact_type"], "milestone");
        assert!(
            r.get("parent_milestone_id").is_none(),
            "milestone hits must omit parent_milestone_id: {r}"
        );
    }
}

// F-14: nested hits (ac, step, wp) still carry parent_milestone_id.
#[test]
fn search_nested_hits_carry_parent_milestone_id() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&["search", "OAuth", "--type", "ac", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(!results.is_empty());
    for r in results {
        assert!(
            r["parent_milestone_id"].is_string(),
            "AC hits must carry parent_milestone_id: {r}"
        );
    }
}

// F-14: group_by_milestone groups milestone hits under themselves (using id).
#[test]
fn search_group_by_milestone_groups_milestone_under_itself() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    let out = env.run(&[
        "search",
        "Markdown",
        "--group-by",
        "milestone",
        "--format",
        "json",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let groups = v["groups"].as_array().unwrap();
    let milestone_group = groups
        .iter()
        .find(|g| g["milestone"].as_str() == Some("M01"))
        .expect("expected M01 group");
    let hits = milestone_group["hits"].as_array().unwrap();
    assert!(
        hits.iter().any(|h| h["artifact_type"] == "milestone"),
        "milestone hit must group under its own id, got: {hits:?}"
    );
}

// Group ordering: the synthetic "(none)" group (non-milestone artifacts)
// must sort LAST, after every milestone group — not first by ASCII.
#[test]
fn search_group_by_orders_none_group_last() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);
    // Create a non-milestone artifact (idea) that matches the same query.
    let idea = env.run(&[
        "idea",
        "create",
        "--title",
        "Markdown robustness idea",
        "--body",
        "out of band",
        "--format",
        "json",
    ]);
    assert!(
        idea.status.success(),
        "idea create: {}",
        String::from_utf8_lossy(&idea.stderr)
    );

    let out = env.run(&[
        "search",
        "Markdown",
        "--group-by",
        "milestone",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let groups = v["groups"].as_array().unwrap();

    let none_idx = groups
        .iter()
        .position(|g| g["milestone"].as_str() == Some("(none)"))
        .expect("expected a (none) group from the idea hit");
    // Every milestone group must precede the (none) group.
    for (i, g) in groups.iter().enumerate() {
        if i < none_idx {
            assert_ne!(
                g["milestone"].as_str(),
                Some("(none)"),
                "(none) must be last; found milestone group after it"
            );
        }
    }
    assert_eq!(none_idx, groups.len() - 1, "(none) must be the final group");
}

// L-2: --include object on milestone hits returns milestone fragment fields.
#[test]
fn search_include_object_on_milestone() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);

    let out = env.run(&[
        "search",
        "Markdown",
        "--type",
        "milestone",
        "--include",
        "object",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(
        !results.is_empty(),
        "expected milestone hits for 'Markdown'"
    );
    let first = &results[0];
    let obj = first
        .get("object")
        .expect("object must be present with --include object");
    let obj = obj.as_object().expect("object should be a JSON object");
    // A milestone hit's object is a MilestoneFile: title lives in milestone.title, intent at top level.
    assert!(
        obj.contains_key("milestone"),
        "milestone object must carry milestone sub-field: {obj:?}"
    );
    assert!(
        obj["milestone"].as_object().unwrap().contains_key("title"),
        "milestone sub-object must carry title: {obj:?}"
    );
    assert!(
        obj.contains_key("intent"),
        "milestone object must carry intent: {obj:?}"
    );
}

// L-2: --include object on step hits returns step fragment fields.
#[test]
fn search_include_object_on_step() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);

    let out = env.run(&[
        "search",
        "markdown",
        "--type",
        "step",
        "--include",
        "object",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(!results.is_empty(), "expected step hits for 'markdown'");
    let first = &results[0];
    let obj = first
        .get("object")
        .expect("object must be present with --include object");
    let obj = obj.as_object().expect("object should be a JSON object");
    assert!(
        obj.contains_key("action"),
        "step object must carry action: {obj:?}"
    );
    assert!(
        obj.contains_key("done_when"),
        "step object must carry done_when: {obj:?}"
    );
    assert!(
        obj.contains_key("tests"),
        "step object must carry tests: {obj:?}"
    );
}

// L-2: --include object on wp hits returns work_package fragment fields.
#[test]
fn search_include_object_on_wp() {
    let env = TestEnv::new();
    setup_milestone_with_fragments(&env);

    let out = env.run(&[
        "search",
        "rendering",
        "--type",
        "wp",
        "--include",
        "object",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(!results.is_empty(), "expected wp hits for 'rendering'");
    let first = &results[0];
    let obj = first
        .get("object")
        .expect("object must be present with --include object");
    let obj = obj.as_object().expect("object should be a JSON object");
    assert!(
        obj.contains_key("name"),
        "wp object must carry name: {obj:?}"
    );
    assert!(
        obj.contains_key("goal"),
        "wp object must carry goal: {obj:?}"
    );
}
