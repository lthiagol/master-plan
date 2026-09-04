//! M215: Autopilot lane — picker, per-drive override panel, and
//! session replay shell.
//!
//! Three independent surfaces live in this module so the lane can
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
//! The structures are pure data — no App / Mode coupling, no
//! subprocess calls — so the test surface is fully isolated. The
//! production hot path (keybinds, lane wiring) lives in `action.rs`
//! and `keybinds.rs`; this module is the typed model.

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
    DRIVABLE_LIFECYCLES.iter().any(|l| *l == lifecycle)
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
        let mut panel = OverridePanel::default();
        panel.topology = "four-agent".to_string();
        let err = panel.validate().unwrap_err();
        assert!(matches!(err, OverrideError::UnknownTopology(_)));
    }

    #[test]
    fn override_panel_rejects_unknown_harness() {
        let mut panel = OverridePanel::default();
        panel
            .roles
            .entry("runner".to_string())
            .or_default()
            .harness = Some("claude-code".to_string());
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
        panel
            .roles
            .entry("runner".to_string())
            .or_default()
            .harness = Some(String::new());
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
        let mut panel = OverridePanel::default();
        panel.refresh_secs = 0;
        let err = panel.validate().unwrap_err();
        assert!(matches!(err, OverrideError::NonPositiveRefresh(0)));
    }

    #[test]
    fn override_panel_to_session_overrides_drops_empty_roles() {
        let mut panel = OverridePanel::default();
        // Only the runner gets an override; orchestrator/reviewer
        // stay inherited.
        panel
            .roles
            .entry("runner".to_string())
            .or_default()
            .model = Some("anthropic/claude-opus-4-1".to_string());
        panel
            .roles
            .entry("runner".to_string())
            .or_default()
            .harness = Some("opencode".to_string());
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
        panel
            .roles
            .entry("runner".to_string())
            .or_default()
            .extras = Some(r#"{"max_retries":3,"label":"r-1"}"#.to_string());
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
        if !ALLOWED_TOPOLOGIES.iter().any(|t| *t == self.topology) {
            return Err(OverrideError::UnknownTopology(self.topology.clone()));
        }
        for (role, ovr) in &self.roles {
            if let Some(h) = ovr.harness.as_deref() {
                if !h.is_empty() && !ALLOWED_HARNESSES.iter().any(|x| *x == h) {
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
                    let parsed: Value =
                        serde_json::from_str(extras).unwrap_or(Value::Null);
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
    let value: Value = serde_json::from_str(extras)
        .map_err(|e| format!("not valid JSON: {e}"))?;
    if !value.is_object() {
        return Err("extras must be a JSON object".to_string());
    }
    Ok(())
}