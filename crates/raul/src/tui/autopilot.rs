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
}