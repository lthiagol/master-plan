//! A minimal JSON Schema validator covering only the subset `mp` uses.
//!
//! Replaces the `jsonschema` crate for local validation. Supported keywords:
//! `type, required, properties, pattern, enum, minLength, minItems, minimum,
//! items, $ref (internal #/$defs/), anyOf, $defs`. Unsupported keywords
//! (`oneOf, allOf, not, if/then/else, format, patternProperties,
//! additionalProperties`, ...) are rejected at compile time so a schema cannot
//! silently rely on a feature we do not implement.
//!
//! The public surface (`Validator::new`, `iter_errors`, `ValidationError`
//! `instance_path()` + `Display`) mirrors the bits of `jsonschema` that
//! `schema.rs` consumes, so it is a near drop-in swap.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde_json::Value;

const ALLOWED_KEYWORDS: &[&str] = &[
    "$defs",
    "$id",
    "$ref",
    "$schema",
    "anyOf",
    "default",
    "description",
    "enum",
    "items",
    "minItems",
    "minLength",
    "minimum",
    "pattern",
    "properties",
    "required",
    "title",
    "type",
];
const ALLOWED_TYPES: &[&str] = &[
    "object", "array", "string", "integer", "number", "boolean", "null",
];

#[derive(Debug)]
pub struct CompileError {
    pub message: String,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CompileError {}

fn compile_error(path: &str, message: impl fmt::Display) -> CompileError {
    CompileError {
        message: format!("invalid schema at {path}: {message}"),
    }
}

fn compile_schema(schema: &Value, path: &str) -> Result<CompiledSchema, CompileError> {
    let object = schema
        .as_object()
        .ok_or_else(|| compile_error(path, "schema must be an object"))?;
    for keyword in object.keys() {
        if !ALLOWED_KEYWORDS.contains(&keyword.as_str()) {
            return Err(compile_error(
                &format!("{path}/{keyword}"),
                format!("unsupported keyword {keyword:?}"),
            ));
        }
    }

    let mut compiled = CompiledSchema::default();
    if let Some(value) = object.get("$ref") {
        let reference = value
            .as_str()
            .ok_or_else(|| compile_error(&format!("{path}/$ref"), "must be a string"))?;
        let name = reference.strip_prefix("#/$defs/").ok_or_else(|| {
            compile_error(
                &format!("{path}/$ref"),
                "only internal #/$defs/<name> references are supported",
            )
        })?;
        if name.is_empty() || name.contains('/') {
            return Err(compile_error(
                &format!("{path}/$ref"),
                "must name one root $defs entry",
            ));
        }
        compiled.reference = Some(name.to_string());
    }
    if let Some(value) = object.get("type") {
        let values = match value {
            Value::String(value) => vec![value.clone()],
            Value::Array(values) if !values.is_empty() => {
                let mut types = Vec::with_capacity(values.len());
                for (index, value) in values.iter().enumerate() {
                    types.push(
                        value
                            .as_str()
                            .ok_or_else(|| {
                                compile_error(&format!("{path}/type/{index}"), "must be a string")
                            })?
                            .to_string(),
                    );
                }
                types
            }
            Value::Array(_) => {
                return Err(compile_error(
                    &format!("{path}/type"),
                    "array must not be empty",
                ));
            }
            _ => {
                return Err(compile_error(
                    &format!("{path}/type"),
                    "must be a string or non-empty string array",
                ));
            }
        };
        let mut seen = HashSet::new();
        for value in &values {
            if !ALLOWED_TYPES.contains(&value.as_str()) {
                return Err(compile_error(
                    &format!("{path}/type"),
                    format!("unknown type {value:?}"),
                ));
            }
            if !seen.insert(value) {
                return Err(compile_error(
                    &format!("{path}/type"),
                    format!("duplicate type {value:?}"),
                ));
            }
        }
        compiled.types = Some(values);
    }
    if let Some(value) = object.get("required") {
        compiled.required = Some(compile_string_array(value, path, "required")?);
    }
    if let Some(value) = object.get("enum") {
        let values = value
            .as_array()
            .filter(|values| !values.is_empty())
            .ok_or_else(|| compile_error(&format!("{path}/enum"), "must be a non-empty array"))?;
        compiled.enum_values = Some(values.clone());
    }
    if let Some(value) = object.get("pattern") {
        let source = value
            .as_str()
            .ok_or_else(|| compile_error(&format!("{path}/pattern"), "must be a string"))?;
        // JSON Schema / ECMAScript: `\d` is `[0-9]`, not Unicode Nd. Disable
        // Unicode mode so mini_schema stays in parity with the jsonschema oracle.
        compiled.pattern = Some(
            regex::RegexBuilder::new(source)
                .unicode(false)
                .build()
                .map_err(|error| {
                    compile_error(
                        &format!("{path}/pattern"),
                        format!("invalid regex: {error}"),
                    )
                })?,
        );
        compiled.pattern_source = Some(source.to_string());
    }
    compiled.min_length = compile_nonnegative_integer(object.get("minLength"), path, "minLength")?;
    compiled.min_items = compile_nonnegative_integer(object.get("minItems"), path, "minItems")?;
    if let Some(value) = object.get("minimum") {
        compiled.minimum = Some(
            value
                .as_f64()
                .ok_or_else(|| compile_error(&format!("{path}/minimum"), "must be a number"))?,
        );
    }
    if let Some(value) = object.get("properties") {
        let properties = value
            .as_object()
            .ok_or_else(|| compile_error(&format!("{path}/properties"), "must be an object"))?;
        for (name, subschema) in properties {
            compiled.properties.insert(
                name.clone(),
                compile_schema(subschema, &format!("{path}/properties/{name}"))?,
            );
        }
    }
    if let Some(value) = object.get("items") {
        compiled.items = Some(Box::new(compile_schema(value, &format!("{path}/items"))?));
    }
    if let Some(value) = object.get("anyOf") {
        let schemas = value
            .as_array()
            .filter(|schemas| !schemas.is_empty())
            .ok_or_else(|| compile_error(&format!("{path}/anyOf"), "must be a non-empty array"))?;
        for (index, subschema) in schemas.iter().enumerate() {
            compiled
                .any_of
                .push(compile_schema(subschema, &format!("{path}/anyOf/{index}"))?);
        }
    }
    if let Some(value) = object.get("$defs") {
        let definitions = value
            .as_object()
            .ok_or_else(|| compile_error(&format!("{path}/$defs"), "must be an object"))?;
        for (name, definition) in definitions {
            compile_schema(definition, &format!("{path}/$defs/{name}"))?;
        }
    }
    Ok(compiled)
}

fn compile_string_array(
    value: &Value,
    path: &str,
    keyword: &str,
) -> Result<Vec<String>, CompileError> {
    let values = value
        .as_array()
        .ok_or_else(|| compile_error(&format!("{path}/{keyword}"), "must be an array"))?;
    let mut result = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();
    for (index, value) in values.iter().enumerate() {
        let value = value
            .as_str()
            .ok_or_else(|| compile_error(&format!("{path}/{keyword}/{index}"), "must be a string"))?
            .to_string();
        if !seen.insert(value.clone()) {
            return Err(compile_error(
                &format!("{path}/{keyword}"),
                format!("duplicate value {value:?}"),
            ));
        }
        result.push(value);
    }
    Ok(result)
}

fn compile_nonnegative_integer(
    value: Option<&Value>,
    path: &str,
    keyword: &str,
) -> Result<Option<u64>, CompileError> {
    value
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                compile_error(
                    &format!("{path}/{keyword}"),
                    "must be a non-negative integer",
                )
            })
        })
        .transpose()
}

fn validate_references(
    schema: &CompiledSchema,
    definitions: &HashSet<&str>,
    path: &str,
) -> Result<(), CompileError> {
    if let Some(reference) = &schema.reference {
        if !definitions.contains(reference.as_str()) {
            return Err(compile_error(
                &format!("{path}/$ref"),
                format!("unresolved reference #/$defs/{reference}"),
            ));
        }
    }
    for (name, property) in &schema.properties {
        validate_references(property, definitions, &format!("{path}/properties/{name}"))?;
    }
    if let Some(items) = &schema.items {
        validate_references(items, definitions, &format!("{path}/items"))?;
    }
    for (index, branch) in schema.any_of.iter().enumerate() {
        validate_references(branch, definitions, &format!("{path}/anyOf/{index}"))?;
    }
    Ok(())
}

fn validate_raw_references(
    schema: &Value,
    definitions: &HashSet<&str>,
    path: &str,
) -> Result<(), CompileError> {
    let object = schema
        .as_object()
        .ok_or_else(|| compile_error(path, "schema must be an object"))?;
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        let name = reference
            .strip_prefix("#/$defs/")
            .expect("$ref shape was checked during schema compilation");
        if !definitions.contains(name) {
            return Err(compile_error(
                &format!("{path}/$ref"),
                format!("unresolved reference {reference}"),
            ));
        }
    }
    for keyword in ["properties", "$defs"] {
        if let Some(entries) = object.get(keyword).and_then(Value::as_object) {
            for (name, subschema) in entries {
                validate_raw_references(
                    subschema,
                    definitions,
                    &format!("{path}/{keyword}/{name}"),
                )?;
            }
        }
    }
    if let Some(items) = object.get("items") {
        validate_raw_references(items, definitions, &format!("{path}/items"))?;
    }
    if let Some(branches) = object.get("anyOf").and_then(Value::as_array) {
        for (index, branch) in branches.iter().enumerate() {
            validate_raw_references(branch, definitions, &format!("{path}/anyOf/{index}"))?;
        }
    }
    Ok(())
}

/// Zero-cost edges follow `$ref` / `anyOf` without descending into instance
/// structure. Pure cycles on those edges stack-overflow at validate time;
/// properties/items self-refs are data-driven and already terminate.
fn zero_cost_ref_targets<'a>(schema: &'a CompiledSchema, out: &mut Vec<&'a str>) {
    if let Some(reference) = &schema.reference {
        out.push(reference.as_str());
        return;
    }
    for branch in &schema.any_of {
        zero_cost_ref_targets(branch, out);
    }
}

fn detect_ref_cycles(
    root: &CompiledSchema,
    defs: &HashMap<String, CompiledSchema>,
) -> Result<(), CompileError> {
    let mut visiting = HashSet::new();
    let mut done = HashSet::new();

    fn walk(
        name: &str,
        defs: &HashMap<String, CompiledSchema>,
        visiting: &mut HashSet<String>,
        done: &mut HashSet<String>,
        path: &str,
    ) -> Result<(), CompileError> {
        if done.contains(name) {
            return Ok(());
        }
        if !visiting.insert(name.to_string()) {
            return Err(compile_error(
                path,
                format!("cyclic $ref involving #/$defs/{name}"),
            ));
        }
        if let Some(schema) = defs.get(name) {
            let mut targets = Vec::new();
            zero_cost_ref_targets(schema, &mut targets);
            for target in targets {
                walk(
                    target,
                    defs,
                    visiting,
                    done,
                    &format!("{path} -> #/$defs/{target}"),
                )?;
            }
        }
        visiting.remove(name);
        done.insert(name.to_string());
        Ok(())
    }

    let mut root_targets = Vec::new();
    zero_cost_ref_targets(root, &mut root_targets);
    for target in root_targets {
        walk(
            target,
            defs,
            &mut visiting,
            &mut done,
            &format!("$ -> #/$defs/{target}"),
        )?;
    }
    for name in defs.keys() {
        walk(
            name,
            defs,
            &mut visiting,
            &mut done,
            &format!("$/$defs/{name}"),
        )?;
    }
    Ok(())
}

pub struct Validator {
    root: CompiledSchema,
    defs: HashMap<String, CompiledSchema>,
}

#[derive(Default)]
struct CompiledSchema {
    reference: Option<String>,
    types: Option<Vec<String>>,
    required: Option<Vec<String>>,
    enum_values: Option<Vec<Value>>,
    pattern: Option<regex::Regex>,
    pattern_source: Option<String>,
    min_length: Option<u64>,
    min_items: Option<u64>,
    minimum: Option<f64>,
    properties: HashMap<String, CompiledSchema>,
    items: Option<Box<CompiledSchema>>,
    any_of: Vec<CompiledSchema>,
}

impl Validator {
    /// Compile a schema. Returns `CompileError` if the schema (recursively)
    /// contains an unsupported keyword.
    pub fn new(schema: &Value) -> Result<Self, CompileError> {
        let object = schema
            .as_object()
            .ok_or_else(|| compile_error("$", "schema must be an object"))?;
        let defs_value = object.get("$defs");
        let mut defs = HashMap::new();
        if let Some(value) = defs_value {
            let map = value
                .as_object()
                .ok_or_else(|| compile_error("$/$defs", "must be an object"))?;
            for (name, definition) in map {
                defs.insert(
                    name.clone(),
                    compile_schema(definition, &format!("$/$defs/{name}"))?,
                );
            }
        }
        let root = compile_schema(schema, "$")?;
        let known_defs = defs.keys().map(String::as_str).collect::<HashSet<_>>();
        validate_raw_references(schema, &known_defs, "$")?;
        validate_references(&root, &known_defs, "$")?;
        for (name, definition) in &defs {
            validate_references(definition, &known_defs, &format!("$/$defs/{name}"))?;
        }
        detect_ref_cycles(&root, &defs)?;
        Ok(Self { root, defs })
    }

    /// Validate `value` against the compiled schema, returning all errors found.
    pub fn iter_errors(&self, value: &Value) -> Vec<ValidationError> {
        let mut errs = Vec::new();
        self.validate(value, &self.root, String::new(), &mut errs);
        errs
    }

    fn validate(
        &self,
        value: &Value,
        schema: &CompiledSchema,
        path: String,
        errs: &mut Vec<ValidationError>,
    ) {
        if let Some(name) = &schema.reference {
            self.validate(value, &self.defs[name], path, errs);
            return;
        }

        if let Some(t) = &schema.types {
            check_type(value, t, &path, errs);
        }
        if let Some(req) = &schema.required {
            check_required(value, req, &path, errs);
        }
        if let Some(e) = &schema.enum_values {
            check_enum(value, e, &path, errs);
        }
        if let Some(pattern) = &schema.pattern {
            check_pattern(
                value,
                pattern,
                schema.pattern_source.as_deref().unwrap_or_default(),
                &path,
                errs,
            );
        }
        if let Some(min_len) = schema.min_length {
            check_min_length(value, min_len, &path, errs);
        }
        if let Some(min_items) = schema.min_items {
            check_min_items(value, min_items, &path, errs);
        }
        if let Some(minimum) = schema.minimum {
            check_minimum(value, minimum, &path, errs);
        }
        if !schema.properties.is_empty() {
            check_properties(self, value, &schema.properties, &path, errs);
        }
        if let Some(items) = &schema.items {
            check_items(self, value, items, &path, errs);
        }
        if !schema.any_of.is_empty() {
            check_any_of(self, value, &schema.any_of, &path, errs);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub instance_path: String,
    pub message: String,
}

impl ValidationError {
    pub fn instance_path(&self) -> &str {
        &self.instance_path
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

fn is_json_integer(value: &Value) -> bool {
    if value.as_i64().is_some() || value.as_u64().is_some() {
        return true;
    }
    // JSON Schema: mathematically integral numbers (e.g. `1.0`) are integers.
    // serde_json may store them as f64, where `as_i64` is None.
    value
        .as_f64()
        .is_some_and(|n| n.is_finite() && n.fract() == 0.0)
}

fn type_matches(value: &Value, t: &str) -> bool {
    match t {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => is_json_integer(value),
        "number" => value.as_f64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn check_type(value: &Value, types: &[String], path: &str, errs: &mut Vec<ValidationError>) {
    if !types.iter().any(|t| type_matches(value, t)) {
        errs.push(ValidationError {
            instance_path: path.to_string(),
            message: format!("{} is not of type {}", json_kind(value), types.join("/")),
        });
    }
}

fn check_required(value: &Value, reqs: &[String], path: &str, errs: &mut Vec<ValidationError>) {
    let Some(obj) = value.as_object() else {
        return;
    };
    for name in reqs {
        if !obj.contains_key(name) {
            errs.push(ValidationError {
                instance_path: path.to_string(),
                message: format!("{name} is a required property"),
            });
        }
    }
}

fn check_enum(value: &Value, allowed: &[Value], path: &str, errs: &mut Vec<ValidationError>) {
    if !allowed.contains(value) {
        errs.push(ValidationError {
            instance_path: path.to_string(),
            message: format!("{value} is not one of the allowed values"),
        });
    }
}

fn check_pattern(
    value: &Value,
    pattern: &regex::Regex,
    source: &str,
    path: &str,
    errs: &mut Vec<ValidationError>,
) {
    if let Value::String(s) = value {
        if !pattern.is_match(s) {
            errs.push(ValidationError {
                instance_path: path.to_string(),
                message: format!("{s:?} does not match pattern {source:?}"),
            });
        }
    }
}

fn check_min_length(value: &Value, min_len: u64, path: &str, errs: &mut Vec<ValidationError>) {
    if let Value::String(s) = value {
        if (s.chars().count() as u64) < min_len {
            errs.push(ValidationError {
                instance_path: path.to_string(),
                message: format!("{s:?} is shorter than {min_len}"),
            });
        }
    }
}

fn check_min_items(value: &Value, min_items: u64, path: &str, errs: &mut Vec<ValidationError>) {
    if let Value::Array(a) = value {
        if (a.len() as u64) < min_items {
            errs.push(ValidationError {
                instance_path: path.to_string(),
                message: format!("array length {} is less than {min_items}", a.len()),
            });
        }
    }
}

fn check_minimum(value: &Value, minimum: f64, path: &str, errs: &mut Vec<ValidationError>) {
    if let Some(n) = value.as_f64() {
        if n < minimum {
            errs.push(ValidationError {
                instance_path: path.to_string(),
                message: format!("{value} is less than the minimum {minimum}"),
            });
        }
    }
}

fn check_properties(
    this: &Validator,
    value: &Value,
    props: &HashMap<String, CompiledSchema>,
    path: &str,
    errs: &mut Vec<ValidationError>,
) {
    let Some(obj) = value.as_object() else {
        return;
    };
    for (key, sub) in props {
        if let Some(child) = obj.get(key) {
            let child_path = format!("{path}/{key}");
            this.validate(child, sub, child_path, errs);
        }
    }
}

fn check_items(
    this: &Validator,
    value: &Value,
    items: &CompiledSchema,
    path: &str,
    errs: &mut Vec<ValidationError>,
) {
    let Some(arr) = value.as_array() else {
        return;
    };
    for (i, el) in arr.iter().enumerate() {
        let child_path = format!("{path}/{i}");
        this.validate(el, items, child_path, errs);
    }
}

fn check_any_of(
    this: &Validator,
    value: &Value,
    any_of: &[CompiledSchema],
    path: &str,
    errs: &mut Vec<ValidationError>,
) {
    let any_pass = any_of.iter().any(|sub| {
        let mut tmp = Vec::new();
        this.validate(value, sub, path.to_string(), &mut tmp);
        tmp.is_empty()
    });
    if !any_pass {
        errs.push(ValidationError {
            instance_path: path.to_string(),
            message: "value does not match any of the allowed schemas (anyOf)".to_string(),
        });
    }
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
