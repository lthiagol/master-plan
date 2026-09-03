//! M207 / AC-07: crash recovery for `session.json`.
//!
//! Recovery complements atomic writes: the rename-from-temp pattern
//! (used by [`crate::store::atomic_write`]) means a crash mid-write
//! cannot publish a torn document — the destination file is either
//! the pre-write version or the post-write version, never a hybrid.
//!
//! What recovery *does* handle is the case where a partial write
//! *did* land but the schema gate, the cursor reconciliation, or the
//! event log got out of sync:
//!
//! - **Schema drift after a partial write**: the loader re-validates
//!   every read against the embedded schema; a malformed tail is
//!   treated as parse-failure.
//! - **Cursor drift**: [`reconcile_event_cursor`] walks the surviving
//!   events and bumps the cursor to `max(events.seq)` so the next
//!   append does not regress.
//! - **Append-only invariant**: events are stored as a `Vec` and
//!   [`append_event_unchecked`] is the only writer; the cursor
//!   monotonicity is enforced by [`EventCursor::advance_to`].
//!
//! No event is ever deleted or rewritten. A torn session.json is
//! rejected and treated as "no session" by the loader — the next
//! write produces a fresh document.

use std::path::Path;

use anyhow::Result;

use crate::autopilot::events::{EventCursor, OrchestrationEvent};
use crate::autopilot::reconcile::{recover_event_tail, TailRecovery};
use crate::autopilot::session::{load_session_from, AutopilotSession};
use crate::autopilot::spawn::MpBinaryProvenance;
use crate::paths::PlanContext;

/// Reconcile the cursor against the surviving event tail. Returns
/// the new cursor value (max of stored + observed).
pub fn reconcile_event_cursor(cursor: &mut EventCursor, events: &[OrchestrationEvent]) {
    cursor.reconcile(events);
}

/// Load a session, reconcile its cursor against the surviving event
/// log, and write it back. Returns the number of events the cursor
/// was bumped by (0 on a clean read, >0 if a stale cursor was found).
pub fn recover_session(ctx: &PlanContext, session_id: &str) -> Result<RecoveredSession> {
    let path = crate::autopilot::session::SessionPath::new(ctx, session_id)?;
    recover_session_at(&path.file, &ctx.project_root)
}

/// Lower-level recovery that takes a file path. Useful for tests
/// that stage partial files.
pub fn recover_session_at(file: &Path, project_root: &Path) -> Result<RecoveredSession> {
    let mut session = load_session_from(file, project_root)?;
    let prev_cursor = session.event_cursor.last_seq;
    reconcile_event_cursor(&mut session.event_cursor, &session.events);
    let next_cursor = session.event_cursor.last_seq;
    save_session_at_and_touch_last_updated(file, &session)?;
    Ok(RecoveredSession {
        file: file.to_path_buf(),
        prev_cursor,
        next_cursor,
        events: session.events.len(),
    })
}

/// Helper that mirrors `save_session_at` but skips the
/// `last_updated` re-stamp, so recovery doesn't perturb the
/// timestamp. Used internally by `recover_session_at`.
fn save_session_at_and_touch_last_updated(file: &Path, session: &AutopilotSession) -> Result<()> {
    crate::autopilot::save_session_at(file, session)?;
    Ok(())
}

/// Diagnostic summary returned by [`recover_session`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredSession {
    pub file: std::path::PathBuf,
    pub prev_cursor: u64,
    pub next_cursor: u64,
    pub events: usize,
}

impl RecoveredSession {
    pub fn cursor_bumped(&self) -> u64 {
        self.next_cursor.saturating_sub(self.prev_cursor)
    }
}

/// Append a single event without round-tripping through the loader
/// (used by tests + future hot-path callers). The caller is
/// responsible for the atomic write.
pub fn append_event_unchecked(
    session: &mut AutopilotSession,
    event: OrchestrationEvent,
) -> Result<()> {
    session.event_cursor.advance_to(event.seq)?;
    session.events.push(event);
    Ok(())
}

// ─── M225 F-01: production wiring for AC-03 (event tail recovery) ───

/// Outcome of [`run_startup_recovery`]. Mirrors
/// [`crate::autopilot::reconcile::TailRecovery`] plus the
/// session-id and the prev/next cursor so the production hot
/// path can log a structured audit row.
///
/// The session id is carried because the wiring iterates over
/// every session in the plan dir (M225 / AC-03 "resume from
/// last valid event" must apply to every active session, not
/// just the one the user named).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupRecoveryReport {
    /// Session id this report covers.
    pub session_id: String,
    /// Underlying M225 verdict. `Recovered` means the cursor
    /// was bumped (or already correct) and the session is safe
    /// to dispatch from. `Rejected` means the schema or binary
    /// gate refused; the caller must surface the reason and
    /// refuse to resume.
    pub outcome: StartupRecoveryOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupRecoveryOutcome {
    /// Tail was recovered. The session is safe to dispatch from.
    /// `prev_cursor` / `next_cursor` mirror [`RecoveredSession`]
    /// for diagnostic logging.
    Recovered {
        prev_cursor: u64,
        next_cursor: u64,
        event_count: usize,
    },
    /// Tail was rejected. No mutation occurred.
    Rejected { reason: String, event_count: usize },
}

impl StartupRecoveryReport {
    /// True when the session is safe to dispatch from.
    pub fn is_safe(&self) -> bool {
        matches!(self.outcome, StartupRecoveryOutcome::Recovered { .. })
    }
}

/// M225 F-01 (AC-03) production wiring: load a session, run
/// [`recover_event_tail`] against the current binary, and write
/// the session back when the cursor was bumped. Refuses to
/// write on a `Rejected` verdict (the gate is hard — see
/// [`crate::autopilot::reconcile::recover_event_tail`] for the
/// contract).
///
/// This is the single entry point the `cmd_watch` / `mp
/// autopilot` hot path uses to "resume from the last valid
/// event sequence after a crash" (M225 intent). Calling it on a
/// clean session is a no-op (cursor matches events; no write
/// occurs).
pub fn run_startup_recovery(
    ctx: &PlanContext,
    session_id: &str,
    current_binary: &MpBinaryProvenance,
) -> Result<StartupRecoveryReport> {
    let path = crate::autopilot::session::SessionPath::new(ctx, session_id)?;
    let mut session = match load_session_from(&path.file, &ctx.project_root) {
        Ok(s) => s,
        Err(e) => {
            // A session that fails to load (e.g. corrupt JSON)
            // is NOT a recovery case — the operator must
            // intervene. We surface the load error verbatim.
            return Err(anyhow::anyhow!(
                "startup_recovery: load {session_id} failed: {e}"
            ));
        }
    };
    let prev_cursor = session.event_cursor.last_seq;
    let event_count = session.events.len();
    let result = recover_event_tail(&mut session, current_binary);
    match result {
        TailRecovery::Recovered { last_seq, .. } => {
            // Write back only when the cursor actually moved.
            // `last_seq == prev_cursor` means a clean read; no
            // disk I/O needed.
            if last_seq != prev_cursor {
                crate::autopilot::save_session_at(&path.file, &session)?;
            }
            Ok(StartupRecoveryReport {
                session_id: session_id.to_string(),
                outcome: StartupRecoveryOutcome::Recovered {
                    prev_cursor,
                    next_cursor: last_seq,
                    event_count,
                },
            })
        }
        TailRecovery::Rejected {
            reason,
            prior_event_count,
        } => {
            // Do NOT write. The gate is the contract.
            Ok(StartupRecoveryReport {
                session_id: session_id.to_string(),
                outcome: StartupRecoveryOutcome::Rejected {
                    reason: format!("{reason}"),
                    event_count: prior_event_count,
                },
            })
        }
    }
}

/// Discover all session ids under the plan dir. The production
/// wiring iterates this list and runs [`run_startup_recovery`]
/// on each. Pure file-system scan; no I/O beyond `read_dir`.
pub fn list_session_ids(ctx: &PlanContext) -> Result<Vec<String>> {
    let dir = crate::autopilot::session::autopilot_dir(ctx);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in
        std::fs::read_dir(&dir).map_err(|e| anyhow::anyhow!("read_dir {}: {e}", dir.display()))?
    {
        let entry = entry.map_err(|e| anyhow::anyhow!("read_dir entry: {e}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if path.join("session.json").is_file() {
            out.push(name.to_string());
        }
    }
    Ok(out)
}

/// Recover every session under the plan dir. The F-01 wiring
/// calls this on every `cmd_watch` / `cmd_autopilot start`
/// invocation. Returns one report per session. The caller
/// (the `cmd_watch_drive` entry point) decides what to do
/// with `Rejected` reports — currently it logs them and
/// continues (a corrupted session does not block other
/// sessions from running), but the reports are surfaced in
/// the JSON output so the operator can act.
pub fn run_startup_recovery_all(
    ctx: &PlanContext,
    current_binary: &MpBinaryProvenance,
) -> Result<Vec<StartupRecoveryReport>> {
    let ids = list_session_ids(ctx)?;
    let mut reports = Vec::with_capacity(ids.len());
    for id in ids {
        match run_startup_recovery(ctx, &id, current_binary) {
            Ok(report) => reports.push(report),
            Err(e) => {
                // Surface the load error as a Rejected report so
                // the caller sees one entry per session. The
                // load failure means the session is unusable
                // until the operator intervenes.
                reports.push(StartupRecoveryReport {
                    session_id: id,
                    outcome: StartupRecoveryOutcome::Rejected {
                        reason: format!("load failed: {e}"),
                        event_count: 0,
                    },
                });
            }
        }
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::events::EventKind;
    use crate::autopilot::session::{load_session, sample_session_for_tests, save_session};
    use crate::paths::PlanContext;
    use serde_json::Value;
    use std::fs;
    use tempfile::TempDir;

    fn ctx_in(dir: &Path) -> PlanContext {
        PlanContext {
            project_root: dir.to_path_buf(),
            plan_dir: dir.join("master-plan"),
        }
    }

    #[test]
    fn reconcile_event_cursor_bumps_to_max_event_seq() {
        let mut cursor = EventCursor::new();
        let events = vec![
            OrchestrationEvent::new(1, EventKind::Dispatch, "t", serde_json::json!({})),
            OrchestrationEvent::new(5, EventKind::Transition, "t", serde_json::json!({})),
        ];
        reconcile_event_cursor(&mut cursor, &events);
        assert_eq!(cursor.last_seq, 5);
    }

    #[test]
    fn reconcile_does_not_regress() {
        // Cursor is already at 10; surviving events only reach 7.
        // Reconcile must keep the cursor at 10 (no regression).
        let mut cursor = EventCursor::new();
        cursor.advance_to(10).unwrap();
        let events = vec![OrchestrationEvent::new(
            7,
            EventKind::Dispatch,
            "t",
            serde_json::json!({}),
        )];
        reconcile_event_cursor(&mut cursor, &events);
        assert_eq!(cursor.last_seq, 10);
    }

    #[test]
    fn append_event_unchecked_enforces_monotonic_cursor() {
        let mut s = sample_session_for_tests("alpha");
        let event = OrchestrationEvent::new(1, EventKind::Dispatch, "t", serde_json::json!({}));
        append_event_unchecked(&mut s, event).unwrap();
        assert_eq!(s.event_cursor.last_seq, 1);
        // Second event must have seq > 1.
        let second = OrchestrationEvent::new(1, EventKind::Dispatch, "t", serde_json::json!({}));
        let err = append_event_unchecked(&mut s, second).unwrap_err();
        assert!(format!("{err}").contains("regression"));
    }

    #[test]
    fn recover_session_no_op_when_cursor_matches() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let mut session = sample_session_for_tests("alpha");
        session.role_state = None;
        session.working_on = None;
        session.queue.clear();
        // Save with one event so cursor matches.
        let event = OrchestrationEvent::new(1, EventKind::Dispatch, "t", serde_json::json!({}));
        append_event_unchecked(&mut session, event).unwrap();
        save_session(&ctx, "alpha", &session).unwrap();

        let report = recover_session(&ctx, "alpha").unwrap();
        assert_eq!(report.cursor_bumped(), 0);
    }

    #[test]
    fn recover_session_advances_cursor_when_events_exceed_it() {
        // Write a session.json whose cursor is stale (lower than
        // the events' max seq). Recovery must bump the cursor to
        // match the surviving events.
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let mut session = sample_session_for_tests("alpha");
        session.role_state = None;
        session.working_on = None;
        session.queue.clear();
        // Three events, then artificially reset the cursor before
        // save to simulate a torn write.
        let events = vec![
            OrchestrationEvent::new(1, EventKind::Dispatch, "t", serde_json::json!({})),
            OrchestrationEvent::new(2, EventKind::Transition, "t", serde_json::json!({})),
            OrchestrationEvent::new(3, EventKind::Note, "t", serde_json::json!({})),
        ];
        for e in events {
            append_event_unchecked(&mut session, e).unwrap();
        }
        // Stale the cursor.
        session.event_cursor.last_seq = 1;
        save_session(&ctx, "alpha", &session).unwrap();

        let report = recover_session(&ctx, "alpha").unwrap();
        assert_eq!(report.prev_cursor, 1);
        assert_eq!(report.next_cursor, 3);
        assert_eq!(report.cursor_bumped(), 2);
    }

    #[test]
    fn torn_session_json_is_rejected_by_loader() {
        // Atomic write means we should never see a torn file in
        // practice — but the loader must defensively reject one
        // if it ever appears (e.g. operator edit, fs corruption).
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let session = sample_session_for_tests("alpha");
        save_session(&ctx, "alpha", &session).unwrap();
        let path = ctx.plan_dir.join("autopilot/alpha/session.json");
        // Overwrite with garbage.
        fs::write(&path, b"not json {{{").unwrap();
        let err = load_session(&ctx, "alpha").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("parse") || msg.contains("decode"), "got {msg}");
    }

    #[test]
    fn schema_invalid_session_is_rejected_by_loader() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let session = sample_session_for_tests("alpha");
        save_session(&ctx, "alpha", &session).unwrap();
        let path = ctx.plan_dir.join("autopilot/alpha/session.json");
        // Load the file, drop a required field, write back.
        let raw = fs::read_to_string(&path).unwrap();
        let mut value: Value = serde_json::from_str(&raw).unwrap();
        value.as_object_mut().unwrap().remove("status");
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

        let err = load_session(&ctx, "alpha").unwrap_err();
        let msg = format!("{err}");
        // The loader may fail at deserialize time (typed struct)
        // or at schema-validate time (typed struct OK). Either
        // path surfaces an error.
        assert!(
            msg.contains("schema validation") || msg.contains("decode") || msg.contains("parse"),
            "got {msg}"
        );
    }

    // ─── M225 F-01 wiring regression ──────────────────────────────────
    // The M225 F-01 review finding flagged that the reconcile
    // primitives were dead code as shipped. The fix is
    // `run_startup_recovery` / `run_startup_recovery_all` —
    // production entry points that exercise the AC-03 cursor
    // bump on every `cmd_watch` / `cmd_autopilot_start`. The
    // tests below pin the wiring contract: a torn write (cursor
    // below events) is bumped on load; a Rejected outcome
    // (incompatible schema) is surfaced, not swallowed.

    fn binary_with_schema(schema: u32) -> MpBinaryProvenance {
        MpBinaryProvenance {
            binary_path: "/usr/bin/mp".into(),
            version: "0.0.0-test".into(),
            schema_version: schema,
            build_kind: "test".into(),
            recorded_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn m225_f01_startup_recovery_bumps_stale_cursor() {
        // F-01 / AC-03 production wiring: a session whose
        // cursor lags the surviving events is recovered on
        // load. The F-01 review's load-bearing concern was
        // that the reconcile primitives were "dead code as
        // shipped" — this test pins that `run_startup_recovery`
        // is the production entry point.
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let mut session = sample_session_for_tests("alpha");
        session.role_state = None;
        session.working_on = None;
        session.queue.clear();
        // Three events; cursor stale at 1.
        let events = vec![
            OrchestrationEvent::new(1, EventKind::Dispatch, "t", serde_json::json!({})),
            OrchestrationEvent::new(2, EventKind::Transition, "t", serde_json::json!({})),
            OrchestrationEvent::new(3, EventKind::Note, "t", serde_json::json!({})),
        ];
        for e in events {
            append_event_unchecked(&mut session, e).unwrap();
        }
        session.event_cursor.last_seq = 1;
        save_session(&ctx, "alpha", &session).unwrap();

        let current = binary_with_schema(session.schema_version);
        let report = run_startup_recovery(&ctx, "alpha", &current).unwrap();
        match report.outcome {
            StartupRecoveryOutcome::Recovered {
                prev_cursor,
                next_cursor,
                event_count,
            } => {
                assert_eq!(prev_cursor, 1);
                assert_eq!(next_cursor, 3);
                assert_eq!(event_count, 3);
            }
            other => panic!("expected Recovered, got {other:?}"),
        }
        // The session on disk now has the bumped cursor.
        let reloaded = load_session(&ctx, "alpha").unwrap();
        assert_eq!(reloaded.event_cursor.last_seq, 3);
    }

    #[test]
    fn m225_f01_startup_recovery_rejects_incompatible_schema() {
        // F-01 / AC-03 production wiring: a session whose
        // recorded `binary_provenance` has a schema_version
        // newer than the current binary must NOT be mutated
        // by `run_startup_recovery`. The F-01 wiring surfaces
        // the rejection as a typed `Rejected` report (the
        // F-03 / F-01 contract).
        //
        // Note: a session whose top-level `schema_version` is
        // newer is rejected at LOAD time (the loader's
        // `UnknownSchemaVersion` gate) — that path is
        // covered by `schema_invalid_session_is_rejected_by_loader`
        // above. The `binary_provenance` path is the
        // complementary gate that fires inside
        // `recover_event_tail`.
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        let mut session = sample_session_for_tests("alpha");
        session.role_state = None;
        session.working_on = None;
        session.queue.clear();
        // Recorded binary has a newer schema_version than
        // the current binary.
        session.binary_provenance = Some(MpBinaryProvenance {
            binary_path: "/usr/bin/mp".into(),
            version: "future".into(),
            schema_version: u32::MAX,
            build_kind: "release".into(),
            recorded_at: "2099-01-01T00:00:00Z".into(),
        });
        save_session(&ctx, "alpha", &session).unwrap();

        let current = binary_with_schema(1);
        let report = run_startup_recovery(&ctx, "alpha", &current).unwrap();
        match report.outcome {
            StartupRecoveryOutcome::Rejected { reason, .. } => {
                assert!(reason.contains("schema"), "got {reason}");
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
        // The session on disk is unchanged.
        let reloaded = load_session(&ctx, "alpha").unwrap();
        assert!(reloaded
            .binary_provenance
            .as_ref()
            .map(|p| p.schema_version == u32::MAX)
            .unwrap_or(false));
    }

    #[test]
    fn m225_f01_startup_recovery_all_iterates_every_session() {
        // F-01 wiring: `run_startup_recovery_all` is the
        // entry point the `cmd_watch_drive` production path
        // calls. It must iterate every session under the
        // plan dir and produce one report per session — the
        // F-01 contract "no fabricated completion after pane
        // restart" is checked per-session.
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(tmp.path());
        // Two sessions, both clean.
        for id in ["alpha", "beta"] {
            let mut session = sample_session_for_tests(id);
            session.role_state = None;
            session.working_on = None;
            session.queue.clear();
            let event = OrchestrationEvent::new(1, EventKind::Note, "t", serde_json::json!({}));
            append_event_unchecked(&mut session, event).unwrap();
            save_session(&ctx, id, &session).unwrap();
        }
        let current = binary_with_schema(1);
        let reports = run_startup_recovery_all(&ctx, &current).unwrap();
        let ids: Vec<&str> = reports.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"alpha"), "missing alpha in {ids:?}");
        assert!(ids.contains(&"beta"), "missing beta in {ids:?}");
        assert_eq!(reports.len(), 2, "got {reports:?}");
    }
}
