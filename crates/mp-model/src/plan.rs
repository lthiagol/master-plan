use serde::{Deserialize, Serialize};

// ── Plan-level types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanFile {
    #[serde(default)]
    pub project: ProjectMeta,
    #[serde(default)]
    pub charter: Charter,
    #[serde(default)]
    pub metrics: Metrics,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub milestones: Vec<MilestoneIndexEntry>,
    #[serde(default)]
    pub releases: Vec<ReleaseEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReleaseEntry {
    pub version: String,
    pub status: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub milestones: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MilestoneIndexEntry {
    pub id: String,
    pub title: String,
    pub spec_status: String,
    pub execution_status: String,
    #[serde(default)]
    pub blocked_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_interleave")]
    pub interleave: String,
    #[serde(default = "default_execution_mode")]
    pub mode: String,
    #[serde(default)]
    pub handoff_at: String,
    #[serde(default)]
    pub handoff_by: String,
    #[serde(default)]
    pub focus_milestone: String,
    #[serde(default)]
    pub focus_through_step: String,
    #[serde(default)]
    pub adoption_order: Vec<AdoptionOrder>,
    /// Milestone ids that changed since the previous handoff (recorded on handoff).
    #[serde(default)]
    pub handoff_changed_milestones: Vec<String>,
    /// Semantic snapshot captured at the last handoff for plan diff.
    #[serde(default)]
    pub handoff_baseline: HandoffBaseline,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HandoffBaseline {
    #[serde(default)]
    pub planning_status: String,
    #[serde(default)]
    pub execution_mode: String,
    #[serde(default)]
    pub milestone_index: Vec<IndexSnapshot>,
    #[serde(default)]
    pub milestones: Vec<MilestoneSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct IndexSnapshot {
    pub id: String,
    pub title: String,
    pub spec_status: String,
    pub execution_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MilestoneSnapshot {
    pub id: String,
    pub title: String,
    pub spec_status: String,
    pub execution_status: String,
    pub updated: String,
    #[serde(default)]
    pub steps: Vec<StepStatusSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct StepStatusSnapshot {
    pub id: String,
    pub status: String,
}

fn default_strategy() -> String {
    "resume_then_ready".to_string()
}

fn default_interleave() -> String {
    "milestone".to_string()
}

fn default_execution_mode() -> String {
    "planning".to_string()
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            strategy: default_strategy(),
            interleave: default_interleave(),
            mode: default_execution_mode(),
            handoff_at: String::new(),
            handoff_by: String::new(),
            focus_milestone: String::new(),
            focus_through_step: String::new(),
            adoption_order: vec![],
            handoff_changed_milestones: vec![],
            handoff_baseline: HandoffBaseline::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdoptionOrder {
    pub milestone: String,
    #[serde(default)]
    pub before: String,
    #[serde(default)]
    pub rank: u32,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub stack: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub created: String,
    pub target_version: String,
    pub planning_status: String,
    #[serde(default)]
    pub planning_phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Charter {
    #[serde(default)]
    pub goals: Vec<String>,
    #[serde(default)]
    pub non_goals: Vec<String>,
    #[serde(default)]
    pub deferred: Vec<String>,
    #[serde(default)]
    pub principles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metrics {
    pub lines_of_code: u64,
    pub unit_tests: u64,
    pub integration_tests: u64,
    pub coverage_percent: f64,
    pub checked_at: String,
}
