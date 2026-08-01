use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::json;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::harness::HarnessRegistry;
use crate::paths::PlanContext;
use crate::store;

pub(crate) fn cmd_agent_role(
    ctx: &PlanContext,
    role: Option<String>,
    clear: bool,
    format: Fmt,
) -> Result<()> {
    let session_path = ctx.plan_dir.join(".mp").join("session.json");

    if clear {
        if session_path.exists() {
            std::fs::remove_file(&session_path)?;
        }
        emit(
            format,
            &json!({"ok": true, "role": serde_json::Value::Null, "message": "role cleared"}),
        )?;
        return Ok(());
    }

    let role = match role {
        Some(r) if r == "mp-coordinator" || r == "coordinator" => "coordinator",
        Some(r) if r == "mp-runner" || r == "runner" => "runner",
        Some(r) => bail!(
            "unknown role: {} (known: coordinator, runner; aliases: mp-coordinator, mp-runner)",
            r
        ),
        None => bail!("role is required (e.g. mp agent role coordinator)"),
    };

    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let payload = json!({"role": role, "set_at": store::today()});
    std::fs::write(&session_path, serde_json::to_string_pretty(&payload)?)?;

    emit(format, &json!({"ok": true, "role": role}))?;
    Ok(())
}

// ─── M151: harness command registry surface ─────────────────────────────

/// JSON envelope for `mp agent harness list`.
#[derive(Debug, Serialize)]
struct HarnessListReport {
    harnesses: Vec<crate::harness::HarnessEntry>,
}

/// Print every entry the v1 registry knows about. The shape mirrors
/// `HarnessEntry`'s serde output so the list command and the
/// under-the-hood struct round-trip through one schema.
pub(crate) fn cmd_agent_harness_list(ctx: &PlanContext, format: Fmt) -> Result<()> {
    let _ = ctx; // ctx is required by the dispatch signature; the
                 // harness list does not depend on plan presence and intentionally
                 // works in fresh checkouts (a user querying the registry from a
                 // not-yet-`mp init`'d cwd should still see the v1 set).
    let reg = HarnessRegistry::v1();
    let report = HarnessListReport {
        harnesses: reg.iter().copied().collect(),
    };
    emit(format, &report)
}

/// JSON envelope for `mp agent harness start-command <name>`.
///
/// `model` and `thinking_level` carry the caller-supplied values
/// even when the registry dropped them (e.g. a `--thinking-level`
/// against a v1 harness that has no thinking flag). They are
/// skipped when `None` so the no-override base case emits only
/// `{id, argv}` — `argv` is the source of truth, these fields
/// are an echo.
#[derive(Debug, Serialize)]
struct HarnessStartCommandReport {
    id: String,
    argv: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_level: Option<String>,
}

/// Resolve the harness argv via the registry. Unknown harnesses
/// surface the same `HarnessError::Unsupported` message — the
/// command-line tool never panics and exits non-zero on bad input
/// (the dispatch in `main.rs` downcasts the error to a process
/// exit code). Optional `--model` / `--thinking-level` flags are
/// forwarded as-is; the registry decides whether each harness
/// actually carries the corresponding flag.
pub(crate) fn cmd_agent_harness_start_command(
    ctx: &PlanContext,
    name: &str,
    model: Option<String>,
    thinking_level: Option<String>,
    format: Fmt,
) -> Result<()> {
    let _ = ctx;
    let reg = HarnessRegistry::v1();
    let argv = reg.resolve_argv(name, model.as_deref(), thinking_level.as_deref())?;
    let report = HarnessStartCommandReport {
        id: name.to_string(),
        argv,
        model,
        thinking_level,
    };
    emit(format, &report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputFormat;

    #[test]
    fn harness_list_emits_three_entries_with_expected_shape() {
        // Suppress stdout — we only assert via the in-memory path.
        let report = HarnessListReport {
            harnesses: HarnessRegistry::v1().iter().copied().collect(),
        };
        let v = serde_json::to_value(&report).unwrap();
        let arr = v["harnesses"].as_array().expect("harnesses array");
        assert_eq!(arr.len(), 3, "v1 registry has exactly 3 entries");

        let names: Vec<&str> = arr.iter().map(|e| e["id"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["opencode", "pi", "cursor"]);

        // Cursor entry must carry both flags; pin the JSON keys.
        let cursor = arr
            .iter()
            .find(|e| e["id"] == "cursor")
            .expect("cursor in list");
        assert_eq!(cursor["model_flag"], "--model");
        assert_eq!(cursor["thinking_flag"], "--thinking");
    }

    #[test]
    fn harness_start_command_resolves_opencode_with_model() {
        let reg = HarnessRegistry::v1();
        let argv = reg
            .resolve_argv("opencode", Some("claude-opus-4"), None)
            .unwrap();
        let report = HarnessStartCommandReport {
            id: "opencode".to_string(),
            argv: argv.clone(),
            model: Some("claude-opus-4".to_string()),
            thinking_level: None,
        };
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["id"], "opencode");
        assert_eq!(v["argv"], json!(["opencode", "--model", "claude-opus-4"]));
        assert_eq!(v["model"], "claude-opus-4");
    }

    #[test]
    fn harness_start_command_handles_unknown_harness() {
        let reg = HarnessRegistry::v1();
        let result = reg.resolve_argv("claude-code", None, None);
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("claude-code") && msg.contains("opencode"),
            "unknown harness must name the offender + supported set: {msg}"
        );
    }

    #[test]
    fn output_format_default_is_json() {
        // Catches a regression that flips the default for readers
        // (the registry command is consumed by both humans via
        // `raul`-style surfaces and agents via JSON parsing — JSON
        // default is the only safe choice).
        assert_eq!(OutputFormat::default(), OutputFormat::Json);
    }

    // M151 ext-review F-02 (2026-07-14): the previous inline test
    // `harness_list_does_not_require_existing_plan` allocated a
    // TempDir + ctx_in(&dir) but never used them; the assertion
    // only hit HarnessRegistry::v1() directly, which is already
    // covered by the inline unit tests in harness/registry.rs and
    // by the `harness_list_works_in_a_fresh_init` integration
    // test in tests/agent_harness_cli.rs (which actually exercises
    // the CLI surface end-to-end). Dropped to remove the dead
    // setup.
}
