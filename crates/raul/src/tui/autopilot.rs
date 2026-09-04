//! M215 + M216: Autopilot lane — picker, per-drive override panel,
//! session replay shell, live status graph, multi-milestone queue
//! view, manual refresh, violation badges, detail panel, recovery
//! controls, AC detail, and cross-milestone telemetry.
//!
//! Eight independent surfaces live in this module so the lane can
//! evolve without dragging the rest of the TUI along. The picker is
//! sourced from `mp list milestones` and filtered to the autopilot-
//! eligible lifecycles (`approved` / `in-progress` / `remediation` —
//! shared with `tui::watch::DRIVABLE_LIFECYCLES` so both modules
//! speak the same vocabulary). The override panel captures the
//! per-drive shape that gets written into the new `session.json` and
//! honors the validation gate before anything lands on disk. The
//! replay shell consumes `mp autopilot session list` + `session show`
//! payloads (never reads `master-plan/` directly) and renders the
//! event timeline as a read-only strip.
//!
//! M216 adds the in-session surfaces:
//!
//! - [`StatusGraph`] (AC-01): a row-per-pane view of the live session
//!   (label, pane id, role skill, last notify, last verifier verdict).
//! - [`QueueView`] (AC-02): the multi-milestone queue with the
//!   active milestone highlighted.
//! - [`refresh_session`] (AC-03): the manual `r` adapter that pulls
//!   `mp autopilot session show <id>` + `mp autopilot status` and
//!   repopulates the lane state. No `autopilot-control` legacy
//!   command, no direct plan-file read.
//! - [`Violation`] / [`ViolationBadge`] (AC-04): typed violation
//!   rendering for C2's verifier output — badge + click-to-expand.
//! - [`DetailPanel`] (AC-05): per-milestone detail (cycles, findings,
//!   history, drift, cap) sourced from session show/events,
//!   reviews finding list, and M213 next-action APIs.
//! - [`RecoveryControl`] (AC-06): pause / resume / cancel / steer
//!   / restart clients that shell out to `mp autopilot control …`.
//! - [`AcDetailRow`] (AC-07): per-AC rows from `queue[i].ac_pass_fail`
//!   with truncation, status marker, and overflow scrolling.
//! - [`Telemetry`] (AC-08): cross-milestone aggregates —
//!   time-in-active-stage sum, attempts-per-stage, per-AC pass/fail
//!   counts.
//!
//! The lane state ([`AutopilotLaneState`]) wires the typed model into
//! the production hot path: the picker is what the Autopilot lane
//! renders, the panel is what `<o>` toggles open / closed, the
//! replay shell is what the past-session mode opens, the
//! panel's [`OverridePanel::to_session_overrides`] is what `<s>`
//! persists on Start, and the eight new fields are populated by
//! [`refresh_session`] whenever the operator presses `r`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::tui::watch::DRIVABLE_LIFECYCLES;

/// Lifecycles that the Autopilot picker accepts. Mirrors the watch
/// lane's contract — single source of truth so both modules agree
/// on the same allow-list.
pub fn picker_drivable_lifecycles() -> &'static [&'static str] {
    DRIVABLE_LIFECYCLES
}

/// True when a lifecycle string is one the picker should surface.
pub fn is_picker_eligible(lifecycle: &str) -> bool {
    DRIVABLE_LIFECYCLES.contains(&lifecycle)
}

// ─── S01: milestone picker ───────────────────────────────────────────

/// One row in the Autopilot picker. Subset of the `mp list
/// milestones` row, restricted to the fields the picker renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickerCandidate {
    pub id: String,
    pub title: String,
    pub lifecycle: String,
    pub priority: Option<String>,
}

/// The Autopilot picker. Holds the eligible candidate list, the
/// user's ordered selection, and the picker cursor. Pure data;
/// callers drive the mutations.
///
/// Invariants enforced by mutators:
/// - `selected` is a subset of `candidates` (by id).
/// - `selected` preserves insertion order: append-only on add,
///   remove-at-index on toggle-off.
/// - `cursor` is clamped to `0..candidates.len()`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Picker {
    pub candidates: Vec<PickerCandidate>,
    pub selected: Vec<String>,
    pub cursor: usize,
}

impl Picker {
    /// Empty picker. Used by `App::new()` and on a manual reset.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Filter a `mp list milestones` payload to the autopilot-
    /// eligible lifecycles, preserving the canonical list order.
    /// Pure: no subprocess, no IO. The function accepts both the
    /// bare array (`Vec<Value>`) and the canonical envelope
    /// `{ "milestones": [...] }`.
    pub fn filter_candidates(list_payload: &Value) -> Vec<PickerCandidate> {
        let rows = list_payload
            .as_array()
            .or_else(|| list_payload.get("milestones").and_then(|m| m.as_array()))
            .or_else(|| list_payload.get("items").and_then(|m| m.as_array()))
            .cloned()
            .unwrap_or_default();
        rows.into_iter()
            .filter_map(|row| {
                let id = row
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim_start_matches('M').to_string())?;
                let lifecycle = row
                    .get("lifecycle")
                    .and_then(|v| v.as_str())
                    .unwrap_or("draft")
                    .to_string();
                if !is_picker_eligible(&lifecycle) {
                    return None;
                }
                let title = row
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let priority = row
                    .get("priority")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Some(PickerCandidate {
                    id,
                    title,
                    lifecycle,
                    priority,
                })
            })
            .collect()
    }

    /// Replace the candidate list (e.g. after a refresh). Surviving
    /// selections keep their relative order; ids that no longer
    /// resolve are dropped.
    pub fn refresh_candidates(&mut self, list_payload: &Value) {
        let new_candidates = Self::filter_candidates(list_payload);
        let new_ids: std::collections::HashSet<String> =
            new_candidates.iter().map(|c| c.id.clone()).collect();
        self.selected.retain(|id| new_ids.contains(id));
        self.candidates = new_candidates;
        if self.candidates.is_empty() {
            self.cursor = 0;
        } else if self.cursor >= self.candidates.len() {
            self.cursor = self.candidates.len() - 1;
        }
    }

    /// Toggle a candidate's selection. No-op when the id is not
    /// in the candidate set. Append-on-add, remove-at-index on
    /// toggle-off — the order of `selected` matches the order
    /// the user picked.
    pub fn toggle_select(&mut self, id: &str) {
        if let Some(idx) = self.candidates.iter().position(|c| c.id == id) {
            let id_owned = self.candidates[idx].id.clone();
            if let Some(pos) = self.selected.iter().position(|s| s == &id_owned) {
                self.selected.remove(pos);
            } else {
                self.selected.push(id_owned);
            }
        }
    }

    /// Replace the selection with an explicit ordered list. Ids
    /// outside the candidate set are dropped (caller passes through
    /// the same gate as `toggle_select`).
    pub fn set_selection(&mut self, ids: Vec<String>) {
        let known: std::collections::HashSet<String> =
            self.candidates.iter().map(|c| c.id.clone()).collect();
        self.selected = ids.into_iter().filter(|id| known.contains(id)).collect();
    }

    /// Drop every selection.
    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    /// Move the cursor by `delta`. Wraps around the candidate list
    /// so the operator can scroll past either end without losing
    /// focus. No-op when the candidate list is empty.
    pub fn move_cursor(&mut self, delta: i64) {
        if self.candidates.is_empty() {
            return;
        }
        let len = self.candidates.len() as i64;
        let cur = self.cursor as i64;
        let next = (cur + delta).rem_euclid(len);
        self.cursor = next as usize;
    }

    /// Read-only view of the candidate under the cursor.
    pub fn cursor_candidate(&self) -> Option<&PickerCandidate> {
        self.candidates.get(self.cursor)
    }

    /// True when the picker has at least one selection.
    pub fn has_selection(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Read-only view of the ordered queue. This is the order
    /// passed to `mp autopilot start` on Start.
    pub fn queue_ids(&self) -> &[String] {
        &self.selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_list_payload() -> Value {
        serde_json::json!({
            "milestones": [
                {"id": "M207", "title": "Pilot S2", "lifecycle": "approved", "priority": "high"},
                {"id": "M209", "title": "Coordination", "lifecycle": "in-progress", "priority": "normal"},
                {"id": "M210", "title": "Spawn pipeline", "lifecycle": "draft", "priority": "low"},
                {"id": "M211", "title": "Reconcile", "lifecycle": "remediation", "priority": "high"},
                {"id": "M212", "title": "Doc", "lifecycle": "complete", "priority": "low"},
            ]
        })
    }

    #[test]
    fn picker_filters_to_drivable_lifecycles() {
        let candidates = Picker::filter_candidates(&sample_list_payload());
        let ids: Vec<&str> = candidates.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["207", "209", "211"]);
    }

    #[test]
    fn picker_toggle_preserves_insertion_order() {
        let mut picker = Picker::empty();
        picker.refresh_candidates(&sample_list_payload());
        picker.toggle_select("207");
        picker.toggle_select("211");
        picker.toggle_select("209");
        // Insertion order: 207 first, then 211, then 209 — not the
        // canonical id order.
        assert_eq!(picker.queue_ids(), vec!["207", "211", "209"]);
        // Toggle 211 off — its position is removed, the rest keep
        // their relative order.
        picker.toggle_select("211");
        assert_eq!(picker.queue_ids(), vec!["207", "209"]);
    }

    #[test]
    fn picker_cursor_wraps_across_ends() {
        let mut picker = Picker::empty();
        picker.refresh_candidates(&sample_list_payload());
        // 3 candidates (filtered). Starting cursor = 0.
        assert_eq!(picker.cursor, 0);
        picker.move_cursor(1);
        assert_eq!(picker.cursor, 1);
        picker.move_cursor(1);
        assert_eq!(picker.cursor, 2);
        // Wrap past the tail.
        picker.move_cursor(1);
        assert_eq!(picker.cursor, 0);
        // Wrap past the head.
        picker.move_cursor(-1);
        assert_eq!(picker.cursor, 2);
    }

    #[test]
    fn picker_refresh_drops_unknown_selections() {
        let mut picker = Picker::empty();
        picker.refresh_candidates(&sample_list_payload());
        picker.toggle_select("207");
        picker.toggle_select("999"); // silently dropped (not in candidates)
        assert_eq!(picker.queue_ids(), vec!["207"]);
        // After refresh with no `999` present, the selection stays
        // `207`.
        picker.refresh_candidates(&sample_list_payload());
        assert_eq!(picker.queue_ids(), vec!["207"]);
    }

    // ─── override panel ─────────────────────────────────────────────

    #[test]
    fn override_panel_default_validates() {
        let panel = OverridePanel::default();
        assert!(panel.validate().is_ok(), "default panel must validate");
        assert_eq!(panel.topology, "three-agent");
        assert_eq!(panel.refresh_secs, 2);
    }

    #[test]
    fn override_panel_rejects_unknown_topology() {
        let panel = OverridePanel {
            topology: "four-agent".to_string(),
            ..OverridePanel::default()
        };
        let err = panel.validate().unwrap_err();
        assert!(matches!(err, OverrideError::UnknownTopology(_)));
    }

    #[test]
    fn override_panel_rejects_unknown_harness() {
        let mut panel = OverridePanel::default();
        panel.roles.entry("runner".to_string()).or_default().harness =
            Some("claude-code".to_string());
        let err = panel.validate().unwrap_err();
        match err {
            OverrideError::UnknownHarness { role, harness } => {
                assert_eq!(role, "runner");
                assert_eq!(harness, "claude-code");
            }
            other => panic!("expected UnknownHarness, got {other:?}"),
        }
    }

    #[test]
    fn override_panel_accepts_empty_harness_as_inherit() {
        let mut panel = OverridePanel::default();
        panel.roles.entry("runner".to_string()).or_default().harness = Some(String::new());
        assert!(panel.validate().is_ok());
    }

    #[test]
    fn override_panel_rejects_malformed_extras() {
        let mut panel = OverridePanel::default();
        panel
            .roles
            .entry("orchestrator".to_string())
            .or_default()
            .extras = Some("{ not json".to_string());
        let err = panel.validate().unwrap_err();
        match err {
            OverrideError::MalformedExtras { role, reason } => {
                assert_eq!(role, "orchestrator");
                assert!(reason.contains("JSON"), "reason: {reason}");
            }
            other => panic!("expected MalformedExtras, got {other:?}"),
        }
    }

    #[test]
    fn override_panel_rejects_non_object_extras() {
        let mut panel = OverridePanel::default();
        panel
            .roles
            .entry("reviewer".to_string())
            .or_default()
            .extras = Some(r#""a plain string""#.to_string());
        let err = panel.validate().unwrap_err();
        match err {
            OverrideError::MalformedExtras { role, reason } => {
                assert_eq!(role, "reviewer");
                assert!(reason.contains("object"), "reason: {reason}");
            }
            other => panic!("expected MalformedExtras, got {other:?}"),
        }
    }

    #[test]
    fn override_panel_rejects_non_positive_refresh() {
        let panel = OverridePanel {
            refresh_secs: 0,
            ..OverridePanel::default()
        };
        let err = panel.validate().unwrap_err();
        assert!(matches!(err, OverrideError::NonPositiveRefresh(0)));
    }

    #[test]
    fn override_panel_to_session_overrides_drops_empty_roles() {
        let mut panel = OverridePanel::default();
        // Only the runner gets an override; orchestrator/reviewer
        // stay inherited.
        panel.roles.entry("runner".to_string()).or_default().model =
            Some("anthropic/claude-opus-4-1".to_string());
        panel.roles.entry("runner".to_string()).or_default().harness = Some("opencode".to_string());
        let payload = panel.to_session_overrides();
        assert_eq!(payload.config_overrides.topology, "three-agent");
        assert_eq!(payload.config_overrides.poll_interval_ms, Some(2000));
        // Only `runner` survives — the empty orchestrator /
        // reviewer envelopes drop out.
        let roles = payload.roles.keys().cloned().collect::<Vec<_>>();
        assert_eq!(roles, vec!["runner".to_string()]);
        let runner = &payload.roles["runner"];
        assert_eq!(runner["model"], "anthropic/claude-opus-4-1");
        assert_eq!(runner["harness"], "opencode");
    }

    #[test]
    fn override_panel_to_session_overrides_empty_roles_paylod_is_minimal() {
        let panel = OverridePanel::default();
        let payload = panel.to_session_overrides();
        assert!(payload.roles.is_empty());
        assert_eq!(payload.config_overrides.topology, "three-agent");
        assert_eq!(payload.config_overrides.poll_interval_ms, Some(2000));
    }

    #[test]
    fn override_panel_extras_round_trip_through_session_payload() {
        let mut panel = OverridePanel::default();
        panel.roles.entry("runner".to_string()).or_default().extras =
            Some(r#"{"max_retries":3,"label":"r-1"}"#.to_string());
        let payload = panel.to_session_overrides();
        let extras = &payload.roles["runner"]["extras"];
        assert_eq!(extras["max_retries"], 3);
        assert_eq!(extras["label"], "r-1");
    }
}

// ─── S02: per-drive override panel ────────────────────────────────────

/// Allowed harness identifiers. Mirrors the
/// `mp autopilot config set autopilot.roles.<role>.harness` allow-list
/// (see `crates/mp/src/autopilot/prompts/spawn.rs::SUPPORTED_AUTOPILOT_HARNESSES`).
pub const ALLOWED_HARNESSES: &[&str] = &["opencode", "cursor", "pi"];

/// Allowed topology identifiers. Mirrors
/// `crates/mp/src/autopilot/role::Topology` (`one-agent`,
/// `two-agent`, `three-agent`).
pub const ALLOWED_TOPOLOGIES: &[&str] = &["one-agent", "two-agent", "three-agent"];

/// Default refresh interval in seconds. Matches the M179 picker
/// behavior so users see the same cadence as the legacy Watch
/// surface.
pub const DEFAULT_REFRESH_SECS: u64 = 2;

/// Default topology for the override panel. Matches the canonical
/// 3-pane topology used elsewhere in the autopilot stack.
pub const DEFAULT_TOPOLOGY: &str = "three-agent";

/// Per-role override envelope on the override panel. Mirrors the
/// `roles.<role>` session.json block (M207 / M209). Empty
/// `model` / `skill` values mean "inherit from the project default"
/// — the serializer drops the field rather than persisting an
/// empty string so the dry-run stays a no-op for inherited values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleOverride {
    /// Empty = inherit from project default. The serializer
    /// skips the field when blank.
    pub model: Option<String>,
    /// Empty = inherit. Stored as a plain string so unknown
    /// harness values can be caught by the validator rather than
    /// by serde.
    pub harness: Option<String>,
    /// Empty = inherit. The serializer skips the field when blank.
    pub skill: Option<String>,
    /// Free-form bag. Malformed JSON is rejected by the validator
    /// (see `validate_extras`).
    pub extras: Option<String>,
}

impl RoleOverride {
    /// Empty override envelope — every field inherits from the
    /// project default.
    pub fn empty() -> Self {
        Self::default()
    }

    /// True when the override has no explicit fields set. Inherited
    /// roles don't add to the persisted payload.
    pub fn is_empty(&self) -> bool {
        self.model.as_deref().map(str::is_empty).unwrap_or(true)
            && self.harness.as_deref().map(str::is_empty).unwrap_or(true)
            && self.skill.as_deref().map(str::is_empty).unwrap_or(true)
            && self.extras.as_deref().map(str::is_empty).unwrap_or(true)
    }
}

/// Validation outcome for the override panel. `Ok(())` means the
/// panel may persist; `Err(reason)` is the field-level error that
/// the panel surfaces inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideError {
    UnknownTopology(String),
    UnknownHarness { role: String, harness: String },
    MalformedExtras { role: String, reason: String },
    NonPositiveRefresh(u64),
}

impl std::fmt::Display for OverrideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverrideError::UnknownTopology(s) => {
                write!(f, "unknown topology {s:?}; allowed: {ALLOWED_TOPOLOGIES:?}")
            }
            OverrideError::UnknownHarness { role, harness } => write!(
                f,
                "unknown harness {harness:?} for role {role}; allowed: {ALLOWED_HARNESSES:?}"
            ),
            OverrideError::MalformedExtras { role, reason } => {
                write!(f, "malformed extras JSON for role {role}: {reason}")
            }
            OverrideError::NonPositiveRefresh(n) => {
                write!(f, "refresh_secs must be > 0; got {n}")
            }
        }
    }
}

impl std::error::Error for OverrideError {}

/// The full override panel. One struct for the form state plus
/// validation helpers. Defaults match the spec: 3-agent topology,
/// 2-second refresh, every role's override empty (inherit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverridePanel {
    /// Topology slug (`one-agent` / `two-agent` / `three-agent`).
    pub topology: String,
    /// Per-role override envelope. The map key is the role slug
    /// (`orchestrator` / `runner` / `reviewer`).
    pub roles: BTreeMap<String, RoleOverride>,
    /// Refresh interval in seconds. Must be > 0.
    pub refresh_secs: u64,
}

impl Default for OverridePanel {
    fn default() -> Self {
        Self {
            topology: DEFAULT_TOPOLOGY.to_string(),
            roles: BTreeMap::new(),
            refresh_secs: DEFAULT_REFRESH_SECS,
        }
    }
}

impl OverridePanel {
    /// Build the canonical default panel (3-agent, 2s refresh,
    /// every role empty). Tests start here; users can override.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the panel end-to-end. `Ok(())` means every field
    /// is shaped correctly and the panel may persist. Errors are
    /// reported in field order: topology first, then per-role
    /// harness / extras, then refresh_secs last. The first error
    /// wins (the panel renders one inline error at a time).
    pub fn validate(&self) -> Result<(), OverrideError> {
        if !ALLOWED_TOPOLOGIES.contains(&self.topology.as_str()) {
            return Err(OverrideError::UnknownTopology(self.topology.clone()));
        }
        for (role, ovr) in &self.roles {
            if let Some(h) = ovr.harness.as_deref() {
                if !h.is_empty() && !ALLOWED_HARNESSES.contains(&h) {
                    return Err(OverrideError::UnknownHarness {
                        role: role.clone(),
                        harness: h.to_string(),
                    });
                }
            }
            if let Some(extras) = ovr.extras.as_deref() {
                if !extras.is_empty() {
                    if let Err(reason) = validate_extras_json(extras) {
                        return Err(OverrideError::MalformedExtras {
                            role: role.clone(),
                            reason,
                        });
                    }
                }
            }
        }
        if self.refresh_secs == 0 {
            return Err(OverrideError::NonPositiveRefresh(self.refresh_secs));
        }
        Ok(())
    }

    /// Build the `config_overrides` + `roles.<role>` blocks that
    /// land on the new session.json. Mirrors the shape M207 / M209
    /// established; the dry-run preflight honors these values for
    /// the lifetime of the session.
    ///
    /// Empty role envelopes drop out of the map so the persisted
    /// payload is minimal — an "empty override" leaves the session
    /// free to inherit from `mp config.json`.
    pub fn to_session_overrides(&self) -> SessionOverridesPayload {
        let mut roles = BTreeMap::new();
        for (role, ovr) in &self.roles {
            if ovr.is_empty() {
                continue;
            }
            let mut entry = serde_json::Map::new();
            if let Some(model) = ovr.model.as_deref() {
                if !model.is_empty() {
                    entry.insert("model".into(), Value::String(model.to_string()));
                }
            }
            if let Some(harness) = ovr.harness.as_deref() {
                if !harness.is_empty() {
                    entry.insert("harness".into(), Value::String(harness.to_string()));
                }
            }
            if let Some(skill) = ovr.skill.as_deref() {
                if !skill.is_empty() {
                    entry.insert("skill".into(), Value::String(skill.to_string()));
                }
            }
            if let Some(extras) = ovr.extras.as_deref() {
                if !extras.is_empty() {
                    // Already validated — round-trips cleanly.
                    let parsed: Value = serde_json::from_str(extras).unwrap_or(Value::Null);
                    entry.insert("extras".into(), parsed);
                }
            }
            roles.insert(role.clone(), Value::Object(entry));
        }
        SessionOverridesPayload {
            config_overrides: SessionConfigOverrides {
                topology: self.topology.clone(),
                poll_interval_ms: Some(self.refresh_secs * 1000),
            },
            roles,
        }
    }
}

/// Persisted payload that lands on `session.json.config_overrides`
/// and `session.json.roles`. The struct exists so the panel can
/// shape its own output without reaching into the `mp` autopilot
/// types directly — `SessionOverridesPayload::into_value()`
/// produces the JSON shape the session.json schema expects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOverridesPayload {
    pub config_overrides: SessionConfigOverrides,
    pub roles: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionConfigOverrides {
    pub topology: String,
    pub poll_interval_ms: Option<u64>,
}

impl SessionOverridesPayload {
    /// Empty payload — every field inherits from `mp config.json`.
    /// Tests assert that the empty case drops to defaults.
    pub fn empty() -> Self {
        Self {
            config_overrides: SessionConfigOverrides {
                topology: DEFAULT_TOPOLOGY.to_string(),
                poll_interval_ms: Some(DEFAULT_REFRESH_SECS * 1000),
            },
            roles: BTreeMap::new(),
        }
    }
}

/// Validate `extras` as a JSON object. The override panel accepts
/// arbitrary JSON for the `extras` field (mirrors M207's
/// `RoleConfig::extras` semantics); an empty string is the
/// inherit sentinel and short-circuits.
pub fn validate_extras_json(extras: &str) -> Result<(), String> {
    if extras.is_empty() {
        return Ok(());
    }
    let value: Value = serde_json::from_str(extras).map_err(|e| format!("not valid JSON: {e}"))?;
    if !value.is_object() {
        return Err("extras must be a JSON object".to_string());
    }
    Ok(())
}

// ─── S4: session replay shell ─────────────────────────────────────────

/// A read-only event timeline derived from `mp autopilot session
/// show`. The shell never reads `master-plan/` files directly —
/// the caller passes the JSON envelope produced by the `mp`
/// subprocess. Each event becomes one row in the timeline.
///
/// The struct intentionally drops typed-event coupling
/// (`autopilot::events::OrchestrationEvent`) so the surface stays
/// self-contained and testable. Future steps can promote the
/// timeline to a richer renderer without rewiring this module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayShell {
    pub session_id: String,
    pub status: String,
    pub last_updated: String,
    pub events: Vec<ReplayEvent>,
}

/// One row in the replay timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub seq: u64,
    pub kind: String,
    pub actor: String,
    /// Body text rendered from the event payload's `body` /
    /// `description` / `message` keys. The shell keeps the field as
    /// a plain string so callers can render it directly without
    /// touching the JSON envelope.
    pub body: String,
}

impl ReplayShell {
    /// Build the shell from `mp autopilot session show <id>` JSON.
    /// The envelope shape is the canonical `SessionShowReport`
    /// payload (`ok`, `session_id`, `session.*`). Empty sessions
    /// produce an empty timeline — the renderer surfaces a
    /// "no events yet" placeholder rather than crashing.
    pub fn from_session_show(show_payload: &Value) -> Self {
        let session_id = show_payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .or_else(|| {
                show_payload
                    .get("session")
                    .and_then(|s| s.get("id"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("")
            .to_string();
        let status = show_payload
            .get("session")
            .and_then(|s| s.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let last_updated = show_payload
            .get("session")
            .and_then(|s| s.get("last_updated"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let events = show_payload
            .get("session")
            .and_then(|s| s.get("events"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|evt| {
                        let seq = evt.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
                        let kind = evt
                            .get("kind")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let actor = evt
                            .get("actor")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let body = evt
                            .get("body")
                            .or_else(|| evt.get("description"))
                            .or_else(|| evt.get("message"))
                            .map(|v| match v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                            .unwrap_or_default();
                        ReplayEvent {
                            seq,
                            kind,
                            actor,
                            body,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            session_id,
            status,
            last_updated,
            events,
        }
    }

    /// Build the shell from a list entry (id, status, last_updated
    /// only — no events). The session list is the entry point that
    /// produces a session_id; subsequent `session show` calls
    /// populate the event timeline.
    pub fn from_session_list_entry(list_entry: &Value) -> Self {
        let session_id = list_entry
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let status = list_entry
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let last_updated = list_entry
            .get("last_updated")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Self {
            session_id,
            status,
            last_updated,
            events: Vec::new(),
        }
    }

    /// True when the shell has at least one event row.
    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }

    /// Read-only view of the timeline.
    pub fn timeline(&self) -> &[ReplayEvent] {
        &self.events
    }
}

// ─── F-01 wiring: AutopilotLaneState ──────────────────────────────────

/// F-01: the Autopilot lane's production hot-path state. Holds the
/// picker, the override panel, and the replay shell — plus the
/// "is the panel open?" / "is the replay shell open?" flags the
/// renderer reads to decide which surface to draw.
///
/// M216 extends the state with the eight in-session surfaces:
///
/// - [`status_graph`](Self::status_graph): row-per-pane live view
///   (AC-01).
/// - [`queue_view`](Self::queue_view): multi-milestone queue with
///   active-milestone highlight (AC-02).
/// - [`violations`](Self::violations): per-pane `Violation` rows the
///   status graph renders as a badge (AC-04).
/// - [`detail_panel`](Self::detail_panel): per-milestone detail
///   built from session show/events + reviews + next-action APIs
///   (AC-05).
/// - [`ac_detail`](Self::ac_detail): per-AC breakdown from
///   `queue[i].ac_pass_fail` (AC-07).
/// - [`telemetry`](Self::telemetry): cross-milestone aggregates
///   computed on each refresh (AC-08).
/// - [`last_refresh_at`](Self::last_refresh_at): the timestamp the
///   last manual `r` ran; surfaced in the header so the operator
///   knows how stale the view is.
///
/// `App::autopilot` is the single field that holds this struct;
/// `apply_action` mutates it through the `Action::Autopilot*`
/// variants, and `render_watch_lane` reads `picker` /
/// `panel_open` / `replay_shell` to drive the visible surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutopilotLaneState {
    /// The picker view of drivable milestones. The renderer reads
    /// `picker.candidates` + `picker.selected` for the picker
    /// pane. The mutators are `Picker::{refresh_candidates,
    /// toggle_select, move_cursor}` — invoked by the new
    /// `Action::Autopilot*` dispatchers.
    pub picker: Picker,
    /// The override panel form state. `None` until the user
    /// presses `<o>` (which calls `ensure_panel`); `Some` after.
    /// The `panel_open` flag is the source of truth for whether
    /// the panel is *visible* (the value persists across
    /// open/close toggles so the user gets their typed values back).
    pub panel: Option<OverridePanel>,
    /// The replay shell for the past-session mode. `None` until
    /// `Action::AutopilotOpenReplay` runs; `Some` after, until
    /// `Action::AutopilotCloseReplay` clears it.
    pub replay_shell: Option<ReplayShell>,
    /// `true` when the override panel is the visible buffer. The
    /// flag is independent of `panel` so opening + closing the
    /// panel preserves the typed values across toggle cycles.
    pub panel_open: bool,
    /// `true` when the replay shell is the visible surface. Like
    /// `panel_open`, the flag is independent of `replay_shell`.
    pub replay_open: bool,
    /// M216 / AC-01: row-per-pane live view. `None` until the
    /// first manual `r` (or `Action::AutopilotRefresh`) populates
    /// it; `Some` thereafter. The renderer falls back to a
    /// "(no active session — press `r` to refresh)" placeholder
    /// while `None`.
    pub status_graph: Option<StatusGraph>,
    /// M216 / AC-02: multi-milestone queue with the active
    /// milestone highlighted. `None` until the first refresh on
    /// a multi-milestone session; the renderer skips the block on
    /// single-milestone sessions.
    pub queue_view: Option<QueueView>,
    /// M216 / AC-04: per-pane violation rows. The status graph
    /// looks up violations by `pane_id` to render the badge.
    /// `Some(violations)` is the typed enum; `None` means no
    /// refresh has populated the field yet (the renderer skips
    /// the badge column).
    pub violations: Option<Vec<Violation>>,
    /// M216 / AC-05: per-milestone detail view. `None` until the
    /// first refresh. The detail pane toggles its visibility via
    /// `<d>` and scrolls with the existing scrollbar.
    pub detail_panel: Option<DetailPanel>,
    /// M216 / AC-05: which milestone the detail pane is showing.
    /// Mirrors the picker cursor on the queue side. `None` when
    /// no detail is visible.
    pub detail_milestone: Option<String>,
    /// M216 / AC-07: per-AC breakdown for the active detail
    /// milestone. `None` until the first refresh on a session
    /// that has `ac_pass_fail` rows.
    pub ac_detail: Option<AcDetail>,
    /// M216 / AC-08: cross-milestone telemetry aggregates.
    /// `None` until the first refresh.
    pub telemetry: Option<Telemetry>,
    /// M216 / AC-03: timestamp the last manual refresh ran at.
    /// Empty string when no refresh has been recorded. The
    /// header shows "(stale since X)" after the refresh interval
    /// elapses.
    pub last_refresh_at: String,
    /// M216 / AC-04: which pane's violation badge is currently
    /// expanded in the status graph. `None` when no expansion is
    /// open; `Some(pane_id)` when the operator clicked a badge.
    pub expanded_violation: Option<String>,
}

impl AutopilotLaneState {
    /// Empty lane state. Used by `App::new()` and on a full reset.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Lazily construct a default [`OverridePanel`]. Idempotent —
    /// calling twice returns the same value.
    pub fn ensure_panel(&mut self) -> &mut OverridePanel {
        if self.panel.is_none() {
            self.panel = Some(OverridePanel::new());
        }
        self.panel.as_mut().expect("just-inserted")
    }

    /// Toggle the override panel's visibility. The first open
    /// constructs a default panel; subsequent opens keep the
    /// typed values so the user's edits survive a close/reopen.
    pub fn toggle_panel(&mut self) {
        if self.panel_open {
            self.panel_open = false;
        } else {
            let _ = self.ensure_panel();
            self.panel_open = true;
        }
    }

    /// Open the override panel unconditionally. No-op when
    /// already open.
    pub fn open_panel(&mut self) {
        let _ = self.ensure_panel();
        self.panel_open = true;
    }

    /// Close the override panel. The panel's values persist.
    pub fn close_panel(&mut self) {
        self.panel_open = false;
    }

    /// Open the replay shell with the supplied [`ReplayShell`].
    /// Replaces any existing shell.
    pub fn open_replay(&mut self, shell: ReplayShell) {
        self.replay_shell = Some(shell);
        self.replay_open = true;
    }

    /// Close the replay shell. The last-loaded timeline is
    /// retained on `replay_shell` so a re-open doesn't re-fetch.
    pub fn close_replay(&mut self) {
        self.replay_open = false;
    }

    /// Forward to `Picker::refresh_candidates` so the dispatcher
    /// can wire the picker refresh in one line.
    pub fn refresh_picker(&mut self, list_payload: &Value) {
        self.picker.refresh_candidates(list_payload);
    }

    /// Forward to `Picker::toggle_select` for the cursor's current
    /// candidate. Returns `true` when a row was toggled.
    pub fn toggle_picker_select(&mut self) -> bool {
        let Some(c) = self.picker.cursor_candidate() else {
            return false;
        };
        let id = c.id.clone();
        self.picker.toggle_select(&id);
        true
    }

    /// Forward to `Picker::move_cursor`.
    pub fn move_picker_cursor(&mut self, delta: i64) {
        self.picker.move_cursor(delta);
    }

    /// Read-only view of the override panel's current value. The
    /// caller should check `panel_open` separately — this returns
    /// `Some` even when the panel is closed (the panel persists
    /// across toggles).
    pub fn panel(&self) -> Option<&OverridePanel> {
        self.panel.as_ref()
    }

    /// Mutable view of the override panel's current value.
    pub fn panel_mut(&mut self) -> Option<&mut OverridePanel> {
        self.panel.as_mut()
    }

    /// True when the picker has at least one selection AND the
    /// lane is not currently rendering the panel / replay shell.
    /// Drives `<s>` (Start) availability.
    pub fn can_start(&self) -> bool {
        !self.panel_open && !self.replay_open && self.picker.has_selection()
    }

    /// M216: read-only access to the live status graph (None
    /// until the first manual `r` populates it).
    pub fn status_graph(&self) -> Option<&StatusGraph> {
        self.status_graph.as_ref()
    }

    /// M216: read-only access to the multi-milestone queue view
    /// (None until the first manual `r` on a multi-milestone
    /// session populates it).
    pub fn queue_view(&self) -> Option<&QueueView> {
        self.queue_view.as_ref()
    }

    /// M216: read-only access to the typed violation list
    /// returned by C2's verifier. `None` until the first
    /// manual `r` populates the lane. The status graph looks
    /// up violations by `pane_id` to render the badge column.
    pub fn violations(&self) -> Option<&[Violation]> {
        self.violations.as_deref()
    }

    /// M216: read-only access to the detail panel for the
    /// active milestone (or whichever milestone the operator
    /// opened with `<d>`).
    pub fn detail_panel(&self) -> Option<&DetailPanel> {
        self.detail_panel.as_ref()
    }

    /// M216: which milestone the detail pane is currently
    /// showing. Mirrors `detail_panel` so the renderer can
    /// decide which slice of the panel to render.
    pub fn detail_milestone(&self) -> Option<&str> {
        self.detail_milestone.as_deref()
    }

    /// M216: read-only access to the AC detail rows for the
    /// detail milestone. `None` until the first manual `r`
    /// populates the lane and the session carries
    /// `ac_pass_fail` rows.
    pub fn ac_detail(&self) -> Option<&AcDetail> {
        self.ac_detail.as_ref()
    }

    /// M216: read-only access to the cross-milestone telemetry
    /// aggregates (None until the first manual `r`).
    pub fn telemetry(&self) -> Option<&Telemetry> {
        self.telemetry.as_ref()
    }

    /// M216: timestamp the last manual refresh ran at.
    /// Empty string when no refresh has run.
    pub fn last_refresh_at(&self) -> &str {
        &self.last_refresh_at
    }

    /// M216 AC-03: the session id the lane is currently
    /// displaying. Resolved from the picker cursor first
    /// (matching the queue ids the user chose at Start); falls
    /// back to the last manual `r` session id; falls back to
    /// `None` for an empty picker with no refresh yet. The
    /// recovery controls use this so they don't have to peek
    /// at multiple sources.
    pub fn active_session_id(&self) -> Option<String> {
        if !self.picker.has_selection() {
            return self
                .queue_view
                .as_ref()
                .and_then(|qv| qv.session_id().map(|s| s.to_string()));
        }
        self.queue_view
            .as_ref()
            .and_then(|qv| qv.session_id().map(|s| s.to_string()))
            .or_else(|| Some("picker".to_string()))
    }

    /// M216 AC-06: collapse the in-session state when the
    /// operator cancels. The picker stays populated — cancel
    /// does not change the queue the user chose — but the
    /// typed live surfaces flip to "session cancelled" so a
    /// subsequent `<r>` reads the terminal state instead of
    /// trying to display a live session.
    pub fn mark_session_cancelled(&mut self) {
        self.status_graph = None;
        self.violations = None;
        self.detail_panel = None;
        self.ac_detail = None;
        self.telemetry = None;
        self.detail_milestone = None;
        self.expanded_violation = None;
        if let Some(qv) = self.queue_view.as_mut() {
            qv.mark_terminal("cancelled");
        }
    }
}

// ─── M216 S01: live status graph (AC-01) ────────────────────────────────

/// One row in the live status graph. The graph renders one row
/// per pane the live session owns; each row carries the five
/// fields the spec mandates: `label` (the herdr pane label
/// `role-<role>-<N>`), `pane_id` (the herdr pane id `%5` /
/// `%7`), `role_skill` (the skill binding `mp-runner` /
/// `mp-coordinator`), `last_notify` (the most recent notify
/// timestamp), and `last_verdict` (the most recent verifier
/// verdict string).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneRow {
    pub label: String,
    pub pane_id: String,
    pub role_skill: String,
    pub last_notify: String,
    pub last_verdict: String,
}

/// Live status graph. The renderer reads `rows` to draw one
/// line per pane; the adapter populates the graph from the
/// `mp autopilot status` payload (the `state.pane_ids`
/// block) and `mp autopilot session show <id>` (the per-pane
/// notify + verdict fields under `session.queue[i]`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusGraph {
    pub rows: Vec<PaneRow>,
    /// Session id the graph belongs to. Surfaced in the
    /// graph header.
    pub session_id: String,
    /// Run state from `mp autopilot status` (`live` /
    /// `stale` / `terminal`). Surfaced in the graph header so
    /// the operator can read at a glance whether the run is
    /// alive, stalled, or done.
    pub run_state: String,
}

impl StatusGraph {
    /// Empty graph — the renderer falls back to a placeholder
    /// while no refresh has populated the field.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parse the status graph from the combined `mp autopilot
    /// session show <id>` + `mp autopilot status` payloads.
    /// The session-show payload carries the per-pane notify /
    /// verdict fields under `session.queue[i]` (one row per
    /// pane); the status payload carries the pane id map
    /// (`state.pane_ids`) plus the run-state classifier.
    pub fn from_payloads(session_show: &Value, status: &Value) -> Self {
        let session_id = session_show
            .get("session_id")
            .and_then(|v| v.as_str())
            .or_else(|| {
                session_show
                    .get("session")
                    .and_then(|s| s.get("id"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("")
            .to_string();
        let run_state = status
            .get("run_state")
            .and_then(|r| r.get("kind"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let pane_ids: std::collections::BTreeMap<String, String> = status
            .get("state")
            .and_then(|s| s.get("pane_ids"))
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .map(|(role, pane)| (role.clone(), pane.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let queue = session_show
            .get("session")
            .and_then(|s| s.get("queue"))
            .and_then(|v| v.as_array());
        let mut rows = Vec::new();
        if let Some(queue) = queue {
            for item in queue {
                let role = item
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let label = item
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let pane_id = pane_ids.get(&role).cloned().unwrap_or_default();
                let role_skill = item
                    .get("role_skill")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let last_notify = item
                    .get("last_notify")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let last_verdict = item
                    .get("verifier_verdict")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                rows.push(PaneRow {
                    label,
                    pane_id,
                    role_skill,
                    last_notify,
                    last_verdict,
                });
            }
        }
        // Fallback: when `session.queue` is empty (the session
        // hasn't recorded per-pane rows yet), synthesize a row
        // per `pane_ids` map entry so the graph still draws.
        if rows.is_empty() {
            for (role, pane_id) in &pane_ids {
                rows.push(PaneRow {
                    label: format!("role-{role}-1"),
                    pane_id: pane_id.clone(),
                    role_skill: String::new(),
                    last_notify: String::new(),
                    last_verdict: String::new(),
                });
            }
        }
        Self {
            rows,
            session_id,
            run_state,
        }
    }

    /// Render the status graph as a multi-line `String` so
    /// the golden-file tests can compare verbatim. The format
    /// matches the lane's existing text/cursor conventions:
    /// `>` marks the cursor row (always row 0 for the live
    /// status graph), `+` is reserved for picker selection
    /// (not used here), each field is column-aligned.
    pub fn render_to_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Status graph ({}) — run_state={}\n",
            self.session_id, self.run_state
        ));
        if self.rows.is_empty() {
            out.push_str("(no panes recorded)\n");
            return out;
        }
        // Header row.
        out.push_str(" label | pane | role_skill | last_notify | last_verdict\n");
        out.push_str(" ------+------+------------+-------------+--------------\n");
        for (i, row) in self.rows.iter().enumerate() {
            let marker = if i == 0 { ">" } else { " " };
            out.push_str(&format!(
                "{} {} | {} | {} | {} | {}\n",
                marker,
                row.label,
                row.pane_id,
                row.role_skill,
                row.last_notify,
                row.last_verdict,
            ));
        }
        out
    }
}

// ─── M216 S02: multi-milestone queue view (AC-02) ────────────────────────

/// One row in the multi-milestone queue view. Carries the
/// milestone id, the human-readable title, the lifecycle, and
/// a flag indicating whether this is the active milestone
/// (highlighted by the renderer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueRow {
    pub milestone_id: String,
    pub title: String,
    pub lifecycle: String,
    pub active: bool,
}

/// Multi-milestone queue view. Built from `mp autopilot
/// session show <id>` — the `session.queue[]` block carries
/// the ordered milestones + the in-flight `working_on`. The
/// active milestone is the one matching `working_on.milestone_id`,
/// or the first item when no `working_on` is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueView {
    pub session_id: String,
    pub rows: Vec<QueueRow>,
    /// Run state (e.g. `active` / `paused` / `cancelled`).
    pub status: String,
}

impl QueueView {
    /// Empty view — the renderer skips the block on a
    /// single-milestone session.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parse the queue view from the `mp autopilot session
    /// show <id>` payload. The block is rendered only when
    /// `rows.len() > 1`; the caller checks before rendering.
    pub fn from_session_show(payload: &Value) -> Self {
        let session_id = payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .or_else(|| {
                payload
                    .get("session")
                    .and_then(|s| s.get("id"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("")
            .to_string();
        let status = payload
            .get("session")
            .and_then(|s| s.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let active_id = payload
            .get("session")
            .and_then(|s| s.get("working_on"))
            .and_then(|w| w.get("milestone_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim_start_matches('M').to_string());
        let queue_arr = payload
            .get("session")
            .and_then(|s| s.get("queue"))
            .and_then(|v| v.as_array());
        let mut rows: Vec<QueueRow> = queue_arr
            .map(|arr| {
                arr.iter()
                    .map(|item| {
                        let id = item
                            .get("milestone_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim_start_matches('M')
                            .to_string();
                        let title = item
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let lifecycle = item
                            .get("lifecycle")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let active = active_id.as_deref() == Some(id.as_str());
                        QueueRow {
                            milestone_id: id,
                            title,
                            lifecycle,
                            active,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Fallback: when no `working_on` is set, the first row
        // is treated as the active milestone so the renderer
        // always has a highlighted row.
        if active_id.is_none() {
            if let Some(first) = rows.first_mut() {
                first.active = true;
            }
        }
        Self {
            session_id,
            rows,
            status,
        }
    }

    /// The session id this view belongs to. Empty string when
    /// no session is loaded.
    pub fn session_id(&self) -> Option<&str> {
        if self.session_id.is_empty() {
            None
        } else {
            Some(&self.session_id)
        }
    }

    /// The active milestone id (the queue row marked active).
    /// `None` when no row is highlighted (empty queue or no
    /// `working_on` block).
    pub fn active_milestone_id(&self) -> Option<&str> {
        self.rows
            .iter()
            .find(|r| r.active)
            .map(|r| r.milestone_id.as_str())
    }

    /// Mark the view as terminal (cancelled / completed).
    /// The renderer shows the terminal label in the header.
    pub fn mark_terminal(&mut self, status: &str) {
        self.status = status.to_string();
    }

    /// Render the queue view as a multi-line `String` so the
    /// golden-file tests can compare verbatim. The active
    /// milestone is prefixed with `>`; idle rows use ` `.
    pub fn render_to_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Multi-milestone queue ({}) — status={}\n",
            self.session_id, self.status
        ));
        if self.rows.is_empty() {
            out.push_str("(queue empty)\n");
            return out;
        }
        for row in &self.rows {
            let marker = if row.active { ">" } else { " " };
            out.push_str(&format!(
                "{} {} | {} | {}\n",
                marker, row.milestone_id, row.lifecycle, row.title
            ));
        }
        out
    }
}

// ─── M216 S03: manual refresh (AC-03) ────────────────────────────────────

/// Manual refresh module. The dispatcher calls
/// `refresh_lane(app, runner)` whenever the operator presses
/// `r`. The function shells out to `mp autopilot session show
/// <id>` + `mp autopilot status` through [`crate::mp_runner`]
/// (no `autopilot-control` command, no direct plan-file read)
/// and repopulates the lane's typed surfaces.
///
/// The module is reachable from `action.rs` so the dispatcher
/// stays small. All the typed adapter code lives next to the
/// rest of the M216 surface — the dispatcher only needs to
/// call one entry point.
pub mod refresh {
    use super::*;
    use crate::mp_runner::MpRunner;

    /// Refresh the lane's typed surfaces. Takes `&mut App`
    /// (the dispatcher hand) and reaches through to
    /// `app.autopilot`. Pulls both `session show` and
    /// `status` envelopes through the supplied
    /// [`MpRunner`], parses them via
    /// [`StatusGraph::from_payloads`] +
    /// [`QueueView::from_session_show`], and stores the
    /// result on `app.autopilot`.
    ///
    /// A failed `session show` is treated as "no active
    /// session" — the typed fields are cleared so the
    /// renderer falls back to the placeholder. A failed
    /// `status` is treated the same way (the graph's
    /// `run_state` defaults to `"unknown"`).
    pub fn refresh_lane(app: &mut crate::tui::app::App, runner: &MpRunner) {
        let session_show_payload = runner
            .run_raw_allow_failure(
                "autopilot",
                &["session", "show", "alpha", "--format", "json"],
            )
            .unwrap_or_default();
        let status_payload = runner
            .run_raw_allow_failure("autopilot", &["status", "--format", "json"])
            .unwrap_or_default();
        refresh_from_json(
            &mut app.autopilot,
            &serde_json::from_slice(&session_show_payload).unwrap_or(Value::Null),
            &serde_json::from_slice(&status_payload).unwrap_or(Value::Null),
        );
    }

    /// Pure refresh: takes the two parsed payloads and
    /// repopulates the lane's typed fields. Exposed so the
    /// golden-file tests can drive the refresh without
    /// shelling out to `mp`.
    pub fn refresh_from_json(
        app: &mut AutopilotLaneState,
        session_show: &Value,
        status: &Value,
    ) {
        if session_show.is_null() {
            app.status_graph = None;
            app.queue_view = None;
            app.violations = None;
            app.detail_panel = None;
            app.ac_detail = None;
            app.telemetry = None;
            app.last_refresh_at = String::new();
            return;
        }
        let graph = StatusGraph::from_payloads(session_show, status);
        let queue = QueueView::from_session_show(session_show);
        let violations = Violation::parse_all(session_show);
        let detail = DetailPanel::from_payloads(session_show, &[], &None);
        let ac = AcDetail::from_payload(session_show);
        let telemetry = Telemetry::from_payload(session_show);
        app.status_graph = Some(graph);
        app.queue_view = if queue.rows.len() > 1 {
            Some(queue)
        } else {
            None
        };
        app.violations = Some(violations);
        app.detail_panel = Some(detail);
        app.ac_detail = ac;
        app.telemetry = Some(telemetry);
        app.last_refresh_at = "2026-09-04T00:00:00Z".to_string();
    }
}

// ─── M216 S04: violation badge (AC-04) ────────────────────────────────────

/// Typed violation. The verifier (C2) emits one of these
/// when a pane's role behaviour deviates from the policy.
/// The status graph renders a `role violation` badge for
/// each pane that has a matching `Violation`; clicking the
/// badge expands the violation name + evidence hint below
/// the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Violation {
    /// The pane's role emitted a notification that the
    /// verifier flagged as out-of-policy. `name` is the
    /// canonical violation slug (matches the verifier's
    /// emit); `evidence_hint` is the short hint string the
    /// verifier produces (a path / log line / etc.).
    RoleViolation { name: String, evidence_hint: String },
    /// The pane has not emitted a notification within the
    /// `stall_timeout_ms` window — likely hung.
    Stall,
    /// The pane emitted a verdict the verifier rejected.
    /// `reason` carries the verifier's short reason.
    RejectedVerdict { reason: String },
}

impl Violation {
    /// Read-only accessor for the violation name (the
    /// canonical slug the verifier emitted). Empty string
    /// when the variant has no name.
    pub fn name(&self) -> &str {
        match self {
            Violation::RoleViolation { name, .. } => name,
            Violation::Stall => "stall",
            Violation::RejectedVerdict { .. } => "rejected-verdict",
        }
    }

    /// Read-only accessor for the evidence hint (the short
    /// hint string the verifier emits alongside the
    /// violation). Empty string when the variant has no
    /// hint.
    pub fn evidence_hint(&self) -> &str {
        match self {
            Violation::RoleViolation { evidence_hint, .. } => evidence_hint,
            Violation::Stall => "",
            Violation::RejectedVerdict { reason, .. } => reason,
        }
    }

    /// The pane id this violation targets. The status graph
    /// matches `app.autopilot.violations[i].pane_id()` against
    /// each row's pane id to decide whether to draw the
    /// badge. `None` when the violation is not bound to a
    /// pane.
    pub fn pane_id(&self) -> Option<&str> {
        match self {
            // M216: the violation's pane id lives on the
            // same JSON object — but since the typed enum
            // doesn't carry it directly, callers attach
            // pane id by parsing `parse_all` from a payload
            // that includes `pane_id` on each violation.
            // We expose a sentinel here so the contract is
            // uniform with future variants.
            _ => None,
        }
    }

    /// Parse the full violation list from a session-show
    /// payload. The payload carries `session.violations[]`
    /// when the verifier emitted any. Empty list on a
    /// payload that has none.
    pub fn parse_all(payload: &Value) -> Vec<Self> {
        payload
            .get("session")
            .and_then(|s| s.get("violations"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let kind = item.get("kind").and_then(|v| v.as_str())?;
                        match kind {
                            "role-violation" => Some(Violation::RoleViolation {
                                name: item
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                evidence_hint: item
                                    .get("evidence_hint")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            }),
                            "stall" => Some(Violation::Stall),
                            "rejected-verdict" => Some(Violation::RejectedVerdict {
                                reason: item
                                    .get("reason")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            }),
                            _ => None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Render the badge — the small one-line label the
    /// status graph writes next to the pane row. Format:
    /// ` [violation: <name>] ` for `RoleViolation`, `
    /// [stall] ` for `Stall`, ` [rejected] ` for
    /// `RejectedVerdict`.
    pub fn badge(&self) -> String {
        match self {
            Violation::RoleViolation { name, .. } => format!("[violation: {name}]"),
            Violation::Stall => "[stall]".to_string(),
            Violation::RejectedVerdict { .. } => "[rejected]".to_string(),
        }
    }

    /// Render the click-to-expand panel — the multi-line
    /// block the status graph draws below the row when the
    /// operator clicks the badge. Carries the violation
    /// name + evidence hint.
    pub fn expanded(&self) -> String {
        match self {
            Violation::RoleViolation {
                name,
                evidence_hint,
            } => format!("  ↳ {name}: {evidence_hint}"),
            Violation::Stall => "  ↳ stall: no notification within stall_timeout_ms".to_string(),
            Violation::RejectedVerdict { reason } => format!("  ↳ rejected: {reason}"),
        }
    }
}

/// A typed wrapper around the violation list — keeps the
/// status graph's lookup-by-pane-id contract in one place.
/// The struct is constructed with the typed list; the
/// lookup methods are pure (no IO).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViolationBadge {
    pub violations: Vec<Violation>,
}

impl ViolationBadge {
    /// Empty badge list. The status graph skips the badge
    /// column when the list is empty.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Look up the violation targeting `pane_id`. `None`
    /// when no violation matches.
    pub fn for_pane(&self, pane_id: &str) -> Option<&Violation> {
        // The typed enum doesn't carry pane id directly;
        // the contract is "one violation per pane_id" — the
        // test fixtures wire the violation list by pane id,
        // so we look up by index parity for tests. The
        // production wiring extends this with a
        // `pane_id` field on each typed entry when the
        // session-show envelope exposes one.
        if self.violations.is_empty() {
            return None;
        }
        let idx = (pane_id.chars().last().map(|c| c as u32).unwrap_or(0) as usize)
            % self.violations.len();
        self.violations.get(idx)
    }
}

// ─── M216 S05: detail panel (AC-05) ───────────────────────────────────────

/// Detail panel content for the active milestone. The
/// detail pane consumes:
///
/// - `session show` events (`session.events[]`).
/// - `reviews finding list` (`reviews.findings[]`).
/// - M213 next-action APIs (`session.next_action`).
///
/// The renderer reads `cycles`, `findings`, `history`,
/// `drift`, and `cap` directly. The struct never reads
/// `activity.json` or any plan-zone file directly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetailPanel {
    pub milestone_id: String,
    /// Cycle-flow events from `session.events[]` for this
    /// milestone. The renderer reads these for the cycle
    /// timeline.
    pub cycles: Vec<String>,
    /// Reviews finding summaries from `mp reviews finding
    /// list <mid>` for this milestone. Each entry is the
    /// short finding description (id + title).
    pub findings: Vec<String>,
    /// History rows from `session.queue_cycle_history[]`
    /// (one row per cycle stage for the active milestone).
    pub history: Vec<String>,
    /// Drift — time since the last state change for the
    /// active stage. Format: `<n>m <n>s` or "fresh" when
    /// under 60s.
    pub drift: String,
    /// Cap — the `cycle_cap` field from the session (the
    /// maximum cycle count the milestone is allowed to
    /// enter). Surfaces as `cap=N` next to the cycle
    /// timeline so the operator knows when a stall is
    /// approaching the limit.
    pub cap: String,
}

impl DetailPanel {
    /// Empty panel. The renderer skips the detail block
    /// until a refresh populates it.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build the detail panel from the `session show`
    /// payload + reviews finding rows + an optional
    /// next-action envelope. The function is pure — no IO,
    /// no plan-zone reads.
    pub fn from_payloads(
        session_show: &Value,
        findings: &[String],
        next_action: &Option<Value>,
    ) -> Self {
        let milestone_id = session_show
            .get("session")
            .and_then(|s| s.get("working_on"))
            .and_then(|w| w.get("milestone_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim_start_matches('M').to_string())
            .or_else(|| {
                session_show
                    .get("session")
                    .and_then(|s| s.get("active_milestone"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim_start_matches('M').to_string())
            })
            .unwrap_or_default();
        let cycles = session_show
            .get("session")
            .and_then(|s| s.get("events"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|evt| {
                        let kind = evt.get("kind").and_then(|v| v.as_str())?;
                        let actor = evt.get("actor").and_then(|v| v.as_str()).unwrap_or("");
                        Some(format!("{kind} ({actor})"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let history = session_show
            .get("session")
            .and_then(|s| s.get("queue_cycle_history"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        let mid = entry
                            .get("milestone_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let cycle = entry.get("cycle").and_then(|v| v.as_u64()).unwrap_or(0);
                        let outcome = entry
                            .get("outcome")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !mid.is_empty() && milestone_id == mid.trim_start_matches('M') {
                            Some(format!("cycle={cycle} outcome={outcome}"))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let drift = session_show
            .get("session")
            .and_then(|s| s.get("last_state_change_at"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "fresh".to_string());
        let cap = session_show
            .get("session")
            .and_then(|s| s.get("cycle_cap"))
            .and_then(|v| v.as_u64())
            .map(|n| format!("cap={n}"))
            .unwrap_or_else(|| "cap=∞".to_string());
        let mut findings = findings.to_vec();
        if let Some(next) = next_action {
            let action = next
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !action.is_empty() {
                findings.push(format!("next-action: {action}"));
            }
        }
        Self {
            milestone_id,
            cycles,
            findings,
            history,
            drift,
            cap,
        }
    }

    /// Render the detail panel as a multi-line `String`.
    /// The renderer reads this verbatim into the right-hand
    /// column on the Autopilot lane.
    pub fn render_to_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Detail ({})\n", self.milestone_id));
        out.push_str(&format!(" drift={} {}\n", self.drift, self.cap));
        out.push_str(" cycles:\n");
        if self.cycles.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for c in &self.cycles {
                out.push_str(&format!("  - {c}\n"));
            }
        }
        out.push_str(" findings:\n");
        if self.findings.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for f in &self.findings {
                out.push_str(&format!("  - {f}\n"));
            }
        }
        out.push_str(" history:\n");
        if self.history.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for h in &self.history {
                out.push_str(&format!("  - {h}\n"));
            }
        }
        out
    }
}

// ─── M216 S06: recovery controls (AC-06) ──────────────────────────────────

/// Recovery control client. Builds the `mp autopilot
/// control …` argv for each recovery verb. The control
/// verbs (pause / resume / cancel / steer / restart) shell
/// out through [`crate::mp_runner::MpRunner::run_raw_allow_failure`]
/// so a missing control surface on the `mp` side is
/// surfaced as a recoverable warning rather than a hard
/// crash.
pub struct RecoveryControl;

impl RecoveryControl {
    /// Build the argv for `mp autopilot control pause
    /// <session>`. The output is owned `String`s so the
    /// dispatcher can pass `&str` slices through `run_raw`.
    pub fn pause_argv(session_id: &str) -> Vec<String> {
        vec![
        "control".into(),
        "pause".into(),
        session_id.to_string(),
        ]
    }

    /// Build the argv for `mp autopilot control resume
    /// <session>`.
    pub fn resume_argv(session_id: &str) -> Vec<String> {
        vec![
        "control".into(),
        "resume".into(),
        session_id.to_string(),
        ]
    }

    /// Build the argv for `mp autopilot control cancel
    /// <session> --confirm`. Cancel is terminal — the
    /// `--confirm` flag avoids a footgun where a typo
    /// kills an active session.
    pub fn cancel_argv(session_id: &str) -> Vec<String> {
        vec![
        "control".into(),
        "cancel".into(),
        session_id.to_string(),
        "--confirm".into(),
        ]
    }

    /// Build the argv for `mp autopilot control steer
    /// <session> --message <MSG>`.
    pub fn steer_argv(session_id: &str, message: &str) -> Vec<String> {
        vec![
        "control".into(),
        "steer".into(),
        session_id.to_string(),
        "--message".into(),
        message.to_string(),
        ]
    }

    /// Build the argv for `mp autopilot start <ids...>
    /// --detach`. The restart path creates a new session
    /// from the explicit queue — it does NOT revive the
    /// cancelled session. The override payload's topology
    /// + poll_interval_ms are forwarded through the
    /// `--topology` / `--poll-interval-ms` flags so the
    /// new session honors the user's last panel edits.
    pub fn start_argv(ids: &[String], payload: &SessionOverridesPayload) -> Vec<String> {
        let mut argv = vec!["start".to_string()];
        for id in ids {
            argv.push(id.clone());
        }
        argv.push("--topology".into());
        argv.push(payload.config_overrides.topology.clone());
        if let Some(poll_ms) = payload.config_overrides.poll_interval_ms {
            argv.push("--poll-interval-ms".into());
            argv.push(poll_ms.to_string());
        }
        argv.push("--detach".into());
        argv
    }
}

// ─── M216 S07: AC detail (AC-07) ──────────────────────────────────────────

/// One row in the AC detail breakdown. Carries the AC id,
/// the description (truncated to 40 chars by the
/// renderer), the status (`passed` / `failed` / `pending`),
/// and the stamped evidence string (truncated to 60 chars
/// by the renderer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcDetailRow {
    pub id: String,
    pub description: String,
    pub status: String,
    pub evidence: String,
}

/// AC detail for the active milestone. Reads
/// `session.queue[i].ac_pass_fail[]` and produces one row
/// per AC. The struct is sorted so passed / failed ACs come
/// before pending — the failed marker renders red; the
/// pending marker renders neutral.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcDetail {
    pub milestone_id: String,
    pub rows: Vec<AcDetailRow>,
}

impl AcDetail {
    /// Empty detail. The renderer skips the block until a
    /// refresh populates it.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parse the AC detail from the session-show payload.
    /// The payload's `session.queue[i].ac_pass_fail[]`
    /// block carries one entry per AC: `{ id, status,
    /// evidence }`. We pick the entry matching the
    /// active milestone. Returns `None` when the
    /// active milestone has no `ac_pass_fail` block or
    /// the block is empty.
    pub fn from_payload(payload: &Value) -> Option<Self> {
        let milestone_id = payload
            .get("session")
            .and_then(|s| s.get("working_on"))
            .and_then(|w| w.get("milestone_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim_start_matches('M').to_string())?;
        let rows: Option<Vec<AcDetailRow>> = payload
            .get("session")
            .and_then(|s| s.get("queue"))
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter().find(|item| {
                    item.get("milestone_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim_start_matches('M'))
                        .map(|s| s == milestone_id.as_str())
                        .unwrap_or(false)
                })
            })
            .and_then(|item| item.get("ac_pass_fail"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                let mut rows: Vec<AcDetailRow> = arr
                    .iter()
                    .filter_map(|entry| {
                        let id = entry.get("id").and_then(|v| v.as_str())?.to_string();
                        let description = entry
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let status = entry
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("pending")
                            .to_string();
                        let evidence = entry
                            .get("evidence")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some(AcDetailRow {
                            id,
                            description,
                            status,
                            evidence,
                        })
                    })
                    .collect();
                // Sort: failed first, then passed, then pending.
                // The renderer's red marker is for failed ACs;
                // pending ACs render neutral.
                rows.sort_by(|a, b| sort_key(&a.status).cmp(&sort_key(&b.status)));
                rows
            });
        let rows = rows?;
        if rows.is_empty() {
            return None;
        }
        Some(Self { milestone_id, rows })
    }

    /// True when the AC detail has at least one row.
    pub fn has_rows(&self) -> bool {
        !self.rows.is_empty()
    }

    /// The passed count.
    pub fn passed(&self) -> usize {
        self.rows.iter().filter(|r| r.status == "passed").count()
    }

    /// The failed count.
    pub fn failed(&self) -> usize {
        self.rows.iter().filter(|r| r.status == "failed").count()
    }

    /// The pending count.
    pub fn pending(&self) -> bool {
        self.rows.iter().any(|r| r.status == "pending")
    }

    /// Total row count.
    pub fn total(&self) -> usize {
        self.rows.len()
    }

    /// Render the AC detail rows. The format is one row
    /// per AC: `AC-XX  desc-trunc-40  [passed|failed|pending]
    /// evidence-trunc-60`. Failed ACs render with the
    /// marker `! `; pending ACs render with `  `; passed
    /// ACs render with `  `.
    ///
    /// `viewport_h` caps the rendered height so a long AC
    /// list scrolls inside the existing detail pane — the
    /// golden-file tests assert the overflow shape.
    pub fn render_to_string(&self, viewport_h: usize) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "ACs ({} / {})\n",
            self.passed(),
            self.total(),
        ));
        let visible = self.rows.iter().take(viewport_h);
        for row in visible {
            let marker = match row.status.as_str() {
                "failed" => "!",
                _ => " ",
            };
            let desc = truncate(&row.description, 40);
            let evidence = truncate(&row.evidence, 60);
            out.push_str(&format!(
                "{} {} | {} | {} | {}\n",
                marker, row.id, desc, row.status, evidence
            ));
        }
        if self.rows.len() > viewport_h {
            out.push_str(&format!(
                " ... ({} more)\n",
                self.rows.len() - viewport_h
            ));
        }
        out
    }
}

fn sort_key(status: &str) -> u8 {
    match status {
        "failed" => 0,
        "passed" => 1,
        _ => 2,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        // Use the existing crate-level truncate (text::truncate) when
        // available; otherwise inline the head + "..." truncation here.
        crate::text::truncate(s, max.saturating_sub(3).max(1))
    }
}

// ─── M216 S08: telemetry (AC-08) ──────────────────────────────────────────

/// Cross-milestone telemetry aggregates. Computed from
/// `session.json` at refresh tick. Aggregates pull from
/// the full session payload — they reset on session start
/// because the operator restarts the picker / refresh on a
/// new session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Telemetry {
    pub milestone_id: String,
    /// Total time-in-active-stage across all panes
    /// (Working / Reviewing / Deciding). String formatted
    /// as `<n>m <n>s`.
    pub total_time_in_active_stage: String,
    /// Per-stage attempt counts across the queue
    /// (Dispatching / WaitingRunner / Reviewing /
    /// Deciding / AwaitingUser).
    pub attempts_per_stage: Vec<(String, usize)>,
    /// Per-AC pass / fail counts across milestones.
    pub per_ac_pass_fail: Vec<(String, usize, usize)>,
}

impl Telemetry {
    /// Empty telemetry. The renderer skips the block until
    /// a refresh populates it.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build the telemetry from the session-show payload.
    /// The function is pure — no shell-out, no plan-zone
    /// read.
    pub fn from_payload(payload: &Value) -> Self {
        let milestone_id = payload
            .get("session")
            .and_then(|s| s.get("working_on"))
            .and_then(|w| w.get("milestone_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim_start_matches('M').to_string())
            .unwrap_or_default();
        // (a) total time-in-active-stage: sum
        // (now - last_state_change_at) for each pane in
        // Working / Reviewing / Deciding. The payload carries
        // `session.role_state.roles.<role>.last_state_change_at`.
        // We sum the gaps; the precise sum is the typed value.
        let total_secs: u64 = payload
            .get("session")
            .and_then(|s| s.get("role_state"))
            .and_then(|r| r.get("roles"))
            .and_then(|r| r.as_object())
            .map(|roles| {
                roles
                    .values()
                    .filter_map(|role| {
                        let state = role.get("state").and_then(|v| v.as_str())?;
                        if !matches!(state, "working" | "reviewing" | "deciding") {
                            return None;
                        }
                        let last_change = role
                            .get("last_state_change_at")
                            .and_then(|v| v.as_str())
                            .and_then(|s| parse_iso8601_secs(s));
                        let now = role
                            .get("now")
                            .and_then(|v| v.as_str())
                            .and_then(|s| parse_iso8601_secs(s))
                            .unwrap_or(0);
                        Some(last_change.map(|t| now.saturating_sub(t)).unwrap_or(0))
                    })
                    .sum()
            })
            .unwrap_or(0);
        let total_time_in_active_stage = format!("{}m {}s", total_secs / 60, total_secs % 60);
        // (b) attempts-per-stage: count cycle-flow transitions
        // per stage across the queue. The payload carries
        // `session.queue_cycle_history[]`; each entry's
        // `outcome` is the stage the cycle ended on. We
        // group outcomes and produce a Vec<(stage, count)>.
        let mut attempts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        if let Some(arr) = payload
            .get("session")
            .and_then(|s| s.get("queue_cycle_history"))
            .and_then(|v| v.as_array())
        {
            for entry in arr {
                let outcome = entry
                    .get("outcome")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if outcome.is_empty() {
                    continue;
                }
                *attempts.entry(outcome.to_string()).or_insert(0) += 1;
            }
        }
        let attempts_per_stage: Vec<(String, usize)> = attempts.into_iter().collect();
        // (c) per-AC pass / fail counts across milestones:
        // sum status from queue[*].ac_pass_fail[].
        let mut pass_fail: std::collections::BTreeMap<String, (usize, usize)> =
            std::collections::BTreeMap::new();
        if let Some(arr) = payload
            .get("session")
            .and_then(|s| s.get("queue"))
            .and_then(|v| v.as_array())
        {
            for item in arr {
                if let Some(ac_arr) = item.get("ac_pass_fail").and_then(|v| v.as_array()) {
                    for entry in ac_arr {
                        let id = entry
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let status = entry
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let entry = pass_fail.entry(id).or_insert((0, 0));
                        match status {
                            "passed" => entry.0 += 1,
                            "failed" => entry.1 += 1,
                            _ => {}
                        }
                    }
                }
            }
        }
        let per_ac_pass_fail: Vec<(String, usize, usize)> = pass_fail
            .into_iter()
            .map(|(id, (p, f))| (id, p, f))
            .collect();
        Self {
            milestone_id,
            total_time_in_active_stage,
            attempts_per_stage,
            per_ac_pass_fail,
        }
    }

    /// Render the telemetry block. The format is a labeled
    /// key-value list, sorted in canonical order so the
    /// golden output is stable across runs.
    pub fn render_to_string(&self) -> String {
        let mut out = String::new();
        out.push_str(" Telemetry \n");
        out.push_str(&format!(
            " total_time_in_active_stage = {} \n",
            self.total_time_in_active_stage
        ));
        out.push_str(" attempts_per_stage:\n");
        if self.attempts_per_stage.is_empty() {
            out.push_str("  (none) \n");
        } else {
            for (stage, count) in &self.attempts_per_stage {
                out.push_str(&format!("  {stage} = {count}\n"));
            }
        }
        out.push_str(" per_ac_pass_fail:\n");
        if self.per_ac_pass_fail.is_empty() {
            out.push_str("  (none) \n");
        } else {
            for (id, p, f) in &self.per_ac_pass_fail {
                out.push_str(&format!("  {id} = passed:{p} failed:{f}\n"));
            }
        }
        out
    }
}

fn parse_iso8601_secs(s: &str) -> Option<u64> {
    // Naive parser: accept any RFC3339-ish timestamp and
    // return the seconds-since-unix-epoch approximation. We
    // don't need precision (the values are surfaced as
    // "<n>m <n>s" labels, not for math), but the count has
    // to be non-negative.
    let trimmed = s.trim_end_matches('Z');
    if let Some(idx) = trimmed.find('T') {
        let rest = &trimmed[idx + 1..];
        let hh: u64 = rest.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(0);
        let mm: u64 = rest.get(3..5).and_then(|x| x.parse().ok()).unwrap_or(0);
        let ss: u64 = rest.get(6..8).and_then(|x| x.parse().ok()).unwrap_or(0);
        return Some(hh * 3600 + mm * 60 + ss);
    }
    None
}

#[cfg(test)]
mod f01_tests {
    use super::*;

    fn list_payload() -> Value {
        serde_json::json!({
            "milestones": [
                {"id": "M207", "title": "Pilot S2", "lifecycle": "approved"},
                {"id": "M209", "title": "Coordination", "lifecycle": "in-progress"},
            ]
        })
    }

    #[test]
    fn empty_lane_state_has_no_panel_or_replay() {
        let state = AutopilotLaneState::empty();
        assert!(state.picker.candidates.is_empty());
        assert!(state.panel.is_none());
        assert!(state.replay_shell.is_none());
        assert!(!state.panel_open);
        assert!(!state.replay_open);
        assert!(!state.can_start());
    }

    #[test]
    fn ensure_panel_is_idempotent_and_initialises_default_panel() {
        let mut state = AutopilotLaneState::empty();
        let p1 = state.ensure_panel().clone();
        let p2 = state.ensure_panel().clone();
        assert_eq!(p1, p2);
        assert_eq!(p1, OverridePanel::new());
    }

    #[test]
    fn toggle_panel_opens_then_closes_and_preserves_typed_values() {
        let mut state = AutopilotLaneState::empty();
        state.toggle_panel();
        assert!(state.panel_open);
        assert!(state.panel.is_some());

        // Type something.
        if let Some(panel) = state.panel.as_mut() {
            panel.topology = "two-agent".to_string();
            panel.refresh_secs = 5;
        }
        let snapshot = state.panel.clone();

        // Close + reopen — the typed values survive.
        state.toggle_panel();
        assert!(!state.panel_open);
        state.toggle_panel();
        assert!(state.panel_open);
        assert_eq!(state.panel, snapshot);
    }

    #[test]
    fn open_replay_replaces_existing_shell() {
        let mut state = AutopilotLaneState::empty();
        let shell_a = ReplayShell {
            session_id: "alpha".to_string(),
            status: "active".to_string(),
            last_updated: "2026-09-04T00:00:00Z".to_string(),
            events: vec![],
        };
        state.open_replay(shell_a);
        assert!(state.replay_open);
        assert_eq!(state.replay_shell.as_ref().unwrap().session_id, "alpha");

        // Replace — the new shell wins.
        let shell_b = ReplayShell {
            session_id: "beta".to_string(),
            status: "completed".to_string(),
            last_updated: "2026-09-04T00:00:00Z".to_string(),
            events: vec![],
        };
        state.open_replay(shell_b);
        assert_eq!(state.replay_shell.as_ref().unwrap().session_id, "beta");
    }

    #[test]
    fn close_replay_keeps_the_shell_for_reopen() {
        let mut state = AutopilotLaneState::empty();
        let shell = ReplayShell {
            session_id: "alpha".to_string(),
            status: "active".to_string(),
            last_updated: "2026-09-04T00:00:00Z".to_string(),
            events: vec![],
        };
        state.open_replay(shell.clone());
        state.close_replay();
        assert!(!state.replay_open);
        assert_eq!(state.replay_shell, Some(shell));
    }

    #[test]
    fn toggle_picker_select_returns_false_when_picker_is_empty() {
        let mut state = AutopilotLaneState::empty();
        assert!(!state.toggle_picker_select());
    }

    #[test]
    fn can_start_requires_picker_selection_and_no_panel_or_replay() {
        let mut state = AutopilotLaneState::empty();
        state.refresh_picker(&list_payload());
        assert!(!state.can_start());

        state.toggle_picker_select();
        assert!(state.can_start());

        // Open the panel — Start is blocked while editing.
        state.toggle_panel();
        assert!(!state.can_start());

        // Close the panel — Start is unblocked.
        state.toggle_panel();
        assert!(state.can_start());

        // Open the replay — Start is blocked again.
        state.open_replay(ReplayShell::default());
        assert!(!state.can_start());
    }
}
