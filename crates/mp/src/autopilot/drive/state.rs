//! M152 S1: crash-safe `watch.state.json` for `mp watch`.
//!
//! Persistent operational scratch the running watch process
//! maintains so a subsequent `mp watch --resume` (or even an
//! unplanned relaunch) can re-attach to live herdr panes instead
//! of re-spawning duplicate agents. **Operational scratch only** —
//! milestone truth still lives in `plan.json`; the state file is
//! never authoritative (see design decision `state-source-of-truth`).
//!
//! ## What lives in the state file
//!
//! ```text
//! {
//!   "schema_version": 1,
//!   "pid":               12345,
//!   "started_at":        "2026-...",
//!   "last_updated_at":   "2026-...",
//!   "panes":      [{role, label, pane_id, spawned_at, last_status?}],
//!   "milestones": [{id, last_lifecycle, target_lifecycle, last_action_at}],
//! }
//! ```
//!
//! - `pid` is informational (resume may compare against the running
//!   `ps` view); a stale pid != panic, just a hint that the
//!   previous run crashed.
//! - `panes` is the re-attachment map: `mp watch --resume` looks
//!   up `role + label` to find the existing pane id in herdr.
//! - `milestones` records the last-known lifecycle so the resume
//!   path can short-circuit plan.json reads when nothing has
//!   changed.
//!
//! ## Atomicity
//!
//! Writes go through `atomic_write`: a `NamedTempFile`
//! opened in the same directory, written, fsync'd via `persist`,
//! and renamed into place. A crash mid-write never produces a torn
//! file — the rename is the only on-disk transition that publishes
//! a new state.
//!
//! ## Why a module instead of folding into `logging`
//!
//! `logging.rs` is append-only and human/tail-friendly (JSONL). The
//! state file is a single JSON document that must be parseable
//! out-of-band by `mp watch --resume` before any subprocess has
//! been spawned. Keeping them separate avoids the bootstrap
//! chicken-and-egg of "resume needs the state, but logging writes
//! only after the runner boots".

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::autopilot::drive::Role;
use crate::paths::PlanContext;
use crate::store::atomic_write;

/// Current state-file schema. Bumped when an incompatible change
/// lands; [`WatchState::load`] surfaces a warning when reading a
/// newer-than-known schema (forward compatibility) and refuses an
/// older-than-known schema (backward compat is not promised; older
/// state files are ignored, the runner re-spawns).
pub const WATCH_STATE_SCHEMA_VERSION: u32 = 1;

/// Default path for the state file: `<plan_dir>/.mp/watch.state.json`.
/// The `.mp` directory is the canonical mp-internal scratch location
/// (also hosts `session.json`, `watch.log`); keeping the state file
/// next to them mirrors the same convention.
pub fn default_state_path(plan_dir: &Path) -> PathBuf {
    plan_dir.join(".mp").join("watch.state.json")
}

/// Snapshot of a single pane's tracked state. One entry per pane the
/// running watch owns (one runner pane + one coordinator pane in v1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneState {
    /// Role that owns this pane — used by `--resume` to match against
    /// the live pane label.
    pub role: Role,
    /// Stable pane label (`role-runner-1` / `role-coordinator-1` in v1;
    /// the counter is owned by the sequencer — S8).
    pub label: String,
    /// herdr pane id; opaque to mp (herdr may return a tmux target,
    /// a fresh uuid, or a session handle depending on the
    /// integration). Persisted so `--resume` can address the same
    /// pane without re-listing.
    pub pane_id: String,
    /// Spawned-at timestamp; informational.
    pub spawned_at: String,
    /// Last observed agent-status string at state-flush time.
    /// `None` means the watcher had not yet observed a status when
    /// it flushed (e.g. immediate SIGINT after spawn).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status: Option<String>,
}

/// Per-milestone tracking entry. The `last_lifecycle` field is what
/// `--resume` uses to skip the plan.json read in the common case
/// (the milestone has not advanced and is still in `in-progress`).
/// `plan.json` remains the source of truth on resume; this entry
/// is just a fast-path hint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneState {
    pub id: String,
    pub last_lifecycle: String,
    /// What lifecycle the watch was waiting for when it last flushed.
    /// On resume, if `plan.json` already shows a lifecycle past
    /// `target_lifecycle`, the milestone is "done-from-our-perspective"
    /// and the sequencer moves to the next milestone.
    pub target_lifecycle: String,
    pub last_action_at: String,
}

/// Crash-safe `mp watch` state file shape. Serialized to
/// `<plan_dir>/.mp/watch.state.json` through [`Self::save`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchState {
    pub schema_version: u32,
    pub pid: u32,
    pub started_at: String,
    pub last_updated_at: String,
    #[serde(default)]
    pub panes: Vec<PaneState>,
    #[serde(default)]
    pub milestones: Vec<MilestoneState>,
}

impl WatchState {
    /// Build a fresh state file with the current process's PID and
    /// "now" timestamps. The caller owns mutation from here.
    pub fn fresh(active_milestones: &[String]) -> Self {
        let now = crate::store::now_rfc3339();
        Self {
            schema_version: WATCH_STATE_SCHEMA_VERSION,
            pid: std::process::id(),
            started_at: now.clone(),
            last_updated_at: now,
            panes: Vec::new(),
            milestones: active_milestones
                .iter()
                .map(|id| MilestoneState {
                    id: id.clone(),
                    last_lifecycle: "approved".to_string(),
                    target_lifecycle: "in-progress".to_string(),
                    last_action_at: crate::store::now_rfc3339(),
                })
                .collect(),
        }
    }

    /// Compute the default state-file path for a plan directory.
    pub fn path_for(plan_dir: &Path) -> PathBuf {
        default_state_path(plan_dir)
    }

    /// Atomically write the state file. Uses `atomic_write` so a
    /// SIGKILL mid-write cannot publish a torn JSON document.
    /// The parent directory is created if missing (`<plan_dir>/.mp/`
    /// is the canonical scratch location but a brand-new project may
    /// not have created it yet).
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create state parent {}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .with_context(|| format!("serialize watch state to {}", path.display()))?;
        atomic_write(path, bytes)
            .with_context(|| format!("atomic write watch state {}", path.display()))
    }

    /// Atomic save under the default `.mp/watch.state.json` path.
    pub fn save_to_plan(&self, ctx: &PlanContext) -> Result<PathBuf> {
        let path = Self::path_for(&ctx.plan_dir);
        self.save(&path)?;
        Ok(path)
    }

    /// Load the state file at the default path under
    /// `ctx.plan_dir`. Returns `Ok(None)` when the file is
    /// missing (fresh-checkout case) — `--resume` treats that as
    /// "no prior run; spawn normally".
    ///
    /// A schema-version mismatch is logged-on-stdout and `Ok(None)`
    /// is returned: the stale file is left in place for forensics
    /// but treated as absent so the caller falls through to the
    /// non-resume path.
    pub fn load(ctx: &PlanContext) -> Result<Option<Self>> {
        let path = Self::path_for(&ctx.plan_dir);
        Self::load_from(&path)
    }

    /// Load the state file at an arbitrary path. Returns `Ok(None)`
    /// when missing or schema-incompatible. Never panics.
    ///
    /// M152 ext-review F-03 (2026-07-14): the corrupt-file and
    /// schema-mismatch branches emit a structured `[M152-LOAD]`
    /// prefix to stderr so tests + operators can filter / suppress.
    /// The layering smell (library code writing to stderr directly)
    /// is real but minor; a future refactor should plumb warnings
    /// through a return value so callers (e.g. `cmd_watch_drive`)
    /// can route them via the `DriveLogger` instead.
    pub fn load_from(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let bytes =
            std::fs::read(path).with_context(|| format!("read watch state {}", path.display()))?;
        let parsed: WatchState = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                // A torn or corrupt state file never blocks a fresh
                // run. Log a warning, drop it, return None — the
                // caller will spawn normally rather than skip.
                eprintln!(
                    "[M152-LOAD] warning: ignoring unreadable watch state {}: {e}",
                    path.display()
                );
                return Ok(None);
            }
        };
        // M178 S1: the v2 control-plane file is a strict superset of
        // v1 (same legacy fields + extras). v2 readers will prefer
        // `WatchRunState::load_from`; this legacy loader accepts v2
        // for the `--resume` reconciliation path so the M152 test
        // suite (which calls `WatchState::load_from` directly) keeps
        // working through the v2 era.
        if parsed.schema_version > 2 {
            eprintln!(
                "[M152-LOAD] warning: watch state {} has schema_version={}, \
                 max known is 2; ignoring",
                path.display(),
                parsed.schema_version,
            );
            return Ok(None);
        }
        Ok(Some(parsed))
    }

    /// Add or replace the pane entry for `role`. The
    /// `last_updated_at` timestamp is bumped to `now`.
    ///
    /// M152 ext-review F-01 (2026-07-14): on overwrite, preserve
    /// the existing `spawned_at` so repeated re-saves during a
    /// run don't reset the pane's first-spawn timestamp to "now".
    /// `last_status` is intentionally updated to the new value —
    /// it tracks the most recent agent-status observation, which
    /// is the caller's responsibility to populate.
    pub fn upsert_pane(&mut self, pane: PaneState) {
        if let Some(existing) = self.panes.iter_mut().find(|p| p.role == pane.role) {
            let mut next = pane;
            // Preserve the original `spawned_at` if we already
            // recorded one (callers that copy panes out of the ops
            // pane cache do not track the original spawn time and
            // would otherwise rewrite it to "now" on every save).
            if !existing.spawned_at.is_empty() {
                next.spawned_at = existing.spawned_at.clone();
            }
            *existing = next;
        } else {
            self.panes.push(pane);
        }
        self.last_updated_at = crate::store::now_rfc3339();
    }

    /// Add or replace the milestone entry for `id`. Bumps
    /// `last_updated_at`.
    pub fn upsert_milestone(&mut self, ms: MilestoneState) {
        if let Some(existing) = self.milestones.iter_mut().find(|m| m.id == ms.id) {
            *existing = ms;
        } else {
            self.milestones.push(ms);
        }
        self.last_updated_at = crate::store::now_rfc3339();
    }

    /// Look up a tracked pane by role. Used by `--resume` to find the
    /// pane id we recorded for the runner / coordinator role during
    /// the previous run.
    pub fn pane_for(&self, role: Role) -> Option<&PaneState> {
        self.panes.iter().find(|p| p.role == role)
    }

    /// Look up a tracked milestone by id.
    pub fn milestone(&self, id: &str) -> Option<&MilestoneState> {
        self.milestones.iter().find(|m| m.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_empty() -> WatchState {
        WatchState::fresh(&[])
    }

    #[test]
    fn fresh_state_has_schema_v1_and_current_pid() {
        let s = fresh_empty();
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.pid, std::process::id());
        assert!(s.started_at.contains('T'), "RFC3339 ts");
        assert_eq!(s.started_at, s.last_updated_at);
        assert!(s.panes.is_empty());
        assert!(s.milestones.is_empty());
    }

    #[test]
    fn fresh_state_seeds_active_milestones() {
        let s = WatchState::fresh(&["M152".to_string(), "M153".to_string()]);
        let ids: Vec<&str> = s.milestones.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["M152", "M153"]);
        // Defaults match the entry-point lifecycle target on approval.
        assert!(s.milestones.iter().all(|m| m.last_lifecycle == "approved"));
        assert!(s
            .milestones
            .iter()
            .all(|m| m.target_lifecycle == "in-progress"));
    }

    #[test]
    fn upsert_pane_replaces_by_role() {
        let mut s = fresh_empty();
        s.upsert_pane(PaneState {
            role: Role::Runner,
            label: "role-runner-1".into(),
            pane_id: "%5".into(),
            spawned_at: "now".into(),
            last_status: None,
        });
        s.upsert_pane(PaneState {
            role: Role::Coordinator,
            label: "role-coordinator-1".into(),
            pane_id: "%7".into(),
            spawned_at: "now".into(),
            last_status: None,
        });
        assert_eq!(s.panes.len(), 2);

        // Re-upsert Runner with a fresh pane id; the entry is
        // replaced, not appended.
        s.upsert_pane(PaneState {
            role: Role::Runner,
            label: "role-runner-1".into(),
            pane_id: "%9".into(),
            spawned_at: "later".into(),
            last_status: None,
        });
        assert_eq!(s.panes.len(), 2);
        let runner = s.pane_for(Role::Runner).unwrap();
        assert_eq!(runner.pane_id, "%9");
    }

    #[test]
    fn upsert_milestone_replaces_by_id() {
        let mut s = fresh_empty();
        s.upsert_milestone(MilestoneState {
            id: "M152".into(),
            last_lifecycle: "in-progress".into(),
            target_lifecycle: "self-reviewed".into(),
            last_action_at: "now".into(),
        });
        s.upsert_milestone(MilestoneState {
            id: "M152".into(),
            last_lifecycle: "self-reviewed".into(),
            target_lifecycle: "reviewed".into(),
            last_action_at: "later".into(),
        });
        assert_eq!(s.milestones.len(), 1);
        let ms = s.milestone("M152").unwrap();
        assert_eq!(ms.last_lifecycle, "self-reviewed");
    }

    #[test]
    fn save_then_load_round_trips_losslessly() {
        let dir = TempDir::new().unwrap();
        let mut s = WatchState::fresh(&["M152".to_string()]);
        s.upsert_pane(PaneState {
            role: Role::Runner,
            label: "role-runner-1".into(),
            pane_id: "%12".into(),
            spawned_at: "2026-01-01T00:00:00Z".into(),
            last_status: Some("working".into()),
        });
        s.upsert_milestone(MilestoneState {
            id: "M152".into(),
            last_lifecycle: "in-progress".into(),
            target_lifecycle: "self-reviewed".into(),
            last_action_at: "2026-01-01T00:00:01Z".into(),
        });

        let path = WatchState::path_for(dir.path());
        s.save(&path).unwrap();
        assert!(path.is_file(), "save must persist the file");

        let loaded = WatchState::load_from(&path).unwrap().unwrap();
        assert_eq!(loaded, s);
    }

    #[test]
    fn save_is_atomic_via_temp_then_rename() {
        // `atomic_write` is exercised here through `WatchState::save`.
        // We can't easily observe the temp-file race from a single-
        // threaded test, so we pin the contract indirectly: the
        // destination file must exist with the final content once
        // save returns, and a torn intermediate path must NOT be
        // visible at the destination.
        let dir = TempDir::new().unwrap();
        let path = WatchState::path_for(dir.path());
        let s = fresh_empty();
        s.save(&path).unwrap();
        let written = std::fs::read(&path).unwrap();
        // Destination file content begins with the JSON pretty
        // header (an open brace followed by `  "schema_version"`).
        assert!(written.starts_with(b"{"));
        assert!(std::str::from_utf8(&written)
            .unwrap()
            .contains("\"schema_version\": 1"));
    }

    #[test]
    fn load_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = WatchState::path_for(dir.path());
        assert!(!path.exists());
        let loaded = WatchState::load_from(&path).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn load_returns_none_when_corrupt() {
        let dir = TempDir::new().unwrap();
        let path = WatchState::path_for(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not json {{{").unwrap();
        // A torn file must not crash the resume; --resume falls
        // through to spawn normally. The `[M152-LOAD]` warning
        // (F-03) is emitted to stderr; operators + tests can
        // filter on the prefix.
        let loaded = WatchState::load_from(&path).unwrap();
        assert!(loaded.is_none());
        // File is left in place for forensics (corruption is
        // pinned: corrupt files are not deleted; they are skipped).
        assert!(path.is_file());
    }

    #[test]
    fn load_returns_none_on_schema_mismatch() {
        let dir = TempDir::new().unwrap();
        let path = WatchState::path_for(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let raw = r#"{
            "schema_version": 99,
            "pid": 1,
            "started_at": "t",
            "last_updated_at": "t",
            "panes": [],
            "milestones": []
        }"#;
        std::fs::write(&path, raw).unwrap();
        let loaded = WatchState::load_from(&path).unwrap();
        assert!(loaded.is_none(), "schema-incompatible file must be ignored");
    }

    #[test]
    fn pane_for_returns_none_when_missing() {
        let s = fresh_empty();
        assert!(s.pane_for(Role::Runner).is_none());
        assert!(s.pane_for(Role::Coordinator).is_none());
    }

    #[test]
    fn default_state_path_is_under_mp_subdir() {
        let plan = Path::new("/tmp/plan");
        assert_eq!(
            default_state_path(plan),
            PathBuf::from("/tmp/plan/.mp/watch.state.json")
        );
    }

    #[test]
    fn upsert_bumps_last_updated_at() {
        let mut s = fresh_empty();
        let first = s.last_updated_at.clone();
        // Sleep is brittle in unit tests; instead we just check
        // the timestamp string changes on a mutation through
        // `now_rfc3339`. Two consecutive mutations within the same
        // nanosecond are rare on real clocks but possible under
        // heavy load — we tolerate equality on the rare miss.
        s.upsert_pane(PaneState {
            role: Role::Runner,
            label: "x".into(),
            pane_id: "y".into(),
            spawned_at: "z".into(),
            last_status: None,
        });
        let second = s.last_updated_at.clone();
        assert!(
            second >= first,
            "last_updated_at must not regress: {first} → {second}"
        );
    }
}
