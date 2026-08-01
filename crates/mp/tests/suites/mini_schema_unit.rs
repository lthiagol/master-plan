//! mini_schema unit tests — one positive + negative per supported keyword, and
//! rejection of every unsupported keyword.

use mp::mini_schema::{ValidationError, Validator};

use serde_json::json;

fn errors(schema: serde_json::Value, value: serde_json::Value) -> Vec<ValidationError> {
    Validator::new(&schema).unwrap().iter_errors(&value)
}

fn passes(schema: serde_json::Value, value: serde_json::Value) -> bool {
    errors(schema, value).is_empty()
}

// ---- type ----
#[test]
fn type_string_passes_and_rejects() {
    let s = json!({ "type": "string" });
    assert!(passes(s.clone(), json!("hi")));
    assert!(!passes(s, json!(42)));
}

#[test]
fn type_object_array_integer_boolean() {
    assert!(passes(json!({"type":"object"}), json!({"a":1})));
    assert!(!passes(json!({"type":"object"}), json!([1])));
    assert!(passes(json!({"type":"array"}), json!([1, 2])));
    assert!(!passes(json!({"type":"array"}), json!("x")));
    assert!(passes(json!({"type":"integer"}), json!(7)));
    assert!(!passes(json!({"type":"integer"}), json!(7.5)));
    assert!(passes(json!({"type":"boolean"}), json!(true)));
    assert!(!passes(json!({"type":"boolean"}), json!("true")));
}

#[test]
fn type_array_of_strings_union() {
    let s = json!({ "type": ["string", "null"] });
    assert!(passes(s.clone(), json!("x")));
    assert!(passes(s.clone(), json!(null)));
    assert!(!passes(s, json!(5)));
}

// ---- required + properties ----
#[test]
fn required_present_and_missing() {
    let s = json!({ "type": "object", "required": ["a", "b"], "properties": {
        "a": { "type": "string" }, "b": { "type": "integer" }
    }});
    assert!(passes(s.clone(), json!({"a":"x","b":1})));
    let errs = errors(s, json!({"a":"x"}));
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.message.contains("b is a required")));
}

#[test]
fn properties_wrong_nested_type() {
    let s = json!({ "properties": { "inner": { "required": ["x"] } } });
    assert!(!passes(s, json!({"inner":{}})));
}

// ---- pattern + enum ----
#[test]
fn pattern_match_and_no_match() {
    let s = json!({ "type": "string", "pattern": "^[0-9]{2}$" });
    assert!(passes(s.clone(), json!("42")));
    assert!(!passes(s, json!("abc")));
}

#[test]
fn enum_valid_and_invalid() {
    let s = json!({ "enum": ["draft", "ready", "done"] });
    assert!(passes(s.clone(), json!("ready")));
    assert!(!passes(s.clone(), json!("bogus")));
    assert!(!passes(s, json!("DRAFT"))); // case-sensitive
}

// ---- minLength / minItems / minimum ----
#[test]
fn min_length_exact_and_below() {
    let s = json!({ "type": "string", "minLength": 2 });
    assert!(passes(s.clone(), json!("ab")));
    assert!(!passes(s, json!("a")));
}

#[test]
fn min_items_exact_and_below() {
    let s = json!({ "type": "array", "minItems": 1 });
    assert!(passes(s.clone(), json!([1])));
    assert!(!passes(s, json!([])));
}

#[test]
fn minimum_exact_and_below() {
    let s = json!({ "type": "integer", "minimum": 0 });
    assert!(passes(s.clone(), json!(0)));
    assert!(passes(s.clone(), json!(5)));
    assert!(!passes(s, json!(-1)));
}

// ---- items + $ref + anyOf ----
#[test]
fn items_all_valid_and_one_invalid() {
    let s = json!({ "type": "array", "items": { "type": "string" } });
    assert!(passes(s.clone(), json!(["a", "b"])));
    assert!(!passes(s, json!(["a", 1])));
}

#[test]
fn ref_internal_defs_resolves_and_rejects() {
    let s = json!({
        "$defs": { "pos": { "type": "integer", "minimum": 0 } },
        "$ref": "#/$defs/pos"
    });
    assert!(passes(s.clone(), json!(3)));
    assert!(!passes(s, json!(-1)));
}

#[test]
fn any_of_first_second_none() {
    let s = json!({ "anyOf": [{ "type": "string" }, { "type": "integer" }] });
    assert!(passes(s.clone(), json!("x")));
    assert!(passes(s.clone(), json!(1)));
    assert!(!passes(s, json!(true)));
}

// ---- unsupported keyword rejection (AC-05) ----
#[test]
fn unsupported_keywords_are_rejected_at_compile() {
    let cases = [
        "oneOf",
        "allOf",
        "not",
        "if",
        "then",
        "else",
        "format",
        "patternProperties",
        "additionalProperties",
    ];
    for kw in cases {
        let s = json!({ "type": "object", kw: true });
        let err = Validator::new(&s)
            .err()
            .unwrap_or_else(|| panic!("expected {kw} to be rejected"));
        assert!(
            err.message.contains("unsupported keyword"),
            "msg: {}",
            err.message
        );
    }
}

#[test]
fn unknown_keywords_fail_closed_but_property_and_definition_names_do_not() {
    for schema in [
        json!({"typoe": "string"}),
        json!({"properties": {"name": {"minLenght": 1}}}),
        json!({"$defs": {"entry": {"requried": ["id"]}}}),
    ] {
        let error = Validator::new(&schema).err().expect("schema must fail");
        assert!(error.message.contains("unsupported keyword"), "{error}");
        assert!(error.message.contains('$'), "{error}");
    }
    assert!(Validator::new(&json!({
        "properties": {"type": {"type": "string"}},
        "$defs": {"required": {"type": "integer"}}
    }))
    .is_ok());
}

#[test]
fn malformed_types_patterns_keyword_values_and_references_fail_at_compile() {
    let cases = [
        json!({"type": "strng"}),
        json!({"type": ["string", 7]}),
        json!({"type": []}),
        json!({"pattern": "["}),
        json!({"pattern": 1}),
        json!({"required": "id"}),
        json!({"required": ["id", 2]}),
        json!({"enum": "draft"}),
        json!({"enum": []}),
        json!({"minLength": -1}),
        json!({"minItems": "1"}),
        json!({"minimum": "zero"}),
        json!({"items": []}),
        json!({"anyOf": {}}),
        json!({"anyOf": []}),
        json!({"properties": []}),
        json!({"$defs": []}),
        json!({"$ref": "#/$defs/missing"}),
        json!({"properties": {"value": {
            "$defs": {"nested": {"$ref": "#/$defs/missing"}}
        }}}),
        json!({"$ref": "https://example.invalid/schema"}),
        json!({"$ref": 7}),
    ];
    for schema in cases {
        let error = Validator::new(&schema)
            .err()
            .unwrap_or_else(|| panic!("schema should fail closed: {schema}"));
        assert!(error.message.contains("invalid schema at $"), "{error}");
    }
}

#[test]
fn supported_schema_compiles_cleanly() {
    let s = json!({
        "$defs": { "scn": { "required": ["given", "when", "then"] } },
        "type": "object",
        "required": ["title"],
        "properties": {
            "title": { "type": "string", "minLength": 1 },
            "id": { "type": "string", "pattern": "^[0-9]{2}$" },
            "tags": { "type": "array", "items": { "type": "string" }, "minItems": 0 },
            "scenario": { "$ref": "#/$defs/scn" }
        }
    });
    assert!(Validator::new(&s).is_ok());
}

#[test]
fn cyclic_refs_fail_at_compile_including_indirect_cycles() {
    let direct = json!({
        "$defs": {
            "a": { "$ref": "#/$defs/b" },
            "b": { "$ref": "#/$defs/a" }
        },
        "$ref": "#/$defs/a"
    });
    let err = Validator::new(&direct)
        .err()
        .expect("direct cycle must fail");
    assert!(err.message.contains("cyclic $ref"), "{err}");

    let indirect = json!({
        "$defs": {
            "a": { "$ref": "#/$defs/b" },
            "b": { "$ref": "#/$defs/c" },
            "c": { "$ref": "#/$defs/a" }
        },
        "type": "object"
    });
    let err = Validator::new(&indirect)
        .err()
        .expect("indirect cycle must fail");
    assert!(err.message.contains("cyclic $ref"), "{err}");

    let any_of_cycle = json!({
        "$defs": {
            "a": { "anyOf": [{ "$ref": "#/$defs/b" }] },
            "b": { "$ref": "#/$defs/a" }
        },
        "$ref": "#/$defs/a"
    });
    let err = Validator::new(&any_of_cycle)
        .err()
        .expect("anyOf zero-cost cycle must fail");
    assert!(err.message.contains("cyclic $ref"), "{err}");
}

#[test]
fn data_driven_self_ref_via_properties_still_compiles() {
    let s = json!({
        "$defs": {
            "node": {
                "type": "object",
                "properties": {
                    "child": { "$ref": "#/$defs/node" }
                }
            }
        },
        "$ref": "#/$defs/node"
    });
    let v = Validator::new(&s).expect("properties self-ref must compile");
    assert!(v.iter_errors(&json!({"child": {"child": {}}})).is_empty());
}

#[test]
fn pattern_digit_class_is_ascii_not_unicode() {
    let s = json!({ "type": "string", "pattern": "^\\d$" });
    assert!(passes(s.clone(), json!("5")));
    assert!(!passes(s, json!("٥"))); // U+0665 Arabic-Indic digit
}

#[test]
fn integer_type_accepts_mathematically_integral_floats() {
    let s = json!({ "type": "integer" });
    assert!(passes(s.clone(), json!(1)));
    assert!(passes(s.clone(), json!(1.0)));
    assert!(!passes(s, json!(1.5)));
}
