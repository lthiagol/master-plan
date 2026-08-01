use serde::{Deserialize, Serialize};

use crate::milestone::{InterfaceSpec, Scenario};

// ── Decisions types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecisionsFile {
    #[serde(default)]
    pub decisions: Vec<DecisionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEntry {
    pub id: String,
    pub date: String,
    pub summary: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub milestone: String,
}

// ── Archive types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArchiveMetaFile {
    #[serde(default)]
    pub entries: Vec<ArchiveEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub entity_type: String,
    pub entity_id: String,
    pub original_path: String,
    pub archived_path: String,
    pub archived_at: String,
}

// ── Brief types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefFile {
    pub brief: BriefMeta,
    #[serde(default)]
    pub topics: Vec<BriefTopic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefMeta {
    pub status: String,
    pub created: String,
    #[serde(default)]
    pub completed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefTopic {
    pub id: String,
    pub key: String,
    pub title: String,
    pub prompt: String,
    #[serde(default)]
    pub body: String,
    pub status: String,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub order: u32,
}

// ── Ideas types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IdeasFile {
    #[serde(default)]
    pub ideas: Vec<IdeaEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeaEntry {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub status: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source: String,
    pub created: String,
    #[serde(default)]
    pub promoted_to: String,
}

// ── Annotation types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnnotationFile {
    #[serde(default)]
    pub annotations: Vec<AnnotationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationItem {
    pub id: String,
    pub target: String,
    pub kind: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub author: String,
    pub status: String,
    pub created_at: String,
    #[serde(default)]
    pub resolved_at: String,
}

// ── Session types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFile {
    pub session: SessionMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    #[serde(default)]
    pub branch: String,
    pub title: String,
    pub status: String,
    pub milestone_id: String,
    #[serde(default)]
    pub milestone_file: String,
    #[serde(default)]
    pub started: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub merged: String,
    #[serde(default)]
    pub archived_at: String,
}

// ── Challenge types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeFile {
    pub challenge: ChallengeMeta,
    #[serde(default)]
    pub findings: Vec<ChallengeFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeMeta {
    pub id: String,
    #[serde(default)]
    pub milestone_id: String,
    pub scope: String,
    pub status: String,
    pub created: String,
    #[serde(default)]
    pub closed: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeFinding {
    pub id: String,
    pub severity: String,
    pub category: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub target: String,
    pub status: String,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub action_ref: String,
}

// ── Domain spec types ───────────────────────────────────────────────────────

/// Long-lived domain truth under `specs/{id}.toml` (P4 brownfield).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainSpecFile {
    pub domain: DomainMeta,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<DomainRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<Scenario>,
    #[serde(default)]
    pub interface: InterfaceSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainMeta {
    pub id: String,
    pub title: String,
    pub version: u32,
    pub updated: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainRequirement {
    pub id: String,
    pub statement: String,
    #[serde(default)]
    pub scenarios: Vec<String>,
}
