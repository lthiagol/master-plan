use anyhow::Result;
use serde::Serialize;
use serde_json::json;

use crate::cli::OutputFormat as Fmt;
use crate::model::MilestoneFile;
use crate::paths::{self, PlanContext};
use crate::projection;
use crate::store;
use crate::track_kind;
use crate::validate;

pub(crate) fn emit(format: Fmt, json_value: &impl serde::Serialize) -> Result<()> {
    match format {
        Fmt::Json | Fmt::Raw => {
            // `Raw` = verbatim on-disk JSON for `show`/`track show` and GraphViz DOT for
            // `graph`; those commands special-case before reaching here. For any other
            // structured value, `Raw` falls back to pretty JSON (same path as `Json`).
            println!("{}", serde_json::to_string_pretty(json_value)?);
        }
    }
    Ok(())
}

/// Emit `report` on stdout, then exit with code 1 when `ok` is false.
/// Single home for the "report first, then maybe fail" pattern shared by
/// `config set`, `config set --dry-run`, `config validate`, and `doctor`
/// (M197 F-11). Adding a new consumer means calling this helper.
pub(crate) fn emit_and_exit_on_fail<T: Serialize>(format: Fmt, report: &T, ok: bool) -> Result<()> {
    emit(format, report)?;
    if !ok {
        return Err(crate::ExitCode(1).into());
    }
    Ok(())
}

pub(crate) fn emit_value(format: Fmt, value: &serde_json::Value, fields: &[String]) -> Result<()> {
    if fields.is_empty() {
        emit(format, value)
    } else {
        let projected = projection::project_fields(value, fields)?;
        emit(format, &projected)
    }
}

/// Mutable-flavoured counterpart. Reads `value` via `projection` (which
/// takes `&Value`) so this only needs `&mut` at the call site; the inner
/// projection layer does not mutate. Exists for callers that compose a
/// `serde_json::Value` in place before handing it to projection (M112 S2 —
/// the typed/raw merge in `show milestone --fields`).
pub(crate) fn emit_value_mut(
    format: Fmt,
    value: &mut serde_json::Value,
    fields: &[String],
) -> Result<()> {
    if fields.is_empty() {
        emit(format, value)
    } else {
        let projected = projection::project_fields(value, fields)?;
        emit(format, &projected)
    }
}

/// Like [`emit_value`] but accepts any serializable. When `--fields` is empty
/// the value is emitted directly (preserving struct field order, which keeps
/// JSON-shape golden tests stable). When `--fields` is set the value is
/// converted to a `serde_json::Value` for projection (key order there is not
/// part of any stability contract — projection is an explicit slice).
pub(crate) fn emit_fields<T: serde::Serialize>(
    format: Fmt,
    value: &T,
    fields: &[String],
) -> Result<()> {
    if fields.is_empty() {
        emit(format, value)
    } else {
        let v = serde_json::to_value(value)?;
        let projected = projection::project_fields(&v, fields)?;
        emit(format, &projected)
    }
}

pub(crate) fn emit_gate_failure(format: Fmt, errors: Vec<validate::ValidationIssue>) -> Result<()> {
    let report = validate::ValidationReport {
        ok: false,
        errors,
        warnings: vec![],
        l5_audit: None,
    };
    emit(format, &report)?;
    Err(anyhow::Error::new(crate::ExitCode(2)))
}

pub(crate) fn find_first_pending_track_item(
    ctx: &PlanContext,
) -> Result<Option<serde_json::Value>> {
    for &tk in &track_kind::TrackKind::ALL {
        let kind = tk.as_str();
        if let Ok(t) = store::load_track(ctx, kind) {
            if let Some(item) = t
                .items
                .iter()
                .find(|i| i.status == "pending" || i.status == "in-progress")
            {
                return Ok(Some(json!({
                    "track": { "kind": kind, "title": t.track.title },
                    "item": item,
                })));
            }
        }
    }
    Ok(None)
}

pub(crate) fn read_evidence(
    evidence: Option<String>,
    evidence_file: Option<std::path::PathBuf>,
) -> Result<Option<String>> {
    if let Some(path) = evidence_file {
        let root = std::env::var_os("MP_PROJECT")
            .or_else(|| std::env::var_os("MPH_PROJECT"))
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok());
        let content = crate::json_input::read_json_payload_in(Some(&path), None, root.as_deref())?;
        return Ok(Some(content));
    }
    Ok(evidence)
}

/// M111 S6: shell-parse pre-flight check. Runs `sh -n` against the candidate
/// string and returns a structured warning if the parse fails. Warn-only —
/// the value is still accepted so authors can stage awkward strings and fix
/// them later. M106/M110/M117 own the ac_verify.rs runner that actually
/// executes the string at `mp milestone complete` time; this pre-flight is
/// authoring-time only.
///
/// Behaviour:
/// - `None` input → no warning (nothing to validate).
/// - Empty string → no warning (treated as a deliberately blank value).
/// - Otherwise wraps the candidate in `set -e; <cmd>` so compound shell
///   syntax (pipes, redirects, `&&`) is parsed the same way the verifier
///   runs it. `sh -n` is universal on POSIX — no plugin specific to bash or
///   zsh features, so the gate stays portable (M110 macOS portability scope).
pub(crate) fn shell_parse_preflight(candidate: &str) -> Option<serde_json::Value> {
    if candidate.is_empty() {
        return None;
    }
    let wrapped = format!("set -e; {candidate}");
    // `sh -n` parses without executing; on parse failure it writes a
    // diagnostic to stderr and exits non-zero. We capture stderr so we can
    // quote it in the warning. Avoid relying on `sh -c "$wrapped"` because
    // double-quote escape semantics would mask parse errors that would
    // surface at completion time.
    let probe = std::process::Command::new("sh")
        .arg("-n")
        .arg("-c")
        .arg(&wrapped)
        .output();
    let probe = match probe {
        Ok(o) => o,
        Err(_) => {
            // Can't even spawn `sh` (e.g. exotically stripped container).
            // Surface the absence but keep the author moving.
            return Some(serde_json::json!({
                "warning": "verification-preflight skipped",
                "reason": "sh -n could not be spawned",
                "command": candidate,
            }));
        }
    };
    if probe.status.success() {
        None
    } else {
        let stderr = String::from_utf8_lossy(&probe.stderr).trim().to_string();
        Some(serde_json::json!({
            "warning": "verification shell-parse failed",
            "command": candidate,
            "stderr": stderr,
            "exit_code": probe.status.code().unwrap_or(-1),
            "hint": "run mp validate or `sh -n -c 'set -e; <cmd>'` to inspect; \
                     the value is still written (warn-not-reject)",
        }))
    }
}

/// M177 S2: write-time hygiene nudge when a verification/tests value
/// classifies as prose via [`crate::ac_verify::looks_like_prose`] and is
/// not already `manual:`-prefixed. Warn-not-reject — the write still
/// succeeds; authors (and agents in `--yes` mode) get a structured
/// signal to re-prefix with `manual: `.
pub(crate) fn prose_verification_warn(candidate: &str) -> Option<serde_json::Value> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.to_ascii_lowercase().starts_with("manual:") {
        return None;
    }
    if !crate::ac_verify::looks_like_prose(trimmed) {
        return None;
    }
    Some(serde_json::json!({
        "warning": "non-runnable verification string",
        "command": candidate,
        "classified_as": "manual",
        "hint": "prefix with 'manual: ' so the complete gate skips shell execution; \
                 the value is still written (warn-not-reject)",
        "suggested": format!("manual: {trimmed}"),
    }))
}

pub(crate) fn milestone_summary(m: &MilestoneFile) -> serde_json::Value {
    json!({
        "id": m.milestone.id,
        "display": paths::display_milestone_id(&m.milestone.id),
        "title": m.milestone.title,
        "slug": m.milestone.slug,
        // M144: surface the canonical M100 lifecycle + its transition
        // timestamp so CLI consumers that read this projection can
        // avoid the stale-by-design spec_status / execution_status pair.
        "lifecycle": m.effective_lifecycle(),
        "lifecycle_at": m.milestone.lifecycle_at,
        // Legacy fields kept for the migration window.
        "spec_status": m.milestone.spec_status,
        "execution_status": m.milestone.execution_status,
    })
}
