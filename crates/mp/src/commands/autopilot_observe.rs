//! M246 WP3: `mp autopilot wait` and `mp autopilot tail`.
//!
//! Both verbs are thin: the polling decision lives in
//! [`crate::autopilot::observe`] (where it is unit-tested against an
//! injected clock), and this module supplies the three real readers —
//! the run-state file, the plan's milestone record, and the activity
//! journal — plus the process plumbing.
//!
//! Exit codes follow the plan's convention: `wait` exits 0 when the
//! milestone reached its target and 1 otherwise, always emitting a
//! JSON body first so a caller can read the reason. `tail` exits 0
//! in both the plain and `--follow` cases; reaching the idle timeout
//! is a normal end to a follow, not an error.

use std::time::{Duration, Instant};

use anyhow::Result;

use crate::activity::ActivityEvent;
use crate::autopilot::drive::AutopilotRunState;
use crate::autopilot::observe::{self, TailCursor};
use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::paths::PlanContext;

/// Poll cadence for both verbs. Short enough that a chat operator sees
/// a terminal state promptly, long enough not to hammer the disk.
const POLL: Duration = Duration::from_millis(250);

/// `mp autopilot wait <id> --timeout <secs>`.
///
/// Blocks until the milestone reaches the lifecycle its run is
/// waiting for. The three on-disk sources are read on every poll:
/// `.mp/autopilot-run.state.json` for "is the run still going and
/// what is it aiming for", and the plan's milestone record for the
/// authoritative lifecycle.
pub(crate) fn cmd_autopilot_wait(
    ctx: &PlanContext,
    id: String,
    timeout_secs: u64,
    format: Fmt,
) -> Result<()> {
    let state_path = AutopilotRunState::path_for(&ctx.plan_dir);
    let milestone_path = milestone_file_for(ctx, &id);

    let report = observe::wait_for_milestone_with(
        || AutopilotRunState::load_from(&state_path).ok().flatten(),
        || current_lifecycle(&milestone_path),
        &id,
        Duration::from_secs(timeout_secs),
        || {
            // Real clock plus the poll interval: the injected seam
            // has no loop of its own, so the wrapper advances time by
            // one poll per call. `wait_for_milestone_with` reads the
            // clock at least once per iteration, so elapsed time
            // tracks real time closely enough for a 30-minute default
            // and stays exact for the short timeouts tests use.
            let now = Instant::now();
            std::thread::sleep(POLL);
            now + POLL
        },
    );

    emit(format, &report)?;
    if !report.reached {
        return Err(crate::ExitCode(1).into());
    }
    Ok(())
}

/// Resolve the on-disk file for a milestone id. Returns a path even
/// when the file is missing so the reader can report "no lifecycle"
/// rather than failing the whole command.
fn milestone_file_for(ctx: &PlanContext, id: &str) -> std::path::PathBuf {
    let numeric = id.trim_start_matches(['M', 'm']);
    ctx.milestones_dir().join(format!("{numeric}-"))
}

/// Read a milestone's lifecycle straight from its JSON record.
///
/// Returns `None` for a missing or unreadable file — a `wait` that
/// cannot see the milestone must report an honest `lifecycle: null`
/// rather than guessing.
fn current_lifecycle(path: &std::path::Path) -> Option<String> {
    // The on-disk name is `<id>-<slug>.json`, so the caller hands us a
    // `<id>-` prefix. A direct hit is rare but free; otherwise scan
    // for the first file carrying that prefix.
    if path.is_file() {
        if let Ok(m) = crate::store::load_milestone(path) {
            return Some(m.milestone.lifecycle);
        }
    }
    let parent = path.parent()?;
    let prefix = path.file_name()?.to_string_lossy().to_string();
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(parent)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("json")
                && p.file_name()
                    .map(|n| n.to_string_lossy().starts_with(&prefix))
                    .unwrap_or(false)
        })
        .collect();
    // Deterministic pick when a slug edit leaves two matching files.
    entries.sort();
    for candidate in entries {
        if let Ok(m) = crate::store::load_milestone(&candidate) {
            return Some(m.milestone.lifecycle);
        }
    }
    None
}

/// `mp autopilot tail <id> [--follow] [--since <ts>] [--idle <secs>]`.
///
/// Prints the journal events scoped to one milestone, oldest first,
/// one compact JSON object per line — a stream shape, so a caller can
/// read it line by line while it is still being written.
pub(crate) fn cmd_autopilot_tail(
    ctx: &PlanContext,
    id: String,
    follow: bool,
    since: Option<String>,
    idle_secs: u64,
) -> Result<()> {
    let journal = crate::activity::default_path(&ctx.plan_dir);
    let mut cursor = TailCursor::default();
    let idle_budget = Duration::from_secs(idle_secs);
    let mut since_last_event = Duration::ZERO;

    loop {
        let events: Vec<ActivityEvent> = crate::activity::load_from(&journal)
            .map(|log| log.events)
            .unwrap_or_default();

        let batch = observe::tail_batch(
            &events,
            &id,
            since.as_deref(),
            &mut cursor,
            follow,
            idle_budget,
            since_last_event,
        );

        for event in &batch.events {
            println!("{}", serde_json::to_string(event)?);
        }
        if !batch.events.is_empty() {
            since_last_event = Duration::ZERO;
        } else {
            since_last_event += POLL;
        }

        match batch.stop {
            // End of the journal and not following: done.
            Some(observe::TailStop::Eof) => return Ok(()),
            // Follow mode with nothing new for `--idle`: a normal
            // end, not a failure.
            Some(observe::TailStop::Idle) => return Ok(()),
            None => {
                if !follow {
                    return Ok(());
                }
                std::thread::sleep(POLL);
            }
        }
    }
}
