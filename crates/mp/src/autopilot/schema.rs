//! M207: JSON-schema loader for `autopilot/session.json`.
//!
//! The schema lives at `schemas/autopilot-session.schema.json` and is
//! embedded into the binary at compile time (same machinery as
//! [`crate::schema`]). A single [`Validator`] is cached per process
//! via [`OnceLock`].
//!
//! The 32 MiB cap on session.json reads is enforced one layer up
//! ([`crate::json_input::read_file_bounded`]) — this module only owns
//! the schema and validator.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::assets;
use crate::mini_schema::Validator;

/// Filename inside the embedded `schemas/` tree.
pub const SCHEMA_FILENAME: &str = "autopilot-session.schema.json";

/// 32 MiB — single explicit cap for autopilot session.json. Same
/// value as the `MAX_JSON_INPUT_BYTES` soft cap in
/// [`crate::json_input`], but declared here so autopilot consumers do
/// not have to reach into `json_input` for the limit.
pub const SESSION_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Cached validator. Process-wide compile + first-validate cost is
/// paid once; subsequent reads pay only the iter cost.
static VALIDATOR: OnceLock<Result<Validator, String>> = OnceLock::new();

/// Return the cached session-schema validator, compiling it on first
/// use. The validator is the same shape the rest of `mp` uses; the
/// error type is a `String` here (rather than the boxed
/// [`crate::mini_schema::CompileError`]) so the `OnceLock` can hold
/// `Send + Sync + 'static`.
pub fn validator() -> Result<&'static Validator> {
    VALIDATOR
        .get_or_init(load_validator)
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))
}

/// Validate `value` against the autopilot session schema. Errors
/// carry their instance path (e.g. `/queue/0/cycle`) so a verifier can
/// point at the offending field.
pub fn validate_value(value: &Value) -> Result<Vec<String>> {
    let validator = validator()?;
    let errors = validator.iter_errors(value);
    Ok(errors
        .into_iter()
        .map(|e| format!("{}: {}", e.instance_path(), e))
        .collect())
}

/// Re-export with the public name used by the autopilot module.
pub fn validate_session_value(value: &Value) -> Result<Vec<String>> {
    validate_value(value)
}

fn load_validator() -> Result<Validator, String> {
    let rel = format!("schemas/{SCHEMA_FILENAME}");
    let raw = assets::read_embedded(&rel).map_err(|e| format!("load schema {rel}: {e}"))?;
    let schema: Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse schema {rel}: {e}"))?;
    Validator::new(&schema).map_err(|e| format!("compile schema {rel}: {e}"))
}

/// Freshen the validator cache — exposed for tests that swap the
/// embedded tree at runtime. Production code should not need this.
pub fn reset_validator_for_tests() {
    // The `OnceLock` has no reset API; the test-only override is to
    // shadow the value via a feature flag in the calling test. Kept
    // here so future test helpers can find the entry point.
    let _ = ();
}

/// Read the embedded schema as a JSON value. Useful for tests that
/// want to assert against the on-disk shape (e.g. required-field
/// drift between schema and Rust struct).
pub fn embedded_schema() -> Result<Value> {
    let rel = format!("schemas/{SCHEMA_FILENAME}");
    let raw = assets::read_embedded(&rel).with_context(|| format!("load schema {rel}"))?;
    Ok(serde_json::from_str(&raw)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validator_compiles_embedded_schema() {
        assert!(validator().is_ok(), "validator must compile");
    }

    #[test]
    fn empty_object_fails_required_fields() {
        let errs = validate_value(&json!({})).unwrap();
        // `id`, `schema_version`, `topology`, `roles`, `queue`,
        // `status`, `last_updated` are required.
        assert!(!errs.is_empty(), "expected required-field errors");
        assert!(errs.iter().any(|e| e.contains("id")));
        assert!(errs.iter().any(|e| e.contains("schema_version")));
        assert!(errs.iter().any(|e| e.contains("topology")));
        assert!(errs.iter().any(|e| e.contains("roles")));
        assert!(errs.iter().any(|e| e.contains("queue")));
        assert!(errs.iter().any(|e| e.contains("status")));
        assert!(errs.iter().any(|e| e.contains("last_updated")));
    }

    #[test]
    fn status_enum_rejects_unknown_value() {
        let errs = validate_value(&json!({
            "id": "s1",
            "schema_version": 1,
            "topology": {},
            "roles": {},
            "queue": [],
            "status": "bogus",
            "last_updated": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        assert!(
            errs.iter().any(|e| e.contains("status")),
            "expected status enum error; got {errs:?}"
        );
    }
}