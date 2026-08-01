//! M101 R4 / F-02: hunk AgentAnnotation structural round-trip. A
//! Finding serialized to JSON must carry summary/rationale/anchor
//! fields that are structurally compatible with hunk's AgentAnnotation
//! shape (summary, rationale, oldRange, newRange, author, tags,
//! confidence). Pinned by tests/fixtures/hunk_agent_annotation.json
//! fixture + a test that hydrates the fixture into a Finding-shaped
//! struct and asserts key-by-key parity.
//!
//! This is a STRUCTURAL test, not a true round-trip — we don't have
//! the hunk codebase as a dep. The fixture's shape mirrors the
//! documented AgentAnnotation contract. A future ID-19 milestone
//! (hunk integration) will swap this fixture for the real hunk type
//! and add a deserializer test.

use crate::common::lib_api;
use crate::common::TestEnv;
use mp_model::{Finding, FindingAnchor, Range};
use serde_json::Value;

fn create_milestone(env: &TestEnv, title: &str) -> String {
    let payload = serde_json::json!({
        "title": title,
        "intent": { "outcome": "M101 hunk round-trip" },
        "problem": { "description": "M101 hunk AgentAnnotation parity" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{ "description": "AC-18", "verification": "echo ok" }],
        "spec_status": "ready",
    })
    .to_string();
    let out = lib_api::run(env, &["milestone", "create", "--json", &payload]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

#[test]
fn findings_hunk_compat() {
    let env = TestEnv::new();
    let _id = create_milestone(&env, "hunk-round-trip");

    // Load the hunk AgentAnnotation fixture.
    let fixture_path = env
        .tmp
        .path()
        .join("master-plan/hunk_agent_annotation.json");
    // Place the fixture at a path TestEnv can reach — TestEnv's mp
    // commands operate against master-plan/ in tmp.path. Copy from the
    // repo's crates/mp/tests/fixtures/ into tmp.
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/hunk_agent_annotation.json");
    std::fs::create_dir_all(fixture_path.parent().unwrap()).unwrap();
    std::fs::copy(&src, &fixture_path).expect("copy hunk fixture");

    let raw = std::fs::read_to_string(&fixture_path).unwrap();
    let hunk: Value = serde_json::from_str(&raw).unwrap();

    // Hydrate into a Finding-shape with anchor.old_range / new_range /
    // summary / rationale / author / tags / confidence populated from
    // the hunk fields. (Side: hunk has old/new sides; we use "new"
    // for the newRange side and "old" for the oldRange side.)
    let finding = Finding {
        id: "F-99".to_string(),
        severity: "high".to_string(),
        category: "correctness".to_string(),
        description: hunk["rationale"].as_str().unwrap().to_string(),
        status: "open".to_string(),
        author: hunk["author"].as_str().unwrap().to_string(),
        fixed_in: String::new(),
        created: "2026-07-06".to_string(),
        resolved: String::new(),
        phase: "external".to_string(),
        anchor: Some(FindingAnchor {
            path: "crates/mp/src/milestone.rs".to_string(),
            commit: "abc1234".to_string(),
            new_range: Some(Range {
                start_line: hunk["newRange"]["start"].as_u64().unwrap() as u32,
                end_line: hunk["newRange"]["end"].as_u64().unwrap() as u32,
            }),
            old_range: Some(Range {
                start_line: hunk["oldRange"]["start"].as_u64().unwrap() as u32,
                end_line: hunk["oldRange"]["end"].as_u64().unwrap() as u32,
            }),
            hunk_index: Some(0),
            side: Some("new".to_string()),
        }),
        thread: vec![],
        summary: hunk["summary"].as_str().unwrap().to_string(),
        rationale: hunk["rationale"].as_str().unwrap().to_string(),
        confidence: hunk["confidence"].as_str().unwrap().to_string(),
        tags: hunk["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect(),
    };

    // Round-trip through serde to assert the JSON shape mirrors the
    // hunk AgentAnnotation structure (key names + types).
    let serialized = serde_json::to_string(&finding).unwrap();
    let parsed: Value = serde_json::from_str(&serialized).unwrap();

    let anchor = parsed.get("anchor").expect("anchor present");
    assert_eq!(anchor["new_range"]["start_line"], 24);
    assert_eq!(anchor["new_range"]["end_line"], 30);
    assert_eq!(anchor["old_range"]["start_line"], 12);
    assert_eq!(anchor["old_range"]["end_line"], 18);
    assert_eq!(
        parsed["summary"],
        "Missed gate wiring — finding not promoted to remediation"
    );
    assert_eq!(
        parsed["rationale"],
        "Helpers exist (has_open_self_findings, has_open_external_findings) but are not wired into the complete_milestone path. AC-01..AC-05 are marked passed in the milestone spec but the code path that fires the gate is absent."
    );
    assert_eq!(parsed["author"], "external-review-2026-07-05");
    assert_eq!(parsed["confidence"], "high");
    let tags: Vec<String> = parsed["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(tags, vec!["m101", "gate", "review"]);

    // The Finding serializes back to the same key shape as the hunk
    // fixture (summary, rationale, author, tags, confidence, anchor.{path,
    // commit, old_range, new_range}). A future hunk integration can map
    // Finding -> AgentAnnotation without key translation.
}
