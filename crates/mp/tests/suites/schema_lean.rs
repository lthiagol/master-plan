//! M82: lean spec model — dropped ceremony fields must not appear in schema, create, or show.

use std::fs;

use crate::common::lib_api;
use crate::common::TestEnv;
use serde_json::Value;

const DROPPED_TOP_LEVEL: &[&str] = &[
    "behavior",
    "context",
    "requirements",
    "success_criteria",
    "assumptions",
    "interface",
    "risks",
    "technical_context",
    "follow_ups",
];

fn milestone_schema() -> Value {
    serde_json::from_str(
        &fs::read_to_string(crate::common::repo_root().join("schemas/milestone.schema.json"))
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn schema_omits_dropped_ceremony_fields() {
    let schema = milestone_schema();
    let props = schema["properties"].as_object().unwrap();
    for key in DROPPED_TOP_LEVEL {
        assert!(
            !props.contains_key(*key),
            "milestone.schema.json must not define dropped field {key}"
        );
    }
}

#[test]
fn create_example_is_lean() {
    let env = TestEnv::new();
    let out = lib_api::run(&env, &["milestone", "create", "--example"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for key in DROPPED_TOP_LEVEL {
        assert!(
            v.get(*key).is_none(),
            "create --example must not include {key}"
        );
    }
}

#[test]
fn show_milestone_emits_lean_json() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "Lean show test",
        "intent": { "outcome": "Lean output only." },
        "problem": { "description": "Test lean show." },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [
            { "description": "works", "verification": "manual: accepted — test" }
        ]
    }"#;
    let created = lib_api::run(&env, &["milestone", "create", "--json", create_json]);
    assert!(created.status.success());
    let id = serde_json::from_slice::<Value>(&created.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = lib_api::run(&env, &["show", "milestone", &id]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for key in DROPPED_TOP_LEVEL {
        assert!(v.get(*key).is_none(), "mp show must not emit {key}");
    }
    assert!(v.get("intent").is_some());
    assert!(v.get("acceptance_criteria").is_some());
}

#[test]
fn create_rejects_dropped_ceremony_fields() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "Bad create",
        "intent": { "outcome": "x" },
        "problem": { "description": "y" },
        "scope": { "in_scope": ["a"], "out_of_scope": ["b", "c"] },
        "acceptance_criteria": [
            { "description": "z", "verification": "manual: accepted — test" }
        ],
        "behavior": { "scenarios": [] }
    }"#;
    let out = lib_api::run(&env, &["milestone", "create", "--json", create_json]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("behavior"),
        "create must reject dropped ceremony fields: {err}"
    );
}

#[test]
fn question_add_and_resolve_use_open_questions() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "Question cmd test",
        "intent": { "outcome": "Questions work." },
        "problem": { "description": "Test open_questions commands." },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [
            { "description": "works", "verification": "manual: accepted — test" }
        ]
    }"#;
    let created = lib_api::run(&env, &["milestone", "create", "--json", create_json]);
    assert!(created.status.success());
    let id = serde_json::from_slice::<Value>(&created.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let added = lib_api::run(
        &env,
        &[
            "milestone",
            "question",
            "add",
            &id,
            "--text",
            "Need API shape?",
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let qid = serde_json::from_slice::<Value>(&added.stdout).unwrap()["question"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resolved = lib_api::run(
        &env,
        &[
            "milestone",
            "question",
            "resolve",
            &id,
            &qid,
            "--resolution",
            "REST JSON only",
        ],
    );
    assert!(
        resolved.status.success(),
        "{}",
        String::from_utf8_lossy(&resolved.stderr)
    );

    let shown = lib_api::run(&env, &["show", "milestone", &id]);
    assert!(shown.status.success());
    let v: Value = serde_json::from_slice(&shown.stdout).unwrap();
    let questions = v["open_questions"].as_array().unwrap();
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0]["status"], "resolved");
    assert_eq!(questions[0]["answer"], "REST JSON only");
}

#[test]
fn interview_checklist_uses_lean_fields() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "Lean checklist",
        "intent": { "outcome": "Checklist lean." },
        "problem": { "description": "Interview fields." },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [
            { "description": "works", "verification": "manual: accepted — test" }
        ]
    }"#;
    let created = lib_api::run(&env, &["milestone", "create", "--json", create_json]);
    assert!(created.status.success());
    let id = serde_json::from_slice::<Value>(&created.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = lib_api::run(
        &env,
        &[
            "interview",
            "checklist",
            "--checklist-type",
            "milestone",
            "--id",
            &id,
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["ready_for_review"].as_bool().unwrap());
    let missing = v["missing"].as_array().unwrap();
    for key in missing {
        let field = key.as_str().unwrap();
        assert!(
            !DROPPED_TOP_LEVEL.contains(&field),
            "interview checklist must not require dropped field {field}"
        );
    }
}

#[test]
fn written_milestone_file_has_no_ceremony_sections() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "Lean disk test",
        "intent": { "outcome": "No scaffolding on disk." },
        "problem": { "description": "Disk lean." },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [
            { "description": "works", "verification": "manual: accepted — test" }
        ]
    }"#;
    let created = lib_api::run(&env, &["milestone", "create", "--json", create_json]);
    assert!(created.status.success());
    let dir = env.tmp.path().join("master-plan/milestones");
    let raw =
        fs::read_to_string(fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path()).unwrap();
    for marker in [
        "[behavior]",
        "[context]",
        "[requirements]",
        "[interface]",
        "[technical_context]",
        "success_criteria",
        "assumptions",
        "follow_ups",
    ] {
        assert!(
            !raw.contains(marker),
            "on-disk milestone must not contain ceremony marker {marker}:\n{raw}"
        );
    }
}
