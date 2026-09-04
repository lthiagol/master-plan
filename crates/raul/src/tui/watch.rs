//! M179 S3 / AC-02: Watch client model + M178 adapter.
//!
//! The Watch lane is the dedicated surface for the `mp watch`
//! workflow. This module owns:
//!
//! - The picker state: which milestones are eligible (mp-watch-
//!   drivable lifecycle from `mp_model::WATCH_DRIVABLE_LIFECYCLES`:
//!   `approved`, `in-progress`, `remediation`), the user's selection
//!   order, and the pending dry-run diagnostics.
//! - The M178 adapter: thin wrappers around the M178 `watch-control`
//!   subcommands (`status`, `stop`, `output`, `result`) plus a
//!   `mp watch --detach` invocation helper.
//! - Selection mutators: `toggle_select(id)`, `clear_selection()`,
//!   `set_selection(ids)`. Selection preserves insertion order so
//!   the queue passed to `mp watch` matches what the user picked.
//! - Dry-run preflight (S4) and start (S5) entry points are stubs
//!   here — S3 only lands the model + selection. Subsequent steps
//!   wire the dry-run and start as the M178 adapter gains surface.
//!
//! The `Watch` struct is the single source of truth for picker
//! state; `App::watch` (in `tui::app`) is the only field. No other
//! module reaches into picker state directly.

use serde::Serialize;
use serde_json::Value;

use crate::tui::app::App;
use anyhow::Result;

/// Lifecycle states that `mp watch` will drive. Sourced from
/// `mp_model::WATCH_DRIVABLE_LIFECYCLES` so Raul does not own a parallel
/// allowlist. Review aliases are excluded — the reviews registry owns
/// those rungs.
pub const DRIVABLE_LIFECYCLES: &[&str] = mp_model::WATCH_DRIVABLE_LIFECYCLES;

/// True when a milestone's lifecycle is one mp watch will drive.
pub fn is_drivable_lifecycle(lifecycle: &str) -> bool {
    mp_model::is_watch_drivable_lifecycle(lifecycle)
}

/// M179 S3 / AC-02: the Watch client model. Holds the picker state
/// (eligible + selected + dry-run diagnostics) and the M178 adapter
/// inputs (the preflight + start / stop / output / result verbs).
///
/// PartialEq is implemented but `Eq` is not (the model carries
/// `serde_json::Value` snapshots in `WatchStatus::raw` and the
/// `OutputSnapshot::output` strings are not necessarily canonical).
/// The picker-level invariants (candidate / selected / cursor
/// consistency) are enforced by the mutator methods, not by
/// `Eq` reflection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Watch {
    /// All drivable milestones loaded from `mp list milestones` on
    /// the last picker refresh. Preserved in `mp list` order
    /// (currently the canonical M122 sort). The picker renders
    /// these as the candidate list.
    pub candidates: Vec<WatchCandidate>,
    /// Ordered, user-selected milestone ids. The order is the
    /// queue order passed to `mp watch` on Start. `selected` is
    /// always a subset of `candidates` (filtered by id).
    pub selected: Vec<String>,
    /// Last dry-run diagnostics (S4). `Ok(())` on a clean
    /// preflight; `Err(reason)` surfaces the per-milestone failure
    /// that blocked Start. `None` means no dry-run has been run
    /// yet (Start is disabled in that state).
    pub preflight: Option<PreflightResult>,
    /// Latest M178 control-plane snapshot (S4). `None` before
    /// the first `mp watch-control status` call.
    pub status: Option<WatchStatus>,
    /// Latest active-pane output (S7). `None` before the first
    /// `mp watch-control output` call.
    pub output: Option<OutputSnapshot>,
    /// Bounded Watch log tail refreshed by the idle poller. Renderers only
    /// consume this in-memory snapshot and never perform filesystem I/O.
    pub log_tail: Vec<String>,
    /// Cursor inside the picker list (0..=candidates.len()).
    /// Independent of `App::selected_index` (the legacy list
    /// cursor) so the Watch lane's selection model is fully
    /// isolated.
    pub picker_index: usize,
    /// Cursor inside the ordered queue (the selection list). Only
    /// meaningful when `selected.len() > 0`.
    pub queue_index: usize,
    /// Last error surfaced by the picker (e.g. shell-out
    /// failure, schema mismatch). Cleared on the next successful
    /// picker refresh.
    pub last_error: Option<String>,
}

/// One row in the Watch picker. A subset of `MilestoneSummary` —
/// only the fields the picker renders. `id` is the canonical
/// milestone id (no `M` prefix), matching the S178 contract.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WatchCandidate {
    pub id: String,
    pub title: String,
    pub lifecycle: String,
    /// The picker surfaces `priority` (urgent/high/normal/low) so
    /// the user can sort by it. `None` for milestones that have no
    /// priority assigned (older format).
    pub priority: Option<String>,
    /// The candidate's current dependency status surfaced by
    /// dry-run; `None` until preflight has run.
    pub dep_status: Option<DepStatus>,
}

/// Per-milestone dependency / configuration check from
/// `mp watch --dry-run`. The Watch picker surfaces one row per
/// candidate, with the dry-run verdict on the same row.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DepStatus {
    /// The candidate's lifecycle, spec_status, execution_status
    /// passed the dry-run gate.
    Ready,
    /// A specific reason the dry-run gate rejected the candidate.
    /// The string carries the human-readable reason from
    /// `mp watch --dry-run` (e.g., "spec_status=verified, not ready").
    Blocked(String),
    /// The candidate was excluded by the picker's own
    /// `is_drivable_lifecycle` filter — not a dry-run verdict.
    Excluded(String),
}

/// Result of the most recent `mp watch --dry-run` invocation.
/// `Ok(())` on a clean preflight (Start is enabled); `Err(reason)`
/// on a failed preflight (Start is disabled; the per-milestone
/// verdicts are stored in `candidates[i].dep_status`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PreflightResult {
    /// Exact ordered queue validated by this dry-run.
    pub queue_fingerprint: Vec<String>,
    /// Per-milestone verdicts keyed by id. The same id set as
    /// `candidates` (filtered by `is_drivable_lifecycle`).
    pub per_milestone: Vec<(String, DepStatus)>,
    /// Aggregate precondition gate (herdr_on_path, harness
    /// configs, etc.). The dry-run JSON's top-level preconditions
    /// block; surfaced verbatim for the user.
    pub aggregate_ok: bool,
    /// Cached `mp watch --dry-run` JSON so the renderer can
    /// surface dependency/configuration diagnostics without a
    /// second subprocess call. `None` if the user has never run
    /// preflight on this picker.
    pub raw: Option<Value>,
    /// The exit/result verdict the dry-run reported. `Ok(())`
    /// when every drivable candidate is `Ready` and the
    /// aggregate preconditions passed. `Err(reason)` when any
    /// candidate is `Blocked` or any aggregate check failed.
    pub verdict: Result<(), String>,
}

/// M178 S4: a snapshot of `mp watch-control status`. Carries the
/// classification (live / stale / terminal) plus the v2 state
/// payload so the renderer can show "what mp thinks" verbatim
/// without re-shelling.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WatchStatus {
    pub kind: String,
    pub reason: Option<String>,
    pub pid_alive: bool,
    pub herdr_listed: bool,
    pub state_file: String,
    pub raw: Value,
    /// The latest run's `run_outcome` (`Some` when terminal,
    /// `None` for live / stale). Surfaced verbatim — M179 AC-10
    /// forbids re-interpreting mp outcomes.
    pub run_outcome: Option<Value>,
    /// Per-milestone outcome log from the v2 state. The queue
    /// renderer reads this for the per-row outcome labels.
    pub milestone_outcomes: Vec<Value>,
}

/// M178 S7: a snapshot of the active-pane output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OutputSnapshot {
    pub role: String,
    pub pane_id: Option<String>,
    pub output: String,
    pub truncated: bool,
    pub bytes: usize,
    pub elapsed_ms: u64,
    pub reason: String,
    pub ok: bool,
}

impl Watch {
    /// Empty picker state. Used by `App::new()` and on
    /// `clear_selection()` to reset the model without losing the
    /// candidate list.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Filter a `mp list milestones` payload to the drivable
    /// subset, preserving the canonical list order. Pure: no
    /// subprocess call, no IO.
    pub fn filter_candidates(list_payload: &Value) -> Vec<WatchCandidate> {
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
                if !is_drivable_lifecycle(&lifecycle) {
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
                Some(WatchCandidate {
                    id,
                    title,
                    lifecycle,
                    priority,
                    dep_status: None,
                })
            })
            .collect()
    }

    /// Replace the candidate list (e.g. after a picker refresh)
    /// and drop any selected ids that are no longer in the
    /// candidate set. Preserves the relative order of the
    /// surviving selection.
    pub fn refresh_candidates(&mut self, list_payload: &Value) {
        let old_selected = self.selected.clone();
        self.candidates = Self::filter_candidates(list_payload);
        let new_ids: std::collections::HashSet<String> =
            self.candidates.iter().map(|c| c.id.clone()).collect();
        self.selected.retain(|id| new_ids.contains(id));
        if self.selected.is_empty() {
            self.queue_index = 0;
        } else if self.queue_index >= self.selected.len() {
            self.queue_index = self.selected.len() - 1;
        }
        if self.picker_index >= self.candidates.len() {
            self.picker_index = self.candidates.len().saturating_sub(1);
        }
        if self.selected != old_selected {
            self.invalidate_queue();
        } else if let Some(preflight) = &self.preflight {
            for (id, verdict) in &preflight.per_milestone {
                if let Some(candidate) = self.candidates.iter_mut().find(|c| c.id == *id) {
                    candidate.dep_status = Some(verdict.clone());
                }
            }
        }
    }

    /// Toggle a candidate's selection. No-op when the id is
    /// not in the candidate set. Preserves insertion order: the
    /// selected list grows by append, shrinks by remove at the
    /// matching index.
    pub fn toggle_select(&mut self, id: &str) {
        let before = self.selected.clone();
        if let Some(idx) = self.candidates.iter().position(|c| c.id == id) {
            let id_owned = self.candidates[idx].id.clone();
            if let Some(pos) = self.selected.iter().position(|s| s == &id_owned) {
                self.selected.remove(pos);
                if self.queue_index >= self.selected.len() {
                    self.queue_index = self.selected.len().saturating_sub(1);
                }
            } else {
                self.selected.push(id_owned);
                self.queue_index = self.selected.len() - 1;
            }
        }
        if self.selected != before {
            self.invalidate_queue();
        }
    }

    /// Replace the selection with a fixed ordered list. Used by
    /// external code that wants to restore a saved selection
    /// (S11 — terminal-state restoration).
    pub fn set_selection(&mut self, ids: Vec<String>) {
        let known: std::collections::HashSet<String> =
            self.candidates.iter().map(|c| c.id.clone()).collect();
        self.selected = ids.into_iter().filter(|id| known.contains(id)).collect();
        self.queue_index = 0;
        self.invalidate_queue();
    }

    /// Drop all selected ids.
    pub fn clear_selection(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.selected.clear();
        self.queue_index = 0;
        self.invalidate_queue();
    }

    /// Move the picker cursor within the candidate list, clamped
    /// to bounds. No-op when the candidate list is empty.
    pub fn move_picker(&mut self, delta: i64) {
        if self.candidates.is_empty() {
            return;
        }
        let len = self.candidates.len() as i64;
        let cur = self.picker_index as i64;
        let next = (cur + delta).rem_euclid(len);
        self.picker_index = next as usize;
    }

    /// Move the queue cursor within the selected list, clamped
    /// to bounds. No-op when the selection is empty.
    pub fn move_queue(&mut self, delta: i64) {
        if self.selected.is_empty() {
            return;
        }
        let len = self.selected.len() as i64;
        let cur = self.queue_index as i64;
        let next = (cur + delta).rem_euclid(len);
        self.queue_index = next as usize;
    }

    /// True when a dry-run has succeeded and Start is permitted.
    /// AC-03: a failed preflight cannot spawn mp watch or herdr.
    pub fn can_start(&self) -> bool {
        matches!(&self.preflight, Some(p)
            if p.verdict.is_ok()
                && !self.selected.is_empty()
                && p.queue_fingerprint == self.selected)
    }

    /// Read-only view of the current picker cursor.
    pub fn picker_candidate(&self) -> Option<&WatchCandidate> {
        self.candidates.get(self.picker_index)
    }

    /// Read-only view of the current queue cursor.
    pub fn queue_id(&self) -> Option<&str> {
        self.selected.get(self.queue_index).map(String::as_str)
    }

    /// Builder: the ordered queue ids that will be passed to
    /// `mp watch --detach` on Start. Mirrors `selected` exactly.
    pub fn queue_ids(&self) -> &[String] {
        &self.selected
    }

    fn invalidate_queue(&mut self) {
        self.preflight = None;
        for candidate in &mut self.candidates {
            candidate.dep_status = None;
        }
    }
}

// M179 S3: pure parsers for the M178 `watch-control` JSON
// envelopes. These are small and live next to the model so
// downstream steps (S4 / S7) can call them without reaching
// into the read layer.

/// Parse `mp watch-control status` JSON. The status envelope has
/// `run_state.kind` and (for non-live) `run_state.reason`. The
/// `state` field carries the v2 control-plane state payload
/// (None when no state file exists).
pub fn parse_status(payload: &Value) -> WatchStatus {
    let run_state = payload.get("run_state");
    let kind = run_state
        .and_then(|r| r.get("kind"))
        .and_then(|k| k.as_str())
        .unwrap_or("unknown")
        .to_string();
    let reason = run_state
        .and_then(|r| r.get("reason"))
        .and_then(|k| k.as_str())
        .map(|s| s.to_string());
    let pid_alive = payload
        .get("pid_alive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let herdr_listed = payload
        .get("herdr_listed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let state_file = payload
        .get("state_file")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let state = payload.get("state");
    // `run_outcome` is `Some(json)` only when the recorded state
    // carries a terminal outcome (e.g., `{"kind": "completed"}`).
    // A live / stale run leaves the field as `null`; the parser
    // stores `None` so callers can distinguish "no terminal
    // record yet" from "terminal record present but null" by
    // checking the JSON shape directly when needed.
    let run_outcome = state.and_then(|s| s.get("run_outcome")).and_then(|v| {
        if v.is_null() {
            None
        } else {
            Some(v.clone())
        }
    });
    // Normalize outcome ids the same way restore strips the M-prefix
    // from queue ids, so compact-queue matching stays consistent when
    // the driver persisted `M02` while candidates/selection use `02`.
    let milestone_outcomes = state
        .and_then(|s| s.get("milestone_outcomes"))
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .map(|entry| {
                    let mut normalized = entry.clone();
                    if let Some(id) = normalized.get("id").and_then(Value::as_str) {
                        normalized["id"] = Value::String(id.trim_start_matches('M').to_string());
                    }
                    normalized
                })
                .collect()
        })
        .unwrap_or_default();
    WatchStatus {
        kind,
        reason,
        pid_alive,
        herdr_listed,
        state_file,
        raw: payload.clone(),
        run_outcome,
        milestone_outcomes,
    }
}

/// Parse `mp watch-control output` JSON. The output envelope has
/// `ok` (bool) and (on success) `output` (the bounded pane text).
/// Failures carry `reason` instead.
pub fn parse_output(payload: &Value) -> OutputSnapshot {
    OutputSnapshot {
        ok: payload.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
        reason: payload
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        role: payload
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        pane_id: payload
            .get("pane_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        output: payload
            .get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        truncated: payload
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        bytes: payload.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        elapsed_ms: payload
            .get("elapsed_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    }
}

/// Parse `mp watch-control stop` JSON. The stop envelope has
/// `stopped` (bool), `pid` (`Option<u32>`), and a human-readable
/// `message`. The full payload is preserved for the renderer's
/// modal.
pub fn parse_stop(payload: &Value) -> StopReport {
    StopReport {
        stopped: payload
            .get("stopped")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        pid: payload
            .get("pid")
            .and_then(|v| v.as_u64())
            .map(|p| p as u32),
        elapsed_secs: payload
            .get("elapsed_secs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        timeout_secs: payload
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        message: payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        raw: payload.clone(),
    }
}

/// M178 S5 (stub): the response envelope from
/// `mp watch --detach <ids...>`. Surfaced verbatim by S3 so the
/// Watch module owns the shape; S5 will fill in the actual
/// invocation.
#[derive(Debug, Clone, Serialize)]
pub struct DetachReport {
    pub dry_run: bool,
    pub detach: bool,
    pub detached_pid: Option<u32>,
    pub log_file: String,
    pub state_file: String,
    pub preconditions_ok: bool,
    pub message: String,
    pub raw: Value,
}

/// M178 S4 (stub): the stop verb's parsed response.
#[derive(Debug, Clone, Serialize)]
pub struct StopReport {
    pub stopped: bool,
    pub pid: Option<u32>,
    pub elapsed_secs: f64,
    pub timeout_secs: u64,
    pub message: String,
    pub raw: Value,
}

// ─── S4 / S5: dry-run + start ─────────────────────────────────────

/// M179 S4: shell out to `mp watch --dry-run <ids...>` and parse
/// the structured response into the picker's `PreflightResult`.
/// M179 AC-03: a failed preflight cannot spawn mp watch or herdr
/// agents; `can_start()` stays false until the verdict flips to
/// `Ok(())`. Returns `Ok(())` and stamps `app.watch.preflight`
/// regardless of the verdict; callers should consult
/// `Watch::can_start` rather than this function's return value.
///
/// `ids` is the ordered queue the user picked. Empty `ids` is a
/// no-op (the function returns `Ok(())` without shelling out);
/// the caller should already have gated on `!selected.is_empty()`
/// before invoking.
pub fn run_preflight(runner: &crate::mp_runner::MpRunner, app: &mut App) -> Result<()> {
    let ids = app.watch.queue_ids().to_vec();
    if ids.is_empty() {
        app.watch.preflight = Some(PreflightResult {
            queue_fingerprint: vec![],
            per_milestone: vec![],
            aggregate_ok: false,
            raw: None,
            verdict: Err("empty queue".to_string()),
        });
        return Ok(());
    }
    // Build argv WITHOUT a leading "watch" — the runner's
    // first arg is the subcommand, and `ids...` is the rest.
    // F-01: previously `vec!["watch".to_string()]` produced
    // `mp watch watch <id>...` (a doubled subcommand), which
    // silently corrupted the persisted queue.
    let mut args: Vec<String> = Vec::new();
    for id in &ids {
        args.push(id.clone());
    }
    args.push("--dry-run".to_string());
    args.push("--format".to_string());
    args.push("json".to_string());
    let payload = match runner.run_raw_allow_failure(
        "watch",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    ) {
        Ok(stdout) => serde_json::from_slice(&stdout).unwrap_or(Value::Null),
        Err(e) => {
            app.watch.last_error = Some(format!("{e:#}"));
            serde_json::json!({ "dry_run": true, "error": format!("{e:#}") })
        }
    };
    let mut preflight = parse_preflight(&payload);
    preflight.queue_fingerprint = ids;
    app.watch.preflight = Some(preflight);
    // M179 S4: stamp per-candidate verdicts so the renderer
    // can show "M170 ✗ spec_status mismatch" without a second
    // subprocess call.
    apply_preflight_to_candidates(app);
    Ok(())
}

/// M179 S4: parse the `mp watch --dry-run` JSON into a
/// `PreflightResult`. Pure: no subprocess call. Public so the
/// tui_state tests can drive it without a runner.
pub fn parse_preflight(payload: &Value) -> PreflightResult {
    // `mp watch --dry-run` JSON shape (M149 / M178):
    //   {
    //     "dry_run": true,
    //     "log_file": "...",
    //     "preconditions": { "ok": bool, "checks": [...] },
    //     "milestones": [
    //       { "id": "M170", "input": "170", "title": "...",
    //         "lifecycle": "approved", "spec_status": "ready",
    //         "execution_status": "planned", "blocked": false,
    //         "ready": true|false, "next_action": "execute|skip_*",
    //         "stage": "...", "target_lifecycle": "...",
    //         "herdr_commands": [...], "prompt_preview": "...",
    //         "prompt_source": "...", "override_diagnostics": [...],
    //         "error": "..." or null
    //       }
    //     ]
    //   }
    let aggregate_ok = payload
        .get("preconditions")
        .and_then(|p| p.get("ok"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut per_milestone: Vec<(String, DepStatus)> = Vec::new();
    if let Some(rows) = payload.get("milestones").and_then(|m| m.as_array()) {
        for row in rows {
            let id = row
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim_start_matches('M').to_string())
                .unwrap_or_default();
            let ready = row.get("ready").and_then(|v| v.as_bool()).unwrap_or(false);
            // F-05: consult the documented `blocked` field. The
            // docstring lists it as part of the M178 dry-run shape;
            // the previous implementation ignored it, so a candidate
            // with `ready: true, blocked: true` (informational block
            // alongside a passing gate) would have been classified
            // Ready and Start would have been permitted. Now any
            // explicit `blocked: true` overrides `ready`.
            let blocked = row
                .get("blocked")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let err = row
                .get("error")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_default();
            let dep_status = if !err.is_empty() {
                DepStatus::Blocked(err)
            } else if blocked {
                // M178 surfaces `blocked: true` independently of
                // `error`; surface a deterministic reason so the
                // renderer can show "M170 ✗ blocked" without a
                // second subprocess call.
                DepStatus::Blocked(format!(
                    "milestone {id} is blocked (mp watch --dry-run reported blocked=true)"
                ))
            } else if !ready {
                // M178 surfaces `ready: false` for unready
                // milestones; the picker's `is_drivable_lifecycle`
                // already guarantees lifecycle is drivable, so a
                // `ready: false` here means a downstream gate
                // (e.g., spec_status mismatch). The dry-run
                // surfaces a generic reason; S5 can refine.
                DepStatus::Blocked(format!(
                    "milestone {id} is not ready (spec_status / execution_status mismatch)"
                ))
            } else {
                DepStatus::Ready
            };
            per_milestone.push((id, dep_status));
        }
    }

    // Per-milestone verdicts propagate into the picker's
    // candidate rows so S6's compact queue view can surface
    // "M170 ✗ spec_status mismatch" without a second subprocess
    // call. The model rebuilds the per-candidate dep_status
    // mapping from the verdict list.
    let verdict = if aggregate_ok
        && per_milestone
            .iter()
            .all(|(_, s)| matches!(s, DepStatus::Ready))
    {
        Ok(())
    } else if aggregate_ok {
        // Aggregate preconditions pass but per-milestone
        // verdicts are mixed → partial. Surface as Err with a
        // per-milestone summary so the renderer can show the
        // failed rows.
        let bad: Vec<String> = per_milestone
            .iter()
            .filter_map(|(id, s)| match s {
                DepStatus::Blocked(reason) => Some(format!("{id}: {reason}")),
                _ => None,
            })
            .collect();
        Err(bad.join("; "))
    } else {
        Err("aggregate preconditions failed".to_string())
    };

    PreflightResult {
        queue_fingerprint: per_milestone.iter().map(|(id, _)| id.clone()).collect(),
        per_milestone: per_milestone.clone(),
        aggregate_ok,
        raw: Some(payload.clone()),
        verdict,
    }
}

/// M179 S4: stamp the per-milestone verdicts from a
/// `PreflightResult` back onto the picker's candidate rows.
/// Called by `App::run_preflight` after `parse_preflight`
/// succeeds. The renderer reads `candidates[i].dep_status` to
/// surface the row verdict (Ready / Blocked(reason)) without a
/// second subprocess call.
///
/// Pure: no subprocess; the caller is responsible for the
/// preflight call.
pub fn apply_preflight_to_candidates(app: &mut App) {
    let Some(preflight) = app.watch.preflight.clone() else {
        return;
    };
    for (id, verdict) in preflight.per_milestone {
        if let Some(c) = app.watch.candidates.iter_mut().find(|c| c.id == id) {
            c.dep_status = Some(verdict);
        }
    }
}

// ─── S5: detach-safe start ────────────────────────────────────────

/// M179 S5 / AC-04: launch the validated queue through
/// `mp watch --detach <ids...>` so the run continues when the
/// user switches lanes. M179 AC-10: the run keeps the exact
/// queue order the user picked. The persisted detached PID is
/// captured into `app.watch.status` (parsed by `parse_status`).
/// M178 also writes the v2 state file before forking, so
/// `parse_status` is the canonical read after a successful start.
///
/// Returns the parsed `DetachReport` so the caller can surface
/// the detached PID in the lane footer. Errors propagate via
/// the `Result` return — the caller surfaces them via
/// `set_action_error`.
///
/// M179 AC-04: a failed preflight or empty queue refuses to
/// spawn. The function short-circuits with `Ok(None)` and
/// stamps `last_error` rather than shelling out.
pub fn start_watch(
    runner: &crate::mp_runner::MpRunner,
    app: &mut App,
) -> Result<Option<DetachReport>> {
    if !app.watch.can_start() {
        app.watch.last_error = Some("cannot start: preflight failed or queue is empty".to_string());
        return Ok(None);
    }
    // F-05: a live run is already attached; the user must
    // monitor, not start a second driver. (M178's double-spawn
    // guard does not catch the detach path.)
    if has_live_run(app) {
        app.watch.last_error =
            Some("a run is already live; monitor instead of starting".to_string());
        return Ok(None);
    }
    // F-01: build argv WITHOUT a leading "watch" — the runner's
    // first arg is the subcommand, and `ids...` follows.
    let mut args: Vec<String> = Vec::new();
    for id in app.watch.queue_ids() {
        args.push(id.clone());
    }
    args.push("--detach".to_string());
    args.push("--format".to_string());
    args.push("json".to_string());
    let payload = match runner.run_raw_allow_failure(
        "watch",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    ) {
        Ok(stdout) => serde_json::from_slice(&stdout).unwrap_or(Value::Null),
        Err(e) => {
            app.watch.last_error = Some(format!("{e:#}"));
            app.touch();
            return Err(e);
        }
    };
    let report = parse_detach_report(&payload);
    if let Some(pid) = report.detached_pid {
        // M179 S5: the state file is persisted by M178 itself;
        // re-read it through `parse_status` so the renderer's
        // first `status` call after Start shows the live run.
        let status_payload = runner
            .run("watch-control", &["status", "--format", "json"])
            .unwrap_or(Value::Null);
        app.watch.status = Some(crate::tui::watch::parse_status(&status_payload));
        let _ = pid; // recorded in status.raw
    } else {
        app.watch.last_error = Some(report.message.clone());
    }
    Ok(Some(report))
}

/// M179 S5: parse the `mp watch --detach` JSON envelope into a
/// `DetachReport`. Pure: no subprocess call.
pub fn parse_detach_report(payload: &Value) -> DetachReport {
    DetachReport {
        dry_run: payload
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        detach: payload
            .get("detach")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        detached_pid: payload
            .get("detached_pid")
            .and_then(|v| v.as_u64())
            .map(|p| p as u32),
        log_file: payload
            .get("log_file")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        state_file: payload
            .get("state_file")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        preconditions_ok: payload
            .get("preconditions")
            .and_then(|p| p.get("ok"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        message: payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        raw: payload.clone(),
    }
}

/// M179 S5 / AC-04: idempotency guard. Calling `start_watch`
/// while a run is already live returns the existing `WatchStatus`
/// rather than spawning a second driver. The check uses the
/// recorded PID (alive) + the v2 control-plane file presence
/// (M178's `--detach` always writes the file). Returns `true`
/// when a live run is already attached and the user should
/// monitor instead of starting a new one.
///
/// F-06: a "stale" run with an alive PID (zombie, herdr orphaned,
/// system clock skew) also blocks Start. The previous guard only
/// matched `kind == "live"`, so a stale-but-alive PID would let
/// `start_watch` proceed and spawn a duplicate driver — exactly
/// the gap this guard is supposed to close (the comment at
/// `start_watch` says M178's double-spawn guard does not catch
/// the detach path). Any `pid_alive == true` blocks Start,
/// regardless of kind.
pub fn has_live_run(app: &App) -> bool {
    if let Some(status) = &app.watch.status {
        status.pid_alive
    } else {
        false
    }
}

// ─── S6: ASCII lifecycle graph ───────────────────────────────────

/// M179 S6 / AC-05: the canonical milestone lifecycle the
/// ASCII graph renders. Order matters — `next_stage` walks
/// these in the same sequence, and the graph's "current
/// lifecycle" highlight reads this list to find the active
/// node.
pub const LIFECYCLE_NODES: &[&str] = &[
    "draft",
    "groomed",
    "approved",
    "in-progress",
    "self-reviewed",
    "reviewed",
    "complete",
    "cancelled",
    "remediation",
];

/// M179 S6: build a single-line ASCII graph of the canonical
/// lifecycle, highlighting the current lifecycle (if known).
/// The graph is intentionally compact (one line) so the Watch
/// lane can render it alongside the queue without eating the
/// whole terminal. The remediation node renders with a `↺`
/// suffix to signal the loop-back (the runner routes reviewed
/// milestones with open findings back to remediation).
///
/// F-09: only the active node is bracketed with `>...<`;
/// inactive nodes render as bare labels joined by `-`. The
/// previous implementation appended `<` to every node, which
/// produced `>draft<-groomed<-approved<-...` — noise on every
/// inactive node and no visual distinction for the active one.
pub fn render_lifecycle_graph(current: Option<&str>) -> String {
    let active = current.unwrap_or("");
    let parts: Vec<String> = LIFECYCLE_NODES
        .iter()
        .map(|node| {
            let label = match *node {
                "self-reviewed" => "self-rev",
                "in-progress" => "in-prog",
                n => n,
            };
            if *node == active {
                format!(">{label}<")
            } else {
                label.to_string()
            }
        })
        .collect();
    let mut out = parts.join("-");
    if active == "remediation" {
        out.push_str(" ↺");
    }
    out
}

/// M179 S6: build a one-row compact queue summary for the
/// Watch lane. Reads `app.watch.selected` + the latest
/// `WatchStatus` snapshot and emits one ASCII row per queued
/// milestone, with the per-milestone outcome (the mp-reported
/// `kind` verbatim) when known, or `pending` while the run
/// is still live.
///
/// AC-10: outcomes are surfaced exactly as reported by the mp
/// watch control/status contract. The renderer does not
/// reinterpret the `kind` string — what mp says, the queue
/// shows.
pub fn render_compact_queue(app: &App) -> String {
    if app.watch.selected.is_empty() {
        return "(empty queue — select one or more drivable milestones)".to_string();
    }
    let outcomes = app
        .watch
        .status
        .as_ref()
        .map(|s| &s.milestone_outcomes)
        .cloned()
        .unwrap_or_default();
    let mut out = String::new();
    for (i, id) in app.watch.selected.iter().enumerate() {
        let prefix = if i == app.watch.queue_index { ">" } else { " " };
        // Pass the mp `kind` string verbatim. F-06 remediation:
        // the previous version collapsed `completed → done`,
        // `partial-failure → failed`, `gracefully-stopped → stopped`
        // — those renames violated AC-10.
        let status = outcomes
            .iter()
            .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
            .and_then(|m| m.get("outcome"))
            .and_then(|o| o.get("kind"))
            .and_then(|k| k.as_str())
            .unwrap_or("pending");
        out.push_str(&format!("{prefix}[{status}] {id}\n"));
    }
    out
}

// ─── S7: non-blocking poller ─────────────────────────────────────

// ─── S7 → M217 cutover ───────────────────────────────────────────
//
// M179's fixed-interval `Poller` + `poll_watch_state` lived here.
// M217 replaced them with the single coalescing, focus-gated
// poller in `crate::tui::poll`, which is the lane's only
// scheduler: it owns the cadence (session override > config >
// 2s default), the single-flight lock, and the state diff that
// suppresses no-op redraws.
//
// Deliberately not re-exported under the old names — a second
// scheduler is the failure mode this cutover exists to prevent,
// so the legacy entry points are gone rather than deprecated.
// `tail_watch_log` below survives as a *pull* helper for the
// renderer's log pane; nothing schedules it.

/// M179 S8: shell out to `mp watch-control output` and update
/// `app.watch.output`. Bounded: `max_bytes` (default 4096),
/// `timeout_ms` (default 5000). Returns the parsed snapshot
/// for the caller to surface in the lane footer. M179 AC-07:
/// role switches are automatic via `poll_watch_state`; the
/// standalone fetch is the "force refresh" entry point.
pub fn fetch_active_output(
    runner: &crate::mp_runner::MpRunner,
    app: &mut App,
    max_bytes: usize,
    timeout_ms: u64,
) -> Result<()> {
    let mut args: Vec<String> = vec![
        "output".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    args.push("--max-bytes".to_string());
    args.push(max_bytes.to_string());
    args.push("--timeout-ms".to_string());
    args.push(timeout_ms.to_string());
    let payload = runner
        .run(
            "watch-control",
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .unwrap_or(Value::Null);
    let next = Some(parse_output(&payload));
    if app.watch.output != next {
        app.watch.output = next;
        app.touch();
    }
    Ok(())
}

/// Read a bounded tail of `<plan_dir>/.mp/watch.log` for the polling cache.
/// At most 64 KiB is read, regardless of total log size.
pub fn tail_watch_log(plan_dir: &std::path::Path, max_lines: usize) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};

    const MAX_TAIL_BYTES: u64 = 64 * 1024;
    let path = plan_dir.join(".mp").join("watch.log");
    let mut file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    let len = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let start = len.saturating_sub(MAX_TAIL_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::with_capacity((len - start) as usize);
    if file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let body = String::from_utf8_lossy(&bytes);
    body.lines()
        .rev()
        .take(max_lines)
        .map(crate::text::sanitize_display_line)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// M179 S9: classify the latest recorded state for the
/// "open the Watch tab" prompt. Returns one of:
/// - `Live`     — a run is in progress; prompt the user to
///   monitor (no second start).
/// - `Stale`    — a run was interrupted; prompt the user to
///   resume explicitly.
/// - `Terminal` — the latest run finished; the renderer
///   surfaces the result row.
/// - `None`     — no state file; the picker is fresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedStateKind {
    Live,
    Stale,
    Terminal,
    None,
}

pub fn classify_recorded_state(app: &App) -> RecordedStateKind {
    let Some(status) = &app.watch.status else {
        return RecordedStateKind::None;
    };
    match status.kind.as_str() {
        "live" => RecordedStateKind::Live,
        "stale" => RecordedStateKind::Stale,
        "terminal" => RecordedStateKind::Terminal,
        _ => RecordedStateKind::None,
    }
}

// ─── S10: stop verb ──────────────────────────────────────────────

/// M179 S10 / AC-04: shell out to `mp watch-control stop` to
/// gracefully terminate a live run. Returns the parsed
/// `StopReport`; the caller surfaces the structured response
/// (or error) in the lane footer. A missing or dead PID is a
/// stable no-op (the helper records the message in
/// `app.watch.last_error`).
pub fn stop_watch(
    runner: &crate::mp_runner::MpRunner,
    app: &mut App,
    timeout_secs: u64,
) -> Result<StopReport> {
    let payload = match runner.run_raw_allow_failure(
        "watch-control",
        &[
            "stop",
            "--format",
            "json",
            "--timeout-secs",
            &timeout_secs.to_string(),
        ],
    ) {
        Ok(stdout) => serde_json::from_slice(&stdout).unwrap_or(Value::Null),
        Err(e) => {
            app.watch.last_error = Some(format!("{e:#}"));
            app.touch();
            return Err(e);
        }
    };
    let report = parse_stop(&payload);
    // F-07: surface a no-op stop to the user. The previous
    // implementation returned Ok(report) regardless of
    // report.stopped, so a Stop invoked on a non-live run (race
    // between status fetch and keypress, or a user double-tap)
    // gave zero feedback. mp's "no live run; nothing to stop"
    // message is parsed into report.message but was discarded.
    if !report.stopped && !report.message.is_empty() {
        app.watch.last_error = Some(report.message.clone());
    }
    // M179 S10: best-effort refresh of the status snapshot so
    // the lane footer shows terminal=true after Stop returns.
    let status_payload = runner
        .run("watch-control", &["status", "--format", "json"])
        .unwrap_or(Value::Null);
    let next_status = Some(parse_status(&status_payload));
    if app.watch.status != next_status {
        app.watch.status = next_status;
        app.touch();
    }
    Ok(report)
}

// ─── S11: terminal-state restoration ──────────────────────────────

/// M179 S11 / AC-09: on `App::new()` (i.e., TUI start) and on
/// lane entry, populate `app.watch.status` from the M178 state
/// file. Restores the latest terminal result so a fresh Raul
/// process shows the final graph, queue outcomes, and concise
/// result row.
///
/// The caller is responsible for opening the PlanContext
/// (this helper is the pure parser). Returns the parsed
/// `WatchStatus` so the caller can decide whether to surface
/// a "monitor" or "resume" prompt (S9 / S11) without a second
/// subprocess call.
pub fn restore_latest_status(
    runner: &crate::mp_runner::MpRunner,
    app: &mut App,
) -> Result<Option<WatchStatus>> {
    let payload = match runner.run("watch-control", &["status", "--format", "json"]) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let status = parse_status(&payload);
    let is_meaningful = !status.state_file.is_empty() && status.kind != "unknown";
    if is_meaningful {
        app.watch.status = Some(status.clone());
        restore_queue_from_status(app, &status);
        Ok(Some(status))
    } else {
        Ok(None)
    }
}

/// Restore the exact persisted queue after candidates have loaded. Missing
/// candidate IDs are omitted deterministically and surfaced to the user.
/// Restored queues always invalidate preflight.
pub fn restore_queue_from_status(app: &mut App, status: &WatchStatus) {
    let queue = status
        .raw
        .get("state")
        .and_then(|state| state.get("queue"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|id| id.trim_start_matches('M').to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if queue.is_empty() {
        return;
    }
    let known: std::collections::HashSet<_> =
        app.watch.candidates.iter().map(|c| c.id.as_str()).collect();
    let missing: Vec<_> = queue
        .iter()
        .filter(|id| !known.contains(id.as_str()))
        .cloned()
        .collect();
    app.watch.set_selection(queue);
    app.watch.preflight = None;
    if !missing.is_empty() {
        app.watch.last_error = Some(format!(
            "restored queue omitted missing candidates: {}",
            missing.join(", ")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_payload() -> Value {
        json!([
            {"id": "M01", "title": "Approved",     "lifecycle": "approved",     "priority": "high"},
            {"id": "M02", "title": "In Progress",  "lifecycle": "in-progress", "priority": "normal"},
            {"id": "M03", "title": "Self Reviewed","lifecycle": "self-reviewed","priority": "low"},
            {"id": "M04", "title": "Reviewed",     "lifecycle": "reviewed",    "priority": "normal"},
            {"id": "M05", "title": "Remediation",  "lifecycle": "remediation", "priority": "high"},
            {"id": "M06", "title": "Draft",        "lifecycle": "draft",       "priority": "low"},
            {"id": "M07", "title": "Complete",     "lifecycle": "complete",    "priority": "low"},
            {"id": "M08", "title": "Cancelled",    "lifecycle": "cancelled",   "priority": "low"},
            {"id": "M09", "title": "Deferred",     "lifecycle": "deferred",    "priority": "low"},
            {"id": "M10", "title": "Blocked In Prog", "lifecycle": "in-progress","priority": "low","blocked": true}
        ])
    }

    #[test]
    fn filter_candidates_keeps_only_drivable_lifecycles() {
        let payload = sample_payload();
        let candidates = Watch::filter_candidates(&payload);
        // M10 has `lifecycle: in-progress` (drivable) and
        // `blocked: true` — the picker includes it; the dry-run
        // (S4) surfaces the `blocked` verdict as
        // `DepStatus::Blocked`. Lifecycle-only filtering.
        let ids: Vec<String> = candidates.iter().map(|c| c.id.clone()).collect();
        assert_eq!(ids, vec!["01", "02", "05", "10"]);
        for c in &candidates {
            assert!(
                is_drivable_lifecycle(&c.lifecycle),
                "{} has non-drivable lifecycle {}",
                c.id,
                c.lifecycle
            );
        }
    }

    #[test]
    fn filter_candidates_accepts_envelope_shapes() {
        // mp list milestones JSON can be a bare array, an object with a
        // `milestones` key, or one with `items`. All three must be
        // parsed by the picker. In-progress is drivable; blocked is
        // not a picker exclusion. Review aliases are not watch-drivable.
        for key in [None, Some("milestones"), Some("items")] {
            let mut payload = sample_payload();
            if let Some(k) = key {
                payload = json!({ k: payload });
            }
            let candidates = Watch::filter_candidates(&payload);
            assert_eq!(candidates.len(), 4, "key={:?}", key);
        }
    }

    #[test]
    fn is_drivable_lifecycle_covers_the_documented_states() {
        for s in DRIVABLE_LIFECYCLES {
            assert!(is_drivable_lifecycle(s), "{s} should be drivable");
        }
        for s in [
            "draft",
            "groomed",
            "done",
            "self-reviewed",
            "reviewed",
            "complete",
            "cancelled",
            "deferred",
            "blocked",
        ] {
            assert!(!is_drivable_lifecycle(s), "{s} should NOT be drivable");
        }
        // Projection must stay pinned to mp-model (no Raul-local drift).
        assert_eq!(DRIVABLE_LIFECYCLES, mp_model::WATCH_DRIVABLE_LIFECYCLES);
    }

    #[test]
    fn refresh_candidates_drops_selections_no_longer_in_the_set() {
        let mut watch = Watch::empty();
        watch.refresh_candidates(&sample_payload());
        watch.toggle_select("01");
        watch.toggle_select("02");
        watch.toggle_select("05");
        assert_eq!(watch.selected, vec!["01", "02", "05"]);
        // Drop M02 from the candidate set; selection should be
        // pruned but preserve relative order.
        let pruned = json!([
            {"id": "M01", "title": "Approved",     "lifecycle": "approved",     "priority": "high"},
            {"id": "M05", "title": "Remediation",  "lifecycle": "remediation", "priority": "high"},
        ]);
        watch.refresh_candidates(&pruned);
        assert_eq!(watch.selected, vec!["01", "05"]);
    }

    #[test]
    fn toggle_select_preserves_insertion_order() {
        let mut watch = Watch::empty();
        watch.refresh_candidates(&sample_payload());
        watch.toggle_select("02");
        watch.toggle_select("01");
        watch.toggle_select("05");
        assert_eq!(watch.selected, vec!["02", "01", "05"]);
        // Toggling an already-selected id removes it (preserving
        // order of the survivors).
        watch.toggle_select("01");
        assert_eq!(watch.selected, vec!["02", "05"]);
    }

    #[test]
    fn toggle_select_for_unknown_id_is_a_noop() {
        let mut watch = Watch::empty();
        watch.refresh_candidates(&sample_payload());
        watch.toggle_select("M99-not-in-candidates");
        assert!(watch.selected.is_empty());
    }

    #[test]
    fn queue_ids_matches_selected_in_order() {
        let mut watch = Watch::empty();
        watch.refresh_candidates(&sample_payload());
        watch.toggle_select("02");
        watch.toggle_select("05");
        watch.toggle_select("01");
        assert_eq!(watch.queue_ids(), &["02", "05", "01"]);
    }

    #[test]
    fn can_start_requires_successful_preflight_and_a_non_empty_selection() {
        let mut watch = Watch::empty();
        watch.refresh_candidates(&sample_payload());
        // No preflight yet → cannot start.
        assert!(!watch.can_start());
        // Selection only, no preflight → still cannot.
        watch.toggle_select("01");
        assert!(!watch.can_start());
        // Successful preflight + selection → can start.
        watch.preflight = Some(PreflightResult {
            queue_fingerprint: vec!["01".to_string()],
            per_milestone: vec![],
            aggregate_ok: true,
            raw: None,
            verdict: Ok(()),
        });
        assert!(watch.can_start());
        // Failed preflight → cannot.
        watch.preflight = Some(PreflightResult {
            queue_fingerprint: vec!["01".to_string()],
            per_milestone: vec![],
            aggregate_ok: false,
            raw: None,
            verdict: Err("blocked".to_string()),
        });
        assert!(!watch.can_start());
    }

    #[test]
    fn picker_index_clamps_to_bounds_on_refresh() {
        let mut watch = Watch::empty();
        watch.refresh_candidates(&sample_payload());
        watch.picker_index = 8; // out of range after refresh (len=5)
        let pruned = json!([
            {"id": "M01", "title": "Approved", "lifecycle": "approved", "priority": "high"},
        ]);
        watch.refresh_candidates(&pruned);
        assert_eq!(watch.picker_index, 0);
    }

    #[test]
    fn move_picker_wraps_around_in_both_directions() {
        let mut watch = Watch::empty();
        watch.refresh_candidates(&sample_payload());
        let len = watch.candidates.len();
        assert!(len >= 2, "test needs ≥2 candidates; got {len}");
        watch.picker_index = 0;
        watch.move_picker(-1);
        assert_eq!(watch.picker_index, len - 1);
        watch.move_picker(1);
        assert_eq!(watch.picker_index, 0);
    }

    #[test]
    fn move_picker_on_empty_is_a_noop() {
        let mut watch = Watch::empty();
        watch.move_picker(1);
        watch.move_picker(-1);
        assert_eq!(watch.picker_index, 0);
    }

    #[test]
    fn parse_status_extracts_live_classification() {
        let payload = json!({
            "run_state": {"kind": "live"},
            "state_file": "/abs/.mp/watch.state.json",
            "schema_version": 2,
            "pid_alive": true,
            "herdr_listed": true,
            "state": {
                "run_outcome": null,
                "milestone_outcomes": []
            }
        });
        let status = parse_status(&payload);
        assert_eq!(status.kind, "live");
        assert!(status.pid_alive);
        assert!(status.herdr_listed);
        assert!(status.run_outcome.is_none());
    }

    #[test]
    fn parse_status_extracts_terminal_outcome() {
        let payload = json!({
            "run_state": {"kind": "terminal"},
            "state_file": "/abs/.mp/watch.state.json",
            "pid_alive": false,
            "herdr_listed": false,
            "state": {
                "run_outcome": {"kind": "completed"},
                "milestone_outcomes": [
                    {"id": "170", "outcome": {"kind": "completed"}}
                ]
            }
        });
        let status = parse_status(&payload);
        assert_eq!(status.kind, "terminal");
        assert!(!status.pid_alive);
        assert!(status.run_outcome.is_some());
        assert_eq!(status.milestone_outcomes.len(), 1);
    }

    #[test]
    fn parse_output_extracts_bounded_text() {
        let payload = json!({
            "ok": true,
            "reason": "ok",
            "role": "runner",
            "pane_id": "%5",
            "output": "hello world",
            "truncated": false,
            "bytes": 11,
            "elapsed_ms": 42
        });
        let out = parse_output(&payload);
        assert!(out.ok);
        assert_eq!(out.role, "runner");
        assert_eq!(out.pane_id.as_deref(), Some("%5"));
        assert_eq!(out.output, "hello world");
        assert_eq!(out.bytes, 11);
    }

    #[test]
    fn parse_stop_extracts_stable_noop_shape() {
        let payload = json!({
            "stopped": false,
            "pid": null,
            "timeout_secs": 30,
            "elapsed_secs": 0.0,
            "message": "no live run; nothing to stop",
            "state_file": "/abs/.mp/watch.state.json"
        });
        let r = parse_stop(&payload);
        assert!(!r.stopped);
        assert!(r.pid.is_none());
        assert!(r.message.contains("no live run"));
    }
}
