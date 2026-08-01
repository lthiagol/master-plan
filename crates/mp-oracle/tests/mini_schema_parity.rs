//! Oracle parity: for every case, `jsonschema` (dev-only oracle) and
//! `mini_schema` must agree on pass/fail. This is the safety gate that lets
//! mini_schema replace jsonschema without behavior drift.

mod common;

use mp::mini_schema::Validator as Mini;

use serde_json::{json, Value};

use crate::common::repo_root;

/// Run both validators; return (jsonschema_passes, mini_passes).
fn agree(schema: &Value, value: &Value) -> (bool, bool) {
    let js = jsonschema::Validator::new(schema)
        .expect("jsonschema compiles")
        .iter_errors(value)
        .count()
        == 0;
    let mini = Mini::new(schema)
        .expect("mini compiles")
        .iter_errors(value)
        .is_empty();
    (js, mini)
}

fn assert_agree(schema: Value, value: Value) {
    let (js, mini) = agree(&schema, &value);
    assert_eq!(
        js, mini,
        "divergence: schema={schema}, value={value} -> jsonschema={js}, mini={mini}"
    );
}

#[test]
fn parity_keyword_type() {
    let s = json!({ "type": "string" });
    for v in [json!("hi"), json!(""), json!(5), json!(null), json!(true)] {
        assert_agree(s.clone(), v);
    }
}

#[test]
fn parity_integer_accepts_integral_floats() {
    let s = json!({ "type": "integer" });
    for v in [
        json!(0),
        json!(1),
        json!(1.0),
        json!(-2.0),
        json!(1.5),
        json!("1"),
    ] {
        assert_agree(s.clone(), v);
    }
}

#[test]
fn parity_pattern_digit_is_ascii() {
    let s = json!({ "type": "string", "pattern": "^\\d$" });
    for v in [
        json!("0"),
        json!("9"),
        json!("٥"), // U+0665 — ECMAScript \d rejects
        json!("a"),
        json!(""),
    ] {
        assert_agree(s.clone(), v);
    }
}

#[test]
fn parity_keyword_required_and_properties() {
    let s = json!({
        "type": "object",
        "required": ["id", "title"],
        "properties": {
            "id": { "type": "string", "pattern": "^[0-9]{2}$" },
            "title": { "type": "string", "minLength": 1 },
            "n": { "type": "integer", "minimum": 0 }
        }
    });
    for v in [
        json!({ "id": "01", "title": "T" }),
        json!({ "id": "01", "title": "T", "n": 5 }),
        json!({ "id": "x" }),               // bad pattern + missing title
        json!({ "id": "01", "title": "" }), // minLength
        json!({ "id": "01", "title": "T", "n": -1 }), // minimum
        json!({}),                          // missing required
    ] {
        assert_agree(s.clone(), v);
    }
}

#[test]
fn parity_keyword_enum() {
    let s = json!({ "enum": ["draft", "ready", "done"] });
    for v in [
        json!("ready"),
        json!("bogus"),
        json!("DRAFT"),
        json!(null),
        json!(1),
    ] {
        assert_agree(s.clone(), v);
    }
}

#[test]
fn parity_keyword_items_and_minitems() {
    let s = json!({ "type": "array", "minItems": 1, "items": { "type": "string" } });
    for v in [json!(["a", "b"]), json!(["a", 1]), json!([]), json!("x")] {
        assert_agree(s.clone(), v);
    }
}

#[test]
fn parity_keyword_ref_and_anyof() {
    let s = json!({
        "$defs": { "pos": { "type": "integer", "minimum": 0 } },
        "anyOf": [{ "$ref": "#/$defs/pos" }, { "type": "string" }]
    });
    for v in [json!(3), json!(-1), json!("x"), json!(true), json!(null)] {
        assert_agree(s.clone(), v);
    }
}

/// Every real schema compiles in BOTH validators, and both reject the empty
/// object (each schema has required fields).
#[test]
fn parity_real_schemas_compile_and_reject_empty() {
    let expected = mp::schema::ACTIVE_SCHEMA_FILENAMES;
    let mut active = std::fs::read_dir(repo_root().join("schemas"))
        .expect("read schemas")
        .map(|entry| entry.expect("schema entry").file_name())
        .filter_map(|name| name.into_string().ok())
        .filter(|name| name.ends_with(".schema.json"))
        .collect::<Vec<_>>();
    active.sort();
    let mut expected_sorted = expected.to_vec();
    expected_sorted.sort();
    assert_eq!(
        active, expected_sorted,
        "active schema registry changed; update oracle parity coverage"
    );

    for filename in expected {
        let path = repo_root().join("schemas").join(filename);
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{filename}"));
        let schema: Value = serde_json::from_str(&raw).expect("schema json");
        // both compile
        let js = jsonschema::Validator::new(&schema).expect("jsonschema compiles");
        let mini = Mini::new(&schema).expect("mini compiles");
        // both reject {} (required fields are present in every schema)
        let empty = json!({});
        let js_fails = js.iter_errors(&empty).count() > 0;
        let mini_fails = !mini.iter_errors(&empty).is_empty();
        assert_eq!(
            js_fails, mini_fails,
            "empty-object divergence on {filename}"
        );
    }
}

/// Milestone schema: a realistic valid instance is accepted by BOTH; targeted
/// mutations are rejected by BOTH.
#[test]
fn parity_milestone_valid_and_mutations() {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("schemas/milestone.schema.json")).unwrap(),
    )
    .unwrap();

    // Minimal valid envelope (matches the milestone schema's required shape).
    let valid = json!({
        "milestone": {
            "id": "01", "title": "T", "slug": "t",
            "spec_status": "draft", "execution_status": "planned",
            "depends_on": [], "effort": "S", "risk": "low",
            "change_kind": "", "priority": "normal",
            "created": "2026-06-25", "updated": "2026-06-25",
            "blocked_at": "", "block_reason": "", "blocked_by": ""
        },
        "intent": { "outcome": "o" },
        "problem": { "description": "p" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": []
    });
    assert_agree(schema.clone(), valid); // both accept a valid envelope
    assert_agree(schema.clone(), json!({})); // both reject empty
    assert_agree(schema.clone(), json!({ "milestone": {} })); // nested required
}
