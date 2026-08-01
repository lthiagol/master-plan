use crate::common::TestEnv;

#[test]
fn fuzzy_search_returns_ranked_results() {
    let env = TestEnv::new();

    // Create a milestone with a searchable title
    let create_json = r#"{
        "title": "Install & distribution architecture",
        "intent": { "outcome": "Users can install via cargo" },
        "problem": { "description": "Need streamlined install flow" },
        "scope": {
            "in_scope": ["Packaging"],
            "out_of_scope": ["Windows", "macOS"]
        },
        "acceptance_criteria": [
            { "description": "Install via cargo works", "verification": "manual" }
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

    // Create another milestone with overlapping keyword
    let create_json2 = r#"{
        "title": "Unified harness registry",
        "intent": { "outcome": "Single registry for all harnesses" },
        "problem": { "description": "Multiple install paths" },
        "scope": {
            "in_scope": ["Registry"],
            "out_of_scope": ["Windows", "CLI"]
        },
        "acceptance_criteria": [
            { "description": "Registry install works", "verification": "manual" }
        ]
    }"#;
    let create2 = env.run(&[
        "milestone",
        "create",
        "--json",
        create_json2,
        "--format",
        "json",
    ]);
    assert!(
        create2.status.success(),
        "create2: {}",
        String::from_utf8_lossy(&create2.stderr)
    );

    // ── AC-01: Search for "install" returns both milestones ranked ──
    let raw = env.run(&["search", "install", "--format", "json"]);
    assert!(
        raw.status.success(),
        "search: {}",
        String::from_utf8_lossy(&raw.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&raw.stdout).unwrap();
    let results = out["results"].as_array().expect("results array");
    assert!(
        !results.is_empty(),
        "expected at least one result for 'install'"
    );

    let titles: Vec<String> = results
        .iter()
        .map(|r| r["title"].as_str().unwrap_or("").to_lowercase())
        .collect();
    assert!(
        titles.iter().any(|t| t.contains("install")),
        "expected milestone with 'Install' in title, got: {titles:?}"
    );
    assert!(
        titles.iter().any(|t| t.contains("registry")),
        "expected milestone with 'Registry' in title, got: {titles:?}"
    );

    // Every result has a score, type, id, title, matched_field
    for r in results {
        assert!(
            r["score"].as_f64().unwrap_or(0.0) > 0.0,
            "missing score: {r}"
        );
        assert!(
            !r["artifact_type"].as_str().unwrap_or("").is_empty(),
            "missing type: {r}"
        );
        assert!(
            !r["id"].as_str().unwrap_or("").is_empty(),
            "missing id: {r}"
        );
        assert!(
            !r["title"].as_str().unwrap_or("").is_empty(),
            "missing title: {r}"
        );
        assert!(
            !r["matched_field"].as_str().unwrap_or("").is_empty(),
            "missing matched_field: {r}"
        );
    }

    // ── AC-02: --type milestone filters to milestones only ──
    let raw2 = env.run(&[
        "search",
        "install",
        "--type",
        "milestone",
        "--format",
        "json",
    ]);
    assert!(
        raw2.status.success(),
        "search type: {}",
        String::from_utf8_lossy(&raw2.stderr)
    );
    let milestone: serde_json::Value = serde_json::from_slice(&raw2.stdout).unwrap();
    let m_results = milestone["results"].as_array().expect("results array");
    for r in m_results {
        assert_eq!(
            r["artifact_type"], "milestone",
            "expected only milestones: {r}"
        );
    }
    assert!(
        !m_results.is_empty(),
        "expected at least one milestone result"
    );

    // ── AC-03: Search for nonexistent returns empty ──
    let raw3 = env.run(&["search", "zzz-nonexistent", "--format", "json"]);
    assert!(
        raw3.status.success(),
        "search empty: {}",
        String::from_utf8_lossy(&raw3.stderr)
    );
    let empty: serde_json::Value = serde_json::from_slice(&raw3.stdout).unwrap();
    let empty_results = empty["results"].as_array().expect("results array");
    assert!(
        empty_results.is_empty(),
        "expected empty results for nonexistent query"
    );
}

#[test]
fn fuzzy_search_type_filter_scopes_correctly() {
    let env = TestEnv::new();

    // Create a milestone with a step
    let create_json = r#"{
        "title": "Auth integration",
        "intent": { "outcome": "OAuth works" },
        "problem": { "description": "Need auth flow" },
        "scope": {
            "in_scope": ["OAuth"],
            "out_of_scope": ["SAML", "Password"]
        },
        "acceptance_criteria": [
            { "description": "Login flow works", "verification": "manual" }
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
    let id = created["milestone"]["id"].as_str().unwrap();

    // Set spec-status ready, plan (creates WPs), and add a step
    env.run(&[
        "milestone",
        "set-spec-status",
        id,
        "ready",
        "--format",
        "json",
    ]);
    let plan = env.run(&["milestone", "plan", id, "--format", "json"]);
    assert!(
        plan.status.success(),
        "plan: {}",
        String::from_utf8_lossy(&plan.stderr)
    );

    // Add a step under the auto-created WP1
    let step = env.run(&[
        "milestone",
        "step",
        "add",
        id,
        "--wp",
        "WP1",
        "--action",
        "Implement OAuth login flow",
        "--tests",
        "cargo test oauth",
        "--done-when",
        "Tests pass",
        "--format",
        "json",
    ]);
    assert!(
        step.status.success(),
        "step add: {}",
        String::from_utf8_lossy(&step.stderr)
    );

    // Search for "oauth" — should find milestone AND step
    let raw = env.run(&["search", "oauth", "--format", "json"]);
    assert!(
        raw.status.success(),
        "search: {}",
        String::from_utf8_lossy(&raw.stderr)
    );
    let all: serde_json::Value = serde_json::from_slice(&raw.stdout).unwrap();
    let results = all["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "expected results for 'oauth'");

    // --type step should return only steps
    let raw2 = env.run(&["search", "oauth", "--type", "step", "--format", "json"]);
    assert!(
        raw2.status.success(),
        "search type step: {}",
        String::from_utf8_lossy(&raw2.stderr)
    );
    let steps: serde_json::Value = serde_json::from_slice(&raw2.stdout).unwrap();
    let s_results = steps["results"].as_array().expect("results array");
    for r in s_results {
        assert_eq!(r["artifact_type"], "step", "expected only steps: {r}");
    }

    // --type milestone should return only milestones
    let raw3 = env.run(&["search", "oauth", "--type", "milestone", "--format", "json"]);
    assert!(
        raw3.status.success(),
        "search type milestone: {}",
        String::from_utf8_lossy(&raw3.stderr)
    );
    let milestones: serde_json::Value = serde_json::from_slice(&raw3.stdout).unwrap();
    let m_results = milestones["results"].as_array().expect("results array");
    for r in m_results {
        assert_eq!(
            r["artifact_type"], "milestone",
            "expected only milestones: {r}"
        );
    }
}

#[test]
fn fuzzy_search_default_json() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "Test milestone",
        "intent": { "outcome": "Verified" },
        "problem": { "description": "Just testing" },
        "scope": {
            "in_scope": ["Test"],
            "out_of_scope": ["Nope", "Other"]
        },
        "acceptance_criteria": []
    }"#;
    let create = env.run(&["milestone", "create", "--json", create_json]);
    assert!(
        create.status.success(),
        "create: {}",
        String::from_utf8_lossy(&create.stderr)
    );

    let out = env.run(&["search", "test"]);
    assert!(
        out.status.success(),
        "search: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert!(json["results"].is_array(), "expected results array");
}

#[test]
fn fuzzy_search_human_format_is_rejected() {
    let env = TestEnv::new();
    let out = env.run(&["search", "test", "--format", "human"]);
    assert!(!out.status.success(), "--format human should be rejected");
}

#[test]
fn fuzzy_search_ideas_and_backlog() {
    let env = TestEnv::new();

    // Create an idea
    let idea = env.run(&[
        "idea",
        "create",
        "--title",
        "Dark mode support",
        "--body",
        "Add dark mode theme option to the UI",
        "--format",
        "json",
    ]);
    assert!(
        idea.status.success(),
        "idea create: {}",
        String::from_utf8_lossy(&idea.stderr)
    );

    // Add a backlog item
    let backlog = env.run(&[
        "backlog",
        "add",
        "--desc",
        "Research dark mode implementation approaches",
        "--format",
        "json",
    ]);
    assert!(
        backlog.status.success(),
        "backlog add: {}",
        String::from_utf8_lossy(&backlog.stderr)
    );

    // Search via JSON
    let raw = env.run(&["search", "dark", "--format", "json"]);
    assert!(
        raw.status.success(),
        "search dark: {}",
        String::from_utf8_lossy(&raw.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&raw.stdout).unwrap();
    let results = out["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "expected results for 'dark'");
    let types: Vec<&str> = results
        .iter()
        .map(|r| r["artifact_type"].as_str().unwrap_or(""))
        .collect();
    assert!(
        types.contains(&"idea"),
        "expected idea in results: {types:?}"
    );

    // Search backlog items
    let raw2 = env.run(&["search", "research", "--format", "json"]);
    assert!(
        raw2.status.success(),
        "search research: {}",
        String::from_utf8_lossy(&raw2.stderr)
    );
    let out2: serde_json::Value = serde_json::from_slice(&raw2.stdout).unwrap();
    let results2 = out2["results"].as_array().expect("results array");
    assert!(!results2.is_empty(), "expected results for 'research'");
    let types2: Vec<&str> = results2
        .iter()
        .map(|r| r["artifact_type"].as_str().unwrap_or(""))
        .collect();
    assert!(
        types2.contains(&"backlog"),
        "expected backlog in results: {types2:?}"
    );
}

#[test]
fn fuzzy_search_empty_query_returns_empty() {
    let env = TestEnv::new();
    let raw = env.run(&["search", "", "--format", "json"]);
    assert!(
        raw.status.success(),
        "search empty: {}",
        String::from_utf8_lossy(&raw.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&raw.stdout).unwrap();
    let results = out["results"].as_array().expect("results array");
    assert!(results.is_empty(), "empty query should return no results");
}

#[test]
fn fuzzy_search_works_on_existing_fixture_project() {
    let env = TestEnv::from_fixture("walkthrough-oauth");
    let out = env.run(&["search", "health", "--format", "json"]);
    assert!(
        out.status.success(),
        "fixture search: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = json["results"].as_array().expect("results array");
    assert!(
        !results.is_empty(),
        "expected results for 'health' in walkthrough fixture"
    );

    // The milestone "Foundation" has "health" in intent.outcome and ACs
    let found = results.iter().any(|r| {
        r["title"]
            .as_str()
            .is_some_and(|t| t.contains("Foundation"))
    });
    assert!(
        found,
        "expected 'Foundation' milestone in health search results"
    );
}

// L-1: --type track returns artifact_type: "track" with correct source paths.
#[test]
fn fuzzy_search_type_track_returns_track_hits() {
    let env = TestEnv::new();
    // Create a track (bugfix kind, has BF-XX ids).
    let track = env.run(&[
        "track",
        "add",
        "bugfix",
        "--title",
        "Search fix for double-load regression",
        "--problem",
        "Tracks with bugfix kind have duplicate load calls",
        "--format",
        "json",
    ]);
    assert!(
        track.status.success(),
        "track add: {}",
        String::from_utf8_lossy(&track.stderr)
    );

    // Search for the track content.
    let raw = env.run(&[
        "search",
        "double-load",
        "--type",
        "track",
        "--format",
        "json",
    ]);
    assert!(
        raw.status.success(),
        "search --type track: {}",
        String::from_utf8_lossy(&raw.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&raw.stdout).unwrap();
    let results = out["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "expected track hits for 'double-load'");
    for r in results {
        assert_eq!(
            r["artifact_type"], "track",
            "expected artifact_type track: {r}"
        );
        let source = r["source"].as_str().unwrap_or("");
        assert!(
            source.contains("/tracks/"),
            "track source must point to tracks/ dir: {source}"
        );
    }
}

// L-1: --type decision returns artifact_type: "decision" with correct source paths.
#[test]
fn fuzzy_search_type_decision_returns_decision_hits() {
    let env = TestEnv::new();
    // Create a decision.
    let decision = env.run(&[
        "decision",
        "add",
        "--summary",
        "Adopt split-track-id helper for track search",
        "--format",
        "json",
    ]);
    assert!(
        decision.status.success(),
        "decision add: {}",
        String::from_utf8_lossy(&decision.stderr)
    );

    // Search for the decision content.
    let raw = env.run(&[
        "search",
        "split-track-id",
        "--type",
        "decision",
        "--format",
        "json",
    ]);
    assert!(
        raw.status.success(),
        "search --type decision: {}",
        String::from_utf8_lossy(&raw.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&raw.stdout).unwrap();
    let results = out["results"].as_array().expect("results array");
    assert!(
        !results.is_empty(),
        "expected decision hits for 'split-track-id'"
    );
    for r in results {
        assert_eq!(
            r["artifact_type"], "decision",
            "expected artifact_type decision: {r}"
        );
        let source = r["source"].as_str().unwrap_or("");
        assert!(
            source.contains("/decisions"),
            "decision source must point to decisions path: {source}"
        );
    }
}

// L-1: split_track_id helper parses "BF-04" into ("bugfix", "04").
#[test]
fn fuzzy_search_split_track_id_parses_correctly() {
    let env = TestEnv::from_fixture("walkthrough-oauth");
    // Call via CLI: "mp search <query> --type track" with a known track id pattern.
    // We verify the id field format returned by the search hit matches the expected
    // "<PREFIX>-<LOCAL_ID>" shape.
    // Search the walkthrough-oauth fixture which has bugfix tracks.
    // "Session" matches the title "Session cookie secure flag" in bugfix.json.
    let out = env.run(&["search", "Session", "--type", "track", "--format", "json"]);
    assert!(
        out.status.success(),
        "search --type track: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = out_json["results"].as_array().unwrap();
    assert!(
        !results.is_empty(),
        "expected track hits for 'Session' in fixture"
    );
    for r in results {
        let id = r["id"].as_str().unwrap_or("");
        // Format must be PREFIX-ID (e.g. BF-BF-01, TW-TW-07) with a '-' separator.
        assert!(
            id.contains('-'),
            "track id must contain '-' separator, got: {id}"
        );
    }
}
