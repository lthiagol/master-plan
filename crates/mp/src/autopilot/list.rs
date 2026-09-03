//! M207 / AC-03: list autopilot sessions under the plan.
//!
//! Discovers `<plan_dir>/autopilot/<id>/session.json` directories and
//! returns a summary per session (id, status, last_updated). Errors
//! are non-fatal — a malformed sub-directory is skipped and the rest
//! of the list still surfaces, matching the chat-session list helper.

use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use crate::autopilot::session::{load_session_from, SessionStatus};
use crate::paths::PlanContext;

/// Summary row used by `mp autopilot session list`.
#[derive(Debug, Serialize, PartialEq)]
pub struct SessionListEntry {
    pub id: String,
    pub status: String,
    pub last_updated: String,
}

/// Return all autopilot sessions under the plan, sorted by id.
pub fn list_sessions(ctx: &PlanContext) -> Result<Vec<SessionListEntry>> {
    let autopilot = ctx.plan_dir.join("autopilot");
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&autopilot) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "read autopilot dir {}: {e}",
                autopilot.display()
            ));
        }
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        if !path.is_dir() {
            continue;
        }
        let session_file = path.join("session.json");
        if !session_file.is_file() {
            continue;
        }
        // Skip the malformed sub-directory non-fatally; the
        // session.json loader emits detailed errors but the list
        // helper is intentionally lossy.
        let Ok(session) = load_session_from(&session_file, &ctx.project_root) else {
            continue;
        };
        out.push(SessionListEntry {
            id: session.id,
            status: session_status_label(&session.status),
            last_updated: session.last_updated,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn session_status_label(status: &SessionStatus) -> String {
    status.as_str().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopilot::session::{save_session, sample_session_for_tests};
    use crate::paths::PlanContext;
    use tempfile::TempDir;

    fn ctx_in(dir: &PathBuf) -> PlanContext {
        PlanContext {
            project_root: dir.clone(),
            plan_dir: dir.join("master-plan"),
        }
    }

    #[test]
    fn list_returns_empty_when_no_autopilot_dir() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(&tmp.path().to_path_buf());
        let list = list_sessions(&ctx).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn list_sorts_by_id() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(&tmp.path().to_path_buf());
        // Insertion order is deliberately non-alphabetical.
        for id in ["zeta", "alpha", "mike"] {
            let s = sample_session_for_tests(id);
            save_session(&ctx, id, &s).unwrap();
        }
        let list = list_sessions(&ctx).unwrap();
        assert_eq!(
            list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "mike", "zeta"]
        );
    }

    #[test]
    fn list_skips_malformed_subdirs_nonfatally() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx_in(&tmp.path().to_path_buf());
        let s = sample_session_for_tests("good");
        save_session(&ctx, "good", &s).unwrap();
        // Drop a directory with no session.json — must be skipped.
        std::fs::create_dir_all(ctx.plan_dir.join("autopilot/empty")).unwrap();
        let list = list_sessions(&ctx).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "good");
    }
}