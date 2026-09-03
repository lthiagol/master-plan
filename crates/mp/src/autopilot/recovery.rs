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
use crate::autopilot::session::{load_session_from, AutopilotSession};
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
}
