//! Project-shared activity journal stored at `<plan_dir>/activity.json`.
//!
//! The journal retains the newest [`ACTIVITY_RETENTION_CAP`] events. Appends are
//! best-effort relative to the primary mutation and must run under the plan write
//! lock so concurrent read-modify-write cycles cannot interleave. An absent file
//! represents an empty feed; no historical backfill is attempted.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::paths::PlanContext;
use crate::store::atomic_write;
use crate::store::read_text_bounded;
use crate::store::MAX_PLAN_FILE_BYTES;

/// Maximum number of core-detail events retained on disk. Adding event
/// 501 evicts only the oldest event row (the storage layer maintains
/// the cap invariant — see `append_event`).
pub const ACTIVITY_RETENTION_CAP: usize = 500;

/// Current journal schema version. Bumped only on breaking changes to
/// the `ActivityLog` / `ActivityEvent` shape; minor additive fields
/// keep the same version.
pub const ACTIVITY_SCHEMA_VERSION: u32 = 1;

/// Core activity event row. The discriminator is the `type` field;
/// `subject` and `summary` are short string fields. `subject` is the
/// normalized milestone id (e.g. `180`, `M180.1`) for milestone-scoped
/// events, otherwise empty.
///
/// `data` is an optional structured payload used by event types that
/// need to round-trip typed state (e.g. `validation-state` carries
/// the prior/cur ok + error_count so consumers don't need to parse
/// the human-readable summary). Absent for events without typed
/// payload; `skip_serializing_if` keeps the on-disk JSON shape
/// identical to pre-`data` events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    /// RFC3339 UTC timestamp (e.g. `2026-07-17T18:21:03+00:00`).
    pub timestamp: String,
    /// Event discriminator in kebab-case
    /// (`milestone-created`, `lifecycle-transition`, …).
    #[serde(rename = "type")]
    pub r#type: String,
    /// Milestone id (e.g. `180`) when the event is milestone-scoped;
    /// otherwise empty.
    pub subject: String,
    /// Short one-line summary safe for the Overview feed.
    pub summary: String,
    /// Optional typed payload — see [`Self::with_data`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ActivityEvent {
    pub fn now(
        r#type: impl Into<String>,
        subject: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            r#type: r#type.into(),
            subject: subject.into(),
            summary: summary.into(),
            data: None,
        }
    }

    /// Attach a typed payload to the event. Use this instead of
    /// stuffing structured data into the `summary` string — the
    /// structured form is round-trip-stable and survives summary
    /// wording changes.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// On-disk shape of the journal. Newest-first when read via
/// [`read_recent_events`] (the in-memory representation is append
/// order; readers reverse as needed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ActivityLog {
    pub schema_version: u32,
    pub events: Vec<ActivityEvent>,
}

impl ActivityLog {
    pub fn empty() -> Self {
        Self {
            schema_version: ACTIVITY_SCHEMA_VERSION,
            events: Vec::new(),
        }
    }

    /// Append an event, then enforce [`ACTIVITY_RETENTION_CAP`] by
    /// dropping the oldest rows. Empty input is a no-op. No-op also
    /// when the log already exceeds the cap (defensive — should never
    /// happen because every append trims, but a corrupt file read
    /// could leave a longer vector on first touch).
    pub fn push_bounded(&mut self, ev: ActivityEvent) {
        if ev.r#type.is_empty() {
            return;
        }
        self.events.push(ev);
        if self.events.len() > ACTIVITY_RETENTION_CAP {
            let drop = self.events.len() - ACTIVITY_RETENTION_CAP;
            self.events.drain(0..drop);
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Default path for the journal. Mirrors `PlanContext::activity_path`.
pub fn default_path(plan_dir: &Path) -> PathBuf {
    plan_dir.join("activity.json")
}

/// Read the journal at the default plan location. Absent file → empty
/// log. Schema-incompatible file → empty log + warning (same shape
/// contract as `WatchState::load_from`).
pub fn load(ctx: &PlanContext) -> Result<ActivityLog> {
    load_from(&ctx.activity_path())
}

/// Read the journal at an arbitrary path. The corrupt-file path
/// returns the default (empty) log; callers that need to distinguish
/// "absent" from "corrupt" should use [`load_from_with_status`].
pub fn load_from(path: &Path) -> Result<ActivityLog> {
    let (log, _status) = load_from_with_status(path)?;
    Ok(log)
}

/// Like [`load_from`] but also returns a structured status string so
/// callers (tests, `--format raw`) can tell absent vs corrupt apart.
pub fn load_from_with_status(path: &Path) -> Result<(ActivityLog, &'static str)> {
    if !path.exists() {
        return Ok((ActivityLog::empty(), "absent"));
    }
    let raw = read_text_bounded(path, MAX_PLAN_FILE_BYTES)
        .with_context(|| format!("read activity journal {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok((ActivityLog::empty(), "empty"));
    }
    match serde_json::from_str::<ActivityLog>(&raw) {
        Ok(mut log) => {
            if log.schema_version == 0 {
                log.schema_version = ACTIVITY_SCHEMA_VERSION;
            }
            if log.events.len() > ACTIVITY_RETENTION_CAP {
                let drop = log.events.len() - ACTIVITY_RETENTION_CAP;
                log.events.drain(0..drop);
            }
            Ok((log, "ok"))
        }
        Err(e) => {
            // M180 S11: a malformed history is a structured diagnostic.
            // We do not destroy the file (forensic value); we surface
            // the warning to stderr and treat the journal as empty.
            eprintln!(
                "warning: ignoring unreadable activity journal {}: {e}",
                path.display()
            );
            Ok((ActivityLog::empty(), "corrupt"))
        }
    }
}

/// Append `event` to the journal at the default plan location with
/// the standard bounded retention, outside any external lock.
///
/// **Locking contract.** This primitive does NOT acquire the plan
/// write lock. The milestone dispatcher (`cmd_milestone`) wraps every
/// write path in [`crate::plan_io::with_plan_write_lock`], so
/// per-milestone callers (`apply_spec_status`, `set_lifecycle`,
/// `complete_milestone`, …) must call this primitive directly — a
/// re-lock from within the dispatcher's already-held section would
/// deadlock on the same thread (the lock's
/// `MP_LOCK_TIMEOUT_SECS` default would surface as a spurious
/// failure).
///
/// Callers outside `cmd_milestone` (validation, scripts) should use
/// [`append_event_best_effort`] which acquires the lock on their
/// behalf.
pub(crate) fn append_event(ctx: &PlanContext, event: ActivityEvent) -> Result<()> {
    let path = ctx.activity_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create journal parent {}", parent.display()))?;
    }
    let mut log = load_from(&path)?;
    log.push_bounded(event);
    let bytes = serde_json::to_vec_pretty(&log)
        .with_context(|| format!("serialize activity journal {}", path.display()))?;
    atomic_write(&path, bytes)
        .with_context(|| format!("atomic write activity journal {}", path.display()))?;
    Ok(())
}

/// Append `event` under the plan-write lock; on failure, log a
/// structured warning and return `Ok(None)`. The underlying mutation
/// has already succeeded, so the journal is best-effort.
///
/// **Locking contract.** Acquires [`crate::plan_io::with_plan_write_lock`].
/// Do NOT call this from inside a `cmd_milestone` write path — the
/// dispatcher already holds the lock and re-acquiring it deadlocks.
/// Use `append_event_best_effort_unlocked` there.
///
/// Returns `Ok(Some(()))` on a clean append, `Ok(None)` on swallowed
/// failure. The structured warning message is emitted to stderr so
/// operators can spot it; the JSON stdout contract is preserved.
pub fn append_event_best_effort(ctx: &PlanContext, event: ActivityEvent) -> Result<Option<()>> {
    let event_type = event.r#type.clone();
    let event_subject = event.subject.clone();
    let result = crate::plan_io::with_plan_write_lock(&ctx.plan_dir, || append_event(ctx, event));
    match result {
        Ok(()) => Ok(Some(())),
        Err(e) => {
            emit_best_effort_warning(&event_type, &event_subject, &e);
            Ok(None)
        }
    }
}

/// In-lock variant of [`append_event_best_effort`] for callers that already
/// hold the plan-write lock. It does not acquire the lock; swallowing and
/// warning semantics are identical.
///
/// The primary mutation has already committed, so journal failure surfaces as
/// a warning plus `Ok(None)` rather than a false command rollback.
///
/// **Locking contract.** Caller MUST already hold
/// [`crate::plan_io::with_plan_write_lock`]. Re-acquiring the lock
/// from the same thread deadlocks; use
/// [`append_event_best_effort`] instead if you don't hold it.
pub(crate) fn append_event_best_effort_unlocked(
    ctx: &PlanContext,
    event: ActivityEvent,
) -> Result<Option<()>> {
    let event_type = event.r#type.clone();
    let event_subject = event.subject.clone();
    match append_event(ctx, event) {
        Ok(()) => Ok(Some(())),
        Err(e) => {
            emit_best_effort_warning(&event_type, &event_subject, &e);
            Ok(None)
        }
    }
}

fn emit_best_effort_warning(event_type: &str, event_subject: &str, e: &anyhow::Error) {
    eprintln!(
        "warning: activity journal append failed (primary mutation \
         succeeded); event_type={} subject={} error={e:#}",
        event_type, event_subject
    );
}

/// Read up to `limit` events newest-first. `limit == 0` returns an
/// empty vector. `limit > log.len()` returns the full log newest-first.
pub fn read_recent_events(ctx: &PlanContext, limit: usize) -> Result<Vec<ActivityEvent>> {
    let log = load(ctx)?;
    let cap = limit.min(log.events.len());
    let out: Vec<ActivityEvent> = log.events.iter().rev().take(cap).cloned().collect();
    Ok(out)
}

/// Convenience builder for milestone-lifecycle transition events.
/// `from` is the previous lifecycle (empty string when the milestone
/// was just created) and `to` is the new lifecycle.
pub fn lifecycle_event(milestone_id: &str, from: &str, to: &str) -> ActivityEvent {
    let summary = if from.is_empty() {
        format!("milestone created ({to})")
    } else {
        format!("lifecycle: {from} → {to}")
    };
    ActivityEvent::now("lifecycle-transition", milestone_id, summary)
}

/// Convenience builder for milestone-created events.
pub fn milestone_created_event(milestone_id: &str) -> ActivityEvent {
    ActivityEvent::now(
        "milestone-created",
        milestone_id,
        format!("milestone created ({milestone_id})"),
    )
}

/// M180 S4: block event for a milestone. `reason` is the block reason
/// recorded on the milestone file (may be empty when the operator
/// omits it).
pub fn milestone_blocked_event(milestone_id: &str, reason: &str) -> ActivityEvent {
    let summary = if reason.is_empty() {
        format!("milestone {milestone_id} blocked")
    } else {
        format!("milestone {milestone_id} blocked: {reason}")
    };
    ActivityEvent::now("milestone-blocked", milestone_id, summary)
}

/// M180 S4: unblock event for a milestone.
pub fn milestone_unblocked_event(milestone_id: &str) -> ActivityEvent {
    ActivityEvent::now(
        "milestone-unblocked",
        milestone_id,
        format!("milestone {milestone_id} unblocked"),
    )
}

/// M180 S4: execution handoff succeeded. `by` is the recorded
/// `handoff_by` field (defaults to "user" in the model).
pub fn execution_handoff_event(by: &str, count: usize) -> ActivityEvent {
    ActivityEvent::now(
        "execution-handoff",
        "",
        format!("execution handed off by {by} ({count} milestone(s) ready)"),
    )
}

/// M180 S5: a watch run started (driven by `mp watch start`).
pub fn watch_started_event(milestones: &[String]) -> ActivityEvent {
    let ids = if milestones.is_empty() {
        String::new()
    } else {
        milestones.join(",")
    };
    ActivityEvent::now("watch-started", "", format!("mp watch started ({ids})"))
}

/// M180 S5: a watch run was gracefully stopped via `mp
/// watch-control stop`.
pub fn watch_stopped_event(pid: u32) -> ActivityEvent {
    ActivityEvent::now("watch-stopped", "", format!("mp watch stopped (pid {pid})"))
}

/// M180 S5: watch run reached a terminal outcome. Maps the
/// M178 `RunOutcome` variants onto the 5-state Overview watch
/// summary (M180 `watch-summary-derivation` design decision).
pub fn watch_outcome_event(
    outcome: &crate::watch::RunOutcome,
    milestones: &[String],
) -> ActivityEvent {
    let ids = if milestones.is_empty() {
        String::new()
    } else {
        milestones.join(",")
    };
    let (r#type, label) = match outcome {
        crate::watch::RunOutcome::Completed => ("watch-completed", "completed"),
        crate::watch::RunOutcome::PartialFailure => ("watch-failed", "partial-failure"),
        crate::watch::RunOutcome::Skipped { .. } => ("watch-skipped", "skipped"),
        crate::watch::RunOutcome::Exhausted { .. } => ("watch-exhausted", "exhausted"),
        crate::watch::RunOutcome::GracefullyStopped => ("watch-stopped", "gracefully-stopped"),
        // M197 WP3 / AC-04: a verified spawn failure gets its
        // own activity tag. The argv + exit code is in the v2
        // control-plane state, so the activity entry just
        // needs a distinct type so dashboards can filter
        // "spawn failure" apart from "partial failure".
        crate::watch::RunOutcome::SpawnFailed { .. } => ("watch-spawn-failed", "spawn-failed"),
    };
    ActivityEvent::now(r#type, "", format!("mp watch {label} ({ids})"))
}

/// M180 S6: validation state changed. `prev_ok` / `prev_count` are
/// the previous persisted state (when known); `cur_ok` / `cur_count`
/// are the new run. The summary describes the direction so the
/// Overview feed renders a clear arrow; the structured `data`
/// payload carries the typed (prev_ok, prev_count, cur_ok,
/// cur_count) tuple so consumers round-trip without parsing prose.
pub fn validation_state_event(
    prev_ok: Option<bool>,
    prev_count: Option<usize>,
    cur_ok: bool,
    cur_count: usize,
) -> ActivityEvent {
    let summary = match (prev_ok, prev_count) {
        (Some(p_ok), Some(p_count)) if p_ok == cur_ok && p_count == cur_count => {
            // Caller should not have invoked this helper when state
            // is unchanged. Defensive summary for the rare case.
            format!("validation state unchanged (ok={cur_ok}, errors={cur_count})")
        }
        (Some(p_ok), Some(p_count)) if p_ok != cur_ok => {
            if cur_ok {
                format!("validation state recovered ({p_count} → 0 errors)")
            } else {
                format!("validation state errors ({p_count} → {cur_count} errors)")
            }
        }
        // Defensive: cover asymmetric shapes (e.g. previous loaded
        // with one field missing). The S7 loader populates both
        // fields together, so this branch is mostly unreachable in
        // practice; it keeps the match exhaustive.
        (Some(p_ok), None) if p_ok != cur_ok => {
            if cur_ok {
                "validation state recovered".to_string()
            } else {
                format!("validation state errors (now {cur_count})")
            }
        }
        (None, _) => {
            if cur_ok {
                "validation state initialized (ok)".to_string()
            } else {
                format!("validation state initialized ({cur_count} errors)")
            }
        }
        (Some(p_ok), Some(p_count)) => {
            // ok unchanged but error count shifted (rare: same ok
            // but different number of errors — e.g. error code
            // replaced). Surface the count delta explicitly.
            format!("validation error count shifted ({p_count} → {cur_count}, ok={p_ok})")
        }
        // Asymmetric shape with ok unchanged: the S7 loader sets
        // both fields together so this is unreachable in practice,
        // but the match must be exhaustive.
        (Some(_), None) => {
            format!("validation state re-recorded (ok={cur_ok}, errors={cur_count})")
        }
    };
    let data = serde_json::json!({
        "prev_ok": prev_ok,
        "prev_count": prev_count,
        "cur_ok": cur_ok,
        "cur_count": cur_count,
    });
    ActivityEvent::now("validation-state", "", summary).with_data(data)
}

/// Convenience helper that compares `old` and `new` and, when they
/// differ, appends a `lifecycle-transition` event. Same-state
/// mutations are no-ops (the AC-02 contract: "Repeated reads and
/// no-op writes do not create duplicate activity").
///
/// Use this from every lifecycle write seam
/// (`create_milestone`, `apply_spec_status`, `set_lifecycle`,
/// `set_execution_status`, `complete_milestone`,
/// `reopen_milestone`, remediation entry/exit). When `old` is empty
/// the event is labelled `milestone created` to match the
/// `milestone-created` discriminator the test suite checks for.
///
/// **Locking contract.** This helper uses
/// `append_event_best_effort_unlocked` (no lock) so it is safe to
/// call from inside the `cmd_milestone` dispatcher's already-held
/// write lock. **AC-04 (M180 F-02):** a journal write failure is
/// swallowed and surfaced as a stderr warning; the helper always
/// returns `Ok(())` so the caller's primary mutation — already
/// committed by the time this runs — never reports a false rollback.
pub fn record_lifecycle_transition(
    ctx: &PlanContext,
    milestone_id: &str,
    old: &str,
    new: &str,
) -> Result<()> {
    if old == new {
        return Ok(());
    }
    let event = if old.is_empty() {
        milestone_created_event(milestone_id)
    } else {
        lifecycle_event(milestone_id, old, new)
    };
    // AC-04 (M180 F-02): swallow + warn — never bubble journal errors
    // out of a lifecycle write seam.
    let _ = append_event_best_effort_unlocked(ctx, event)?;
    Ok(())
}
/// M180 S6 helper: find the most recent `validation-state` event in
/// the journal, if any. Used by [`record_validation_state_change`]
/// to detect state changes between successive validate runs without
/// needing a side-channel state file (which would turn validate
/// into a write operation and break read-only callers / golden
/// scenarios).
///
/// Returns `(ok, error_count)` for the last `validation-state` row,
/// or `None` when no prior validation event exists. Reads the
/// structured `data` payload (preferred) and falls back to a
/// summary-string scan only for journals written before the
/// `data` field existed.
pub fn last_validation_state(ctx: &PlanContext) -> Result<Option<(bool, usize)>> {
    let log = load(ctx)?;
    for ev in log.events.iter().rev() {
        if ev.r#type != "validation-state" {
            continue;
        }
        if let Some((ok, count)) = parse_validation_data(&ev.data) {
            return Ok(Some((ok, count)));
        }
        // Legacy fallback: a pre-`data` validation-state event whose
        // summary was written by an older M180 release. The parsing
        // here is best-effort — an unrecognized summary returns
        // None and the next call treats the journal as having no
        // prior validation event.
        if let Some((ok, count)) = parse_legacy_validation_summary(&ev.summary) {
            return Ok(Some((ok, count)));
        }
    }
    Ok(None)
}

fn parse_validation_data(data: &Option<serde_json::Value>) -> Option<(bool, usize)> {
    let data = data.as_ref()?;
    let obj = data.as_object()?;
    let cur_ok = obj.get("cur_ok")?.as_bool()?;
    let cur_count = obj.get("cur_count")?.as_u64()? as usize;
    Some((cur_ok, cur_count))
}

/// Legacy fallback for journals written before `ActivityEvent::data`
/// shipped. **Best-effort only (M180 F-01):** an unrecognized summary
/// returns `None` and the caller treats the journal as having no
/// prior state, which silently degrades change-detection to "always
/// emit". Only fires for journals written during the M180 dev window
/// (the canonical `data` field always populates for new events), so
/// the impact is bounded; the docstring is the durable record of
/// the residual.
fn parse_legacy_validation_summary(summary: &str) -> Option<(bool, usize)> {
    if summary.contains("initialized") {
        if summary.contains("(ok)") {
            return Some((true, 0));
        }
        if let Some(rest) = summary.strip_prefix("validation state initialized (") {
            if let Some(inner) = rest.strip_suffix(" errors)") {
                if let Ok(n) = inner.parse::<usize>() {
                    return Some((false, n));
                }
            }
        }
        return None;
    }
    if summary.contains("recovered") {
        return Some((true, 0));
    }
    if summary.contains("validation state errors") || summary.contains("error count shifted") {
        if let Some(rest) = summary.split('→').next_back() {
            let n: usize = rest
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            return Some((false, n));
        }
    }
    if summary.contains("unchanged") {
        let mut ok = false;
        let mut count = 0usize;
        if let Some(rest) = summary.split("ok=").nth(1) {
            ok = rest.starts_with('t');
        }
        if let Some(rest) = summary.split("errors=").nth(1) {
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            count = n.parse().unwrap_or(0);
        }
        return Some((ok, count));
    }
    None
}

/// Emit one `validation-state` event when validation differs from the most
/// recent journal state. Returns whether an event was emitted.
///
/// Reads the journal (no write) to find the prior state, then
/// appends (write) if changed. This keeps `validate` effectively
/// pure on the no-change path so golden scenarios that expect
/// `fs_unchanged` after a validate call continue to hold (the
/// journal is only touched when state actually changes, and most
/// runs hit the unchanged path).
///
/// The prior-state read runs outside the plan-write lock; append re-acquires it
/// inside
/// [`append_event_best_effort`]. Two concurrent `mp validate`
/// invocations could both observe the same prior, both decide
/// "changed", and both append — producing a duplicate
/// `validation-state` event. Impact is bounded (best-effort +
/// retention cap + structural dedup at the consumer), so the
/// residual is accepted at low severity.
pub fn record_validation_state_change(
    ctx: &PlanContext,
    ok: bool,
    error_count: usize,
) -> Result<bool> {
    let prior = last_validation_state(ctx)?;
    let changed = match prior {
        Some((p_ok, p_count)) => p_ok != ok || p_count != error_count,
        None => true,
    };
    if !changed {
        return Ok(false);
    }
    let event = validation_state_event(
        prior.map(|(ok, _)| ok),
        prior.map(|(_, count)| count),
        ok,
        error_count,
    );
    append_event_best_effort(ctx, event).map(|_| true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn ctx(tmp: &TempDir) -> PlanContext {
        PlanContext {
            project_root: tmp.path().to_path_buf(),
            plan_dir: tmp.path().to_path_buf(),
        }
    }

    #[test]
    fn empty_log_round_trips() {
        let log = ActivityLog::empty();
        let s = serde_json::to_string(&log).unwrap();
        let back: ActivityLog = serde_json::from_str(&s).unwrap();
        assert_eq!(back, log);
        assert_eq!(back.schema_version, ACTIVITY_SCHEMA_VERSION);
    }

    #[test]
    fn push_bounded_caps_at_500() {
        let mut log = ActivityLog::empty();
        for i in 0..ACTIVITY_RETENTION_CAP + 5 {
            log.push_bounded(ActivityEvent::now(
                "lifecycle-transition",
                format!("M{i:03}"),
                format!("step {i}"),
            ));
        }
        assert_eq!(log.len(), ACTIVITY_RETENTION_CAP);
        // The oldest five rows are the dropped ones; the first
        // surviving row is the 5th appended event.
        assert_eq!(log.events[0].summary, "step 5");
        assert_eq!(
            log.events.last().unwrap().summary,
            format!("step {}", ACTIVITY_RETENTION_CAP + 4)
        );
    }

    #[test]
    fn push_bounded_skips_empty_type() {
        let mut log = ActivityLog::empty();
        log.push_bounded(ActivityEvent::now("", "M01", "ignored"));
        assert!(log.is_empty());
    }

    #[test]
    fn load_absent_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let (log, status) = load_from_with_status(&default_path(tmp.path())).unwrap();
        assert!(log.is_empty());
        assert_eq!(status, "absent");
    }

    #[test]
    fn load_corrupt_returns_empty_and_warns() {
        let tmp = TempDir::new().unwrap();
        let path = default_path(tmp.path());
        std::fs::write(&path, b"not json {{{").unwrap();
        let (log, status) = load_from_with_status(&path).unwrap();
        assert!(log.is_empty());
        assert_eq!(status, "corrupt");
        // File preserved for forensics.
        assert!(path.exists());
    }

    #[test]
    fn load_trims_over_cap_input() {
        let tmp = TempDir::new().unwrap();
        let path = default_path(tmp.path());
        let mut log = ActivityLog::empty();
        for i in 0..ACTIVITY_RETENTION_CAP + 10 {
            log.push_bounded(ActivityEvent::now(
                "lifecycle-transition",
                "M01",
                format!("e{i}"),
            ));
        }
        std::fs::write(&path, serde_json::to_vec_pretty(&log).unwrap()).unwrap();
        let back = load_from(&path).unwrap();
        assert_eq!(back.len(), ACTIVITY_RETENTION_CAP);
        assert_eq!(back.events[0].summary, "e10");
    }

    #[test]
    fn append_event_persists_across_loads() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        append_event(
            &ctx,
            ActivityEvent::now("milestone-created", "M01", "created"),
        )
        .unwrap();
        append_event(
            &ctx,
            ActivityEvent::now("lifecycle-transition", "M01", "approved → in-progress"),
        )
        .unwrap();
        let log = load(&ctx).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log.events[0].r#type, "milestone-created");
        assert_eq!(log.events[1].r#type, "lifecycle-transition");
    }

    #[test]
    fn read_recent_events_returns_newest_first() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        for i in 0..5 {
            append_event(&ctx, ActivityEvent::now("e", "M01", format!("step {i}"))).unwrap();
        }
        let recent = read_recent_events(&ctx, 3).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].summary, "step 4");
        assert_eq!(recent[2].summary, "step 2");
    }

    #[test]
    fn append_event_best_effort_swallows_errors_with_warning() {
        // Unwritable target path: append_event should fail; the
        // best-effort wrapper converts that into Ok(None) so the
        // primary mutation can still proceed. We assert via a tmp
        // dir whose parent becomes a file (impossible to create a
        // child under) to provoke a write failure.
        let tmp = TempDir::new().unwrap();
        let blocker = tmp.path().join("not_a_dir");
        std::fs::write(&blocker, b"x").unwrap();
        let ctx = PlanContext {
            project_root: tmp.path().to_path_buf(),
            plan_dir: blocker.clone(),
        };
        let outcome = append_event_best_effort(&ctx, ActivityEvent::now("e", "M01", "x")).unwrap();
        assert!(
            outcome.is_none(),
            "best-effort must swallow the write failure"
        );
    }

    #[test]
    fn append_event_best_effort_unlocked_swallows_errors_with_warning() {
        // M180 F-02 regression: the in-lock best-effort variant
        // must mirror the lock-acquiring wrapper's swallow + warn
        // behavior. Without this, milestone lifecycle commands
        // (block / unblock / complete / set-lifecycle / spec-status
        // / create) propagate journal errors as command failures —
        // reporting a false rollback that violates AC-04.
        let tmp = TempDir::new().unwrap();
        let blocker = tmp.path().join("not_a_dir");
        std::fs::write(&blocker, b"x").unwrap();
        let ctx = PlanContext {
            project_root: tmp.path().to_path_buf(),
            plan_dir: blocker.clone(),
        };
        let outcome =
            append_event_best_effort_unlocked(&ctx, ActivityEvent::now("e", "M01", "x")).unwrap();
        assert!(
            outcome.is_none(),
            "in-lock best-effort must swallow the write failure (AC-04)"
        );
    }

    #[test]
    fn record_lifecycle_transition_returns_ok_when_journal_fails() {
        // M180 F-02 regression: the lifecycle helper MUST return
        // Ok(()) when the journal write fails. Returning Err would
        // propagate out of every milestone write seam (block,
        // complete, set-lifecycle, …) as a command failure even
        // though the primary mutation has already committed — the
        // false-rollback behavior AC-04 forbids.
        let tmp = TempDir::new().unwrap();
        let blocker = tmp.path().join("not_a_dir");
        std::fs::write(&blocker, b"x").unwrap();
        let ctx = PlanContext {
            project_root: tmp.path().to_path_buf(),
            plan_dir: blocker.clone(),
        };
        // Changed-state call (old != new) so the helper actually
        // tries to append; the unwritable plan_dir forces the
        // failure path.
        let result = record_lifecycle_transition(&ctx, "M01", "draft", "approved");
        assert!(
            result.is_ok(),
            "record_lifecycle_transition must not bubble journal errors (AC-04 / F-02): {result:?}"
        );
    }

    #[test]
    fn record_lifecycle_transition_no_op_when_state_unchanged() {
        // Same-state writes must be a no-op (AC-02).
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        record_lifecycle_transition(&ctx, "M01", "approved", "approved").unwrap();
        let log = load(&ctx).unwrap();
        assert!(log.is_empty(), "no-op transition must not append");
    }

    #[test]
    fn read_recent_events_limit_zero_returns_empty_against_real_plan() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        append_event(&ctx, ActivityEvent::now("e", "M01", "x")).unwrap();
        let recent = read_recent_events(&ctx, 0).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn lifecycle_event_emits_summary_in_arrow_form() {
        let e = lifecycle_event("M180", "approved", "in-progress");
        assert_eq!(e.r#type, "lifecycle-transition");
        assert_eq!(e.subject, "M180");
        assert!(e.summary.contains("approved → in-progress"));
    }

    #[test]
    fn lifecycle_event_for_creation_uses_created_label() {
        let e = lifecycle_event("M180", "", "draft");
        assert!(e.summary.starts_with("milestone created"));
    }

    #[test]
    fn validation_state_event_attaches_structured_data() {
        // Round-trip: prev_ok / prev_count / cur_ok / cur_count
        // survive a JSON serialize + parse without touching prose.
        let ev = validation_state_event(Some(false), Some(3), true, 0);
        let bytes = serde_json::to_vec(&ev).unwrap();
        let back: ActivityEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.r#type, "validation-state");
        let data = back.data.expect("data must survive round-trip");
        assert_eq!(data["prev_ok"], serde_json::json!(false));
        assert_eq!(data["prev_count"], serde_json::json!(3));
        assert_eq!(data["cur_ok"], serde_json::json!(true));
        assert_eq!(data["cur_count"], serde_json::json!(0));
        assert!(back.summary.contains("recovered"));
    }

    #[test]
    fn last_validation_state_reads_structured_data() {
        // The structured `data` payload is the canonical read path;
        // summary-parsing is only a legacy fallback.
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        append_event(&ctx, validation_state_event(Some(false), Some(2), true, 0)).unwrap();
        let got = last_validation_state(&ctx).unwrap();
        assert_eq!(got, Some((true, 0)));
    }

    #[test]
    fn last_validation_state_legacy_summary_fallback() {
        // A pre-data validation-state event (only summary) still
        // parses via the legacy fallback path so older journals
        // don't lose change-detection.
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        let mut legacy = ActivityEvent::now(
            "validation-state",
            "",
            "validation state errors (3 → 5 errors)",
        );
        // data stays None on purpose (legacy shape)
        legacy.data = None;
        append_event(&ctx, legacy).unwrap();
        let got = last_validation_state(&ctx).unwrap();
        assert_eq!(got, Some((false, 5)));
    }
}
