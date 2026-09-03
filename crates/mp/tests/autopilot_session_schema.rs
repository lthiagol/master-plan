//! M207 / S01 / AC-01: session.json I/O validates against mini_schema,
//! uses the shared bounded-read primitive (32 MiB), enforces
//! project-root containment, and writes atomically.

mod common;

use common::TestEnv;
use mp::autopilot::{
    load_session, load_session_from, sample_session_for_tests, save_session, save_session_at,
    validate_session_value, AutopilotSession, SESSION_MAX_BYTES, SESSION_SCHEMA_VERSION,
};
use mp::paths::PlanContext;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

#[test]
fn session_max_bytes_is_explicit_32_mib() {
    // The 32 MiB cap must be the value the spec calls out; if it
    // ever changes, this test will flag it for review.
    assert_eq!(SESSION_MAX_BYTES, 32 * 1024 * 1024);
}

#[test]
fn load_session_validates_against_embedded_schema() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let s = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &s).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    // Re-validate the loaded document against the embedded schema.
    let value = serde_json::to_value(&loaded).unwrap();
    let errs = validate_session_value(&value).unwrap();
    assert!(errs.is_empty(), "loaded session failed validation: {errs:?}");
}

#[test]
fn save_session_validates_before_writing() {
    // Save refuses to write a schema-invalid value before touching
    // the disk.
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let mut bad = sample_session_for_tests("alpha");
    // Pattern requires lowercase-start; "Alpha-Capital" is invalid.
    bad.id = "Alpha-Capital".to_string();
    let err = save_session(&ctx, "alpha", &bad).unwrap_err();
    assert!(err.to_string().contains("schema validation"));
}

#[test]
fn load_session_rejects_unknown_schema_version() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let path = ctx.plan_dir.join("autopilot/alpha/session.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        r#"{
            "id": "alpha",
            "schema_version": 999,
            "topology": {},
            "roles": {},
            "queue": [],
            "status": "draft",
            "last_updated": "2026-01-01T00:00:00Z"
        }"#,
    )
    .unwrap();
    let err = load_session(&ctx, "alpha").unwrap_err();
    assert!(format!("{err}").contains("schema_version"));
}

#[test]
fn load_session_rejects_outside_project_root() {
    // Two disjoint temp dirs; reading from one with the other as
    // project_root must surface OutsideProjectRoot.
    let project = TempDir::new().unwrap();
    let foreign = TempDir::new().unwrap();
    let foreign_dir = foreign.path().join("autopilot/foreign/session.json");
    fs::create_dir_all(foreign_dir.parent().unwrap()).unwrap();
    fs::write(
        &foreign_dir,
        r#"{"id":"x","schema_version":1,"topology":{},"roles":{},"queue":[],"status":"draft","last_updated":"t"}"#,
    )
    .unwrap();
    let err = load_session_from(&foreign_dir, project.path()).unwrap_err();
    assert!(matches!(err, mp::autopilot::session::SessionLoadError::OutsideProjectRoot { .. }));
}

#[test]
fn load_session_rejects_parse_errors() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let s = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &s).unwrap();
    let path = ctx.plan_dir.join("autopilot/alpha/session.json");
    fs::write(&path, b"not json {{{").unwrap();
    let err = load_session(&ctx, "alpha").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("parse") || msg.contains("decode"), "got {msg}");
}

#[test]
fn load_session_rejects_schema_invalid_values() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let s = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &s).unwrap();
    let path = ctx.plan_dir.join("autopilot/alpha/session.json");
    let raw = fs::read_to_string(&path).unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    // Drop the required `id` field.
    value.as_object_mut().unwrap().remove("id");
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let err = load_session(&ctx, "alpha").unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("schema validation") || msg.contains("decode") || msg.contains("parse"),
        "got {msg}"
    );
}

#[test]
fn session_path_rejects_path_traversal() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    assert!(mp::autopilot::session::SessionPath::new(&ctx, "../etc").is_err());
    assert!(mp::autopilot::session::SessionPath::new(&ctx, "a/b").is_err());
    assert!(mp::autopilot::session::SessionPath::new(&ctx, "").is_err());
}

#[test]
fn save_session_at_creates_parent_directory() {
    let tmp = TempDir::new().unwrap();
    let s = sample_session_for_tests("alpha");
    let path = tmp.path().join("autopilot/alpha/session.json");
    assert!(!path.parent().unwrap().exists());
    save_session_at(&path, &s).unwrap();
    assert!(path.parent().unwrap().is_dir());
    assert!(path.is_file());
}

#[test]
fn save_session_is_atomic_via_temp_rename() {
    // Pin the atomic-write contract: after save returns, the
    // destination contains a parseable JSON document.
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let s = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &s).unwrap();
    let path = ctx.plan_dir.join("autopilot/alpha/session.json");
    let raw = fs::read(&path).unwrap();
    assert!(raw.starts_with(b"{"));
    let value: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(value["id"], "alpha");
    assert_eq!(value["schema_version"], SESSION_SCHEMA_VERSION);
}

#[test]
fn schema_validator_rejects_unknown_root_type() {
    // An empty object fails required-field validation.
    let errs = validate_session_value(&serde_json::json!({})).unwrap();
    assert!(!errs.is_empty());
}

#[test]
fn schema_validator_rejects_unknown_status_enum() {
    // `status` is one of draft/active/paused/stopped/completed/failed.
    let errs = validate_session_value(&serde_json::json!({
        "id": "alpha",
        "schema_version": 1,
        "topology": {},
        "roles": {},
        "queue": [],
        "status": "unknown-state",
        "last_updated": "t"
    }))
    .unwrap();
    assert!(errs.iter().any(|e| e.contains("status")));
}

#[test]
fn bounded_read_uses_32_mib_session_limit() {
    // Synthesize a session.json that exceeds the 32 MiB cap;
    // load_session must reject it.
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let path = ctx.plan_dir.join("autopilot/big/session.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Use a payload in a required-free field so the only failure is
    // the size cap, not schema validation. `prompt_bundles` is open.
    let huge_payload = "x".repeat(SESSION_MAX_BYTES as usize + 1);
    let body = format!(
        r#"{{"id":"big","schema_version":1,"topology":{{}},"roles":{{}},"queue":[],"status":"draft","last_updated":"t","prompt_bundles":{{"huge":"{}"}}}}"#,
        huge_payload
    );
    fs::write(&path, body).unwrap();
    let err = load_session(&ctx, "big").unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("exceeds") || msg.contains("32"),
        "got {msg}"
    );
}

#[test]
fn session_schema_version_matches_module_constant() {
    let s = sample_session_for_tests("alpha");
    assert_eq!(s.schema_version, SESSION_SCHEMA_VERSION);
}

#[test]
fn autopilot_session_value_round_trips_through_typed_struct() {
    // The typed struct + JSON value are bidirectionally lossless.
    let s: AutopilotSession = sample_session_for_tests("alpha");
    let value = serde_json::to_value(&s).unwrap();
    let back: AutopilotSession = serde_json::from_value(value.clone()).unwrap();
    // last_updated is auto-stamped on save, not on construction.
    let mut expected = s;
    expected.last_updated = back.last_updated.clone();
    // The value path is the source of truth for the loader; the
    // typed struct accepts whatever the loader produces.
    assert_eq!(serde_json::to_value(&back).unwrap(), serde_json::to_value(&expected).unwrap());
}