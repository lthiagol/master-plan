use std::sync::OnceLock;

use crate::mini_schema::Validator;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;

use crate::assets;
use crate::config::ProjectConfig;
use crate::milestone::CreateMilestoneInput;
use crate::model::{AnnotationFile, BriefFile, ChallengeFile, IdeasFile, MilestoneFile, TrackFile};

/// Canonical set of schemas shipped and checked by validation/oracle coverage.
pub const ACTIVE_SCHEMA_FILENAMES: &[&str] = &[
    "annotation.schema.json",
    "brief.schema.json",
    "challenge.schema.json",
    "idea.schema.json",
    "milestone.schema.json",
    "plan.schema.json",
    "spec-domain.schema.json",
    "track.schema.json",
];

#[derive(Debug, Clone, Copy)]
pub enum SchemaKind {
    Milestone,
    Brief,
    Track,
    Idea,
    Challenge,
    Annotation,
}

impl SchemaKind {
    pub const fn filename(self) -> &'static str {
        match self {
            SchemaKind::Milestone => "milestone.schema.json",
            SchemaKind::Brief => "brief.schema.json",
            SchemaKind::Track => "track.schema.json",
            SchemaKind::Idea => "idea.schema.json",
            SchemaKind::Challenge => "challenge.schema.json",
            SchemaKind::Annotation => "annotation.schema.json",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SchemaIssue {
    pub code: String,
    pub message: String,
}

fn validator(kind: SchemaKind) -> Result<&'static Validator> {
    static MILESTONE: OnceLock<Result<Validator, String>> = OnceLock::new();
    static BRIEF: OnceLock<Result<Validator, String>> = OnceLock::new();
    static TRACK: OnceLock<Result<Validator, String>> = OnceLock::new();
    static IDEA: OnceLock<Result<Validator, String>> = OnceLock::new();
    static CHALLENGE: OnceLock<Result<Validator, String>> = OnceLock::new();
    static ANNOTATION: OnceLock<Result<Validator, String>> = OnceLock::new();

    let slot = match kind {
        SchemaKind::Milestone => &MILESTONE,
        SchemaKind::Brief => &BRIEF,
        SchemaKind::Track => &TRACK,
        SchemaKind::Idea => &IDEA,
        SchemaKind::Challenge => &CHALLENGE,
        SchemaKind::Annotation => &ANNOTATION,
    };

    slot.get_or_init(|| load_validator(kind))
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))
}

fn load_validator(kind: SchemaKind) -> Result<Validator, String> {
    let rel = format!("schemas/{}", kind.filename());
    let raw = assets::read_embedded(&rel).map_err(|e| format!("load schema {rel}: {e}"))?;
    let schema: Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse schema {rel}: {e}"))?;
    Validator::new(&schema).map_err(|e| format!("compile schema {rel}: {e}"))
}

pub fn validate_value(kind: SchemaKind, value: &Value) -> Result<Vec<SchemaIssue>> {
    let validator = validator(kind)?;
    let mut issues = Vec::new();
    for err in validator.iter_errors(value) {
        let path = err.instance_path().to_string();
        let field_hint = if path.is_empty() || path == "/" {
            String::new()
        } else {
            // Convert JSON path to TOML-like path (e.g. /scope/out_of_scope → scope.out_of_scope)
            let toml_path = path.trim_start_matches('/').replace('/', ".");
            format!(" (field: {toml_path})")
        };
        issues.push(SchemaIssue {
            code: "SCH-01".to_string(),
            message: format!("{}{field_hint}", err),
        });
    }
    Ok(issues)
}

pub fn validate_serializable<T: Serialize>(
    kind: SchemaKind,
    value: &T,
) -> Result<Vec<SchemaIssue>> {
    let json = serde_json::to_value(value).context("serialize for schema validation")?;
    validate_value(kind, &json)
}

pub fn enforce_schema(cfg: &ProjectConfig, kind: SchemaKind, value: &Value) -> Result<()> {
    let issues = validate_value(kind, value)?;
    if issues.is_empty() {
        return Ok(());
    }
    let msg = format_schema_errors(&issues);
    if cfg.strictness() == "full" {
        bail!("schema validation failed ({:?}): {msg}", kind.filename());
    }
    eprintln!("mp: schema warning ({:?}): {msg}", kind.filename());
    Ok(())
}

pub fn enforce_serializable<T: Serialize>(
    cfg: &ProjectConfig,
    kind: SchemaKind,
    value: &T,
) -> Result<()> {
    let json = serde_json::to_value(value).context("serialize for schema validation")?;
    enforce_schema(cfg, kind, &json)
}

pub fn validate_milestone_create_input(input: &CreateMilestoneInput) -> Result<Vec<SchemaIssue>> {
    validate_serializable(SchemaKind::Milestone, input)
}

pub fn enforce_milestone_create(cfg: &ProjectConfig, input: &CreateMilestoneInput) -> Result<()> {
    enforce_serializable(cfg, SchemaKind::Milestone, input)
}

pub fn enforce_milestone_file(cfg: &ProjectConfig, m: &MilestoneFile) -> Result<()> {
    enforce_serializable(cfg, SchemaKind::Milestone, &milestone_file_for_schema(m))
}

pub fn enforce_brief(cfg: &ProjectConfig, brief: &BriefFile) -> Result<()> {
    enforce_serializable(cfg, SchemaKind::Brief, brief)
}

pub fn enforce_ideas(cfg: &ProjectConfig, ideas: &IdeasFile) -> Result<()> {
    for idea in &ideas.ideas {
        if idea.title.is_empty() {
            continue;
        }
        let mut json = serde_json::to_value(idea).context("serialize idea")?;
        if let Some(obj) = json.as_object_mut() {
            if obj.get("id").and_then(|v| v.as_str()) == Some("") {
                obj.remove("id");
            }
            strip_empty_strings(obj);
        }
        enforce_schema(cfg, SchemaKind::Idea, &json)?;
    }
    Ok(())
}

pub fn enforce_track(cfg: &ProjectConfig, track: &TrackFile) -> Result<()> {
    for item in &track.items {
        if item.status == "archived" {
            continue;
        }
        if item.done_when.is_empty() && item.verification.is_empty() {
            continue;
        }
        let mut json = serde_json::to_value(item).context("serialize track item")?;
        if let Some(obj) = json.as_object_mut() {
            if obj
                .get("steps")
                .and_then(|v| v.as_array())
                .is_some_and(|a| a.is_empty())
            {
                obj.remove("steps");
            }
            strip_empty_strings(obj);
        }
        enforce_schema(cfg, SchemaKind::Track, &json)?;
    }
    Ok(())
}

pub fn enforce_annotations(cfg: &ProjectConfig, annotations: &AnnotationFile) -> Result<()> {
    for a in &annotations.annotations {
        let mut json = serde_json::to_value(a).context("serialize annotation")?;
        if let Some(obj) = json.as_object_mut() {
            if obj.get("id").and_then(|v| v.as_str()) == Some("") {
                obj.remove("id");
            }
            strip_empty_strings(obj);
        }
        enforce_schema(cfg, SchemaKind::Annotation, &json)?;
    }
    Ok(())
}

pub fn enforce_challenge(cfg: &ProjectConfig, challenge: &ChallengeFile) -> Result<()> {
    enforce_serializable(cfg, SchemaKind::Challenge, challenge)
}

fn milestone_file_for_schema(m: &MilestoneFile) -> Value {
    let mut doc = serde_json::to_value(m).unwrap_or(Value::Null);
    if let Some(obj) = doc.as_object_mut() {
        if let Some(meta) = obj.remove("milestone") {
            if let Some(meta_obj) = meta.as_object() {
                for (k, v) in meta_obj {
                    obj.entry(k.clone()).or_insert(v.clone());
                }
            }
        }
        normalize_milestone_doc(obj);
        if m.milestone.change_kind != "delta" {
            obj.remove("delta");
        }
        obj.remove("work_packages");
        obj.remove("steps");
    }
    doc
}

fn normalize_milestone_doc(obj: &mut serde_json::Map<String, Value>) {
    if obj
        .get("change_kind")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.is_empty())
    {
        obj.remove("change_kind");
    }
    if obj
        .get("risk")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s == "medium")
    {
        obj.insert("risk".to_string(), Value::String("med".to_string()));
    }
    if obj.get("priority").and_then(|v| v.as_str()) == Some("normal") {
        obj.remove("priority");
    }
    strip_empty_strings(obj);
    if let Some(acs) = obj
        .get_mut("acceptance_criteria")
        .and_then(|v| v.as_array_mut())
    {
        if acs.is_empty() {
            obj.remove("acceptance_criteria");
        } else {
            for ac in acs {
                if let Some(ac_obj) = ac.as_object_mut() {
                    if ac_obj.get("status").and_then(|v| v.as_str()) == Some("") {
                        ac_obj.remove("status");
                    }
                    if ac_obj.get("evidence").and_then(|v| v.as_str()) == Some("") {
                        ac_obj.remove("evidence");
                    }
                }
            }
        }
    }
}

fn strip_empty_strings(obj: &mut serde_json::Map<String, Value>) {
    let keys: Vec<String> = obj
        .keys()
        .filter(|k| obj.get(*k).and_then(|v| v.as_str()) == Some(""))
        .cloned()
        .collect();
    for key in keys {
        obj.remove(&key);
    }
    if obj.get("slug").and_then(|v| v.as_str()) == Some("") {
        obj.remove("slug");
    }
}

fn format_schema_errors(issues: &[SchemaIssue]) -> String {
    let body = issues
        .iter()
        .map(|i| format!("{}: {}", i.code, i.message))
        .collect::<Vec<_>>()
        .join("; ");
    format!("{body}. Use --format json to see full details or check the schema in schemas/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_milestone_schema_from_repo() {
        let issues = validate_value(
            SchemaKind::Milestone,
            &serde_json::json!({
                "title": "Test milestone",
                "intent": { "outcome": "Something verifiable" }
            }),
        )
        .expect("validator");
        assert!(issues.is_empty());
    }

    #[test]
    fn rejects_milestone_without_title() {
        let issues =
            validate_value(SchemaKind::Milestone, &serde_json::json!({})).expect("validator");
        assert!(!issues.is_empty());
    }
}
