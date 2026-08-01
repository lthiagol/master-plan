use serde::{Deserialize, Serialize};

// ── Track types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackFile {
    pub track: TrackMeta,
    #[serde(default)]
    pub items: Vec<TrackItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMeta {
    pub kind: String,
    pub title: String,
    pub perpetual: bool,
    pub scope: String,
    pub created: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub effort: String,
    pub problem: String,
    pub done_when: String,
    pub verification: String,
    #[serde(default)]
    pub steps: Vec<String>,
    pub evidence: String,
    pub created: String,
    pub completed: String,
    pub archived_at: String,
}

// ── Backlog types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BacklogFile {
    #[serde(default)]
    pub items: Vec<BacklogItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BacklogItem {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub suggested_when: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub resolved_at: String,
}
