//! Schema-specific fixture tests for mini_schema: a valid milestone passes and
//! each rejection reason is caught.

use mp::mini_schema::Validator;

use serde_json::{json, Value};

use crate::common::repo_root;

fn milestone_schema() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("schemas/milestone.schema.json")).unwrap(),
    )
    .unwrap()
}

fn valid_envelope() -> Value {
    // Flat shape (milestone_file_for_schema merges the [milestone] meta into root).
    json!({
        "id": "01", "title": "T", "slug": "t",
        "spec_status": "draft", "execution_status": "planned",
        "depends_on": [], "effort": "S", "risk": "low",
        "intent": { "outcome": "o" },
        "problem": { "description": "p" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{ "description": "d", "verification": "v" }]
    })
}

fn check(value: Value) -> Vec<String> {
    Validator::new(&milestone_schema())
        .unwrap()
        .iter_errors(&value)
        .into_iter()
        .map(|e| e.message)
        .collect()
}

#[test]
fn valid_milestone_passes() {
    let errs = check(valid_envelope());
    assert!(errs.is_empty(), "expected valid, got: {errs:?}");
}

#[test]
fn invalid_bad_id_pattern() {
    let mut v = valid_envelope();
    v["id"] = json!("abc");
    let errs = check(v);
    assert!(errs.iter().any(|m| m.contains("pattern")), "{errs:?}");
}

#[test]
fn invalid_bad_spec_status_enum() {
    let mut v = valid_envelope();
    v["spec_status"] = json!("bogus");
    let errs = check(v);
    assert!(
        errs.iter().any(|m| m.contains("allowed values")),
        "{errs:?}"
    );
}

#[test]
fn invalid_missing_title() {
    let mut v = valid_envelope();
    v.as_object_mut().unwrap().remove("title");
    let errs = check(v);
    assert!(
        errs.iter().any(|m| m.contains("title is a required")),
        "{errs:?}"
    );
}

#[test]
fn invalid_out_of_scope_min_items() {
    let mut v = valid_envelope();
    v["scope"]["out_of_scope"] = json!(["only-one"]);
    let errs = check(v);
    assert!(
        !errs.is_empty(),
        "out_of_scope minItems should flag: {errs:?}"
    );
}
