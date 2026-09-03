//! S0 / AC-01: `mp watch` startup preconditions.
//!
//! Before processing any milestone, `mp watch` must verify that the
//! runtime environment is wired:
//!
//! 1. `herdr` is on `PATH` (no auto-install; surfaces a clear error
//!    pointing the user at herdr.dev/docs/install — see M149 Q-03).
//! 2. The `[agent.runner]` and `[agent.coordinator]` config sections
//!    each have at least a `harness` field set. The harness value
//!    itself is validated at `mp config set` time; here we only check
//!    presence so a freshly-initialized project gets an actionable
//!    error rather than a silent no-op.
//! 3. The configured log file path is writable. The default is
//!    `<plan_dir>/.mp/watch.log`; if the user overrides it to an
//!    unwritable location we want to fail fast at startup, not 30
//!    minutes into a run.
//!
//! All checks produce structured entries; the overall `ok` is the
//! AND of per-check `ok` flags. Failures are aggregated and reported
//! together — the user should not have to fix-and-restart N times to
//! discover N problems.

use std::path::Path;

use serde::Serialize;

use crate::config::ProjectConfig;
use crate::doctor::command_on_path;
use crate::harness::AutoSetDecision;

#[derive(Debug, Clone, Serialize)]
pub struct PreconditionCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreconditionReport {
    pub ok: bool,
    pub checks: Vec<PreconditionCheck>,
}

impl PreconditionReport {
    pub fn failed(&self) -> Vec<&PreconditionCheck> {
        self.checks.iter().filter(|c| !c.ok).collect()
    }
}

/// Default log file path for `mp watch`: `<plan_dir>/.mp/watch.log`.
/// The `.mp/` dir is the standard mp-internal scratch location (also
/// used for session.json, etc.) and is the right default — it lives
/// alongside the plan, is gitignored by `mp init`, and is writable in
/// every profile.
pub fn default_log_path(plan_dir: &Path) -> std::path::PathBuf {
    plan_dir.join(".mp").join("watch.log")
}

/// Run all `mp watch` startup preconditions. Pure function over the
/// supplied config + log path; no side effects, no panics. Callers
/// (the watch command) decide how to surface failures (exit code,
/// JSON, human message).
pub fn check_preconditions(cfg: &ProjectConfig, log_path: &Path) -> PreconditionReport {
    let mut checks = vec![
        check_herdr_on_path(),
        check_herdr_cli_shape(),
        check_role_config("runner_config_present", "agent.runner", cfg.runner_config()),
        check_role_config(
            "coordinator_config_present",
            "agent.coordinator",
            cfg.coordinator_config(),
        ),
        check_log_path_writable(log_path),
    ];

    // M197 WP1 / AC-01: surface the harness auto-set decision in
    // the precondition report. The check is `ok=true` even when no
    // action is taken — the role-config-present checks above are
    // the real gate. This line exists so the operator always sees
    // the harness auto-set state in `mp watch`'s JSON output.
    checks.push(check_harness_auto_set(cfg));

    // M209 AC-05: run the legacy-to-autopilot role resolution for
    // the two roles that have legacy analogs. Surfaces conflicts
    // between the new `autopilot.roles.<role>.*` surface and the
    // legacy `agent.<role>.*` surface as typed precondition
    // failures so the operator sees them at startup, not mid-run.
    checks.push(check_role_config_resolution(
        cfg,
        "runner",
        crate::autopilot::role::Role::Runner,
    ));
    checks.push(check_role_config_resolution(
        cfg,
        "orchestrator",
        crate::autopilot::role::Role::Orchestrator,
    ));

    let ok = checks.iter().all(|c| c.ok);
    PreconditionReport { ok, checks }
}

/// M197 WP3 / AC-04: herdr CLI version + shape gate. The check
/// shells out to `herdr --version` and `herdr agent start --help`
/// to confirm the install exposes the `--kind` / `--pane` flags
/// the wp2 realignment relies on. When herdr is missing or
/// shape-incompatible, this check is the precondition that
/// surfaces the upgrade message — without it, the spawn would
/// fail later in `pane_split` / `spawn_pane` with a less
/// actionable error.
fn check_herdr_cli_shape() -> PreconditionCheck {
    let shape = crate::watch::detect_herdr_cli_default();
    PreconditionCheck {
        name: "herdr_cli_shape".to_string(),
        ok: shape.compatible,
        message: shape.message,
    }
}

/// M197 WP1 / AC-01 / F-10: precondition check that surfaces the
/// harness auto-set decision. The `ok` flag is `true` unconditionally;
/// the message carries the actual decision (auto-set, no-op, or
/// ambiguous). The real failure is upstream in
/// `runner_config_present` / `coordinator_config_present`.
///
/// F-10: the structured parts of the decision come from
/// [`crate::harness::decision_label`] (shared with `mp doctor`'s
/// `harness_auto_set` DoctorCheck). This surface wraps the label
/// in the precondition's tagged `harness auto-set: …` shape.
fn check_harness_auto_set(cfg: &ProjectConfig) -> PreconditionCheck {
    let installed = crate::harness::detect_installed_harnesses();
    let decision = crate::harness::auto_set_target(
        cfg.agent.runner.harness.as_deref(),
        cfg.agent.coordinator.harness.as_deref(),
        &installed,
    );
    let label = crate::harness::decision_label(
        cfg.agent.runner.harness.as_deref(),
        cfg.agent.coordinator.harness.as_deref(),
        &decision,
    );
    let message = match decision {
        AutoSetDecision::NoOp => {
            if cfg.agent.runner.harness.is_some() || cfg.agent.coordinator.harness.is_some() {
                format!("harness auto-set: noop ({label} already configured)")
            } else {
                format!(
                    "harness auto-set: noop ({label}; mp watch preconditions will surface the missing config below)"
                )
            }
        }
        AutoSetDecision::AutoSet { .. } => {
            format!("harness auto-set: {label} (single installed harness)")
        }
        AutoSetDecision::Ambiguous { .. } => {
            format!("harness auto-set: {label}")
        }
    };
    PreconditionCheck {
        name: "harness_auto_set".to_string(),
        ok: true,
        message,
    }
}

/// M197 WP1 / AC-01 / F-09: lazy auto-set fallback. Called by the watch
/// command before `check_preconditions` so a project that never
/// went through `mp init` (or whose harness was installed *after*
/// init) still gets a sensible default. Mutates `cfg` in place when
/// the decision is [`AutoSetDecision::AutoSet`]; leaves it alone
/// otherwise. The caller is responsible for persisting the mutated
/// config (the watch command calls
/// [`crate::store::write_config`] after a successful fallback).
///
/// Returns the decision so the caller can log it (e.g. a
/// `harness_auto_set` entry in `watch.log`) and surface it in
/// stdout / JSON output.
///
/// F-09: the apply step routes through
/// [`crate::harness::apply_auto_set_decision`] so the init-time
/// write path (which operates on raw `serde_json::Value` over the
/// profile template) and this lazy fallback (which operates on
/// the typed [`ProjectConfig`] struct) cannot drift their
/// understanding of what "auto-set" means.
pub fn try_lazy_auto_set(cfg: &mut ProjectConfig) -> AutoSetDecision {
    let installed = crate::harness::detect_installed_harnesses();
    let mut runner_h = cfg.agent.runner.harness.clone();
    let mut coord_h = cfg.agent.coordinator.harness.clone();
    let decision =
        crate::harness::auto_set_target(runner_h.as_deref(), coord_h.as_deref(), &installed);
    crate::harness::apply_auto_set_decision(&mut runner_h, &mut coord_h, &decision);
    cfg.agent.runner.harness = runner_h;
    cfg.agent.coordinator.harness = coord_h;
    decision
}

fn check_herdr_on_path() -> PreconditionCheck {
    let on_path = command_on_path("herdr");
    PreconditionCheck {
        name: "herdr_on_path".to_string(),
        ok: on_path,
        message: if on_path {
            "herdr resolves on PATH".to_string()
        } else {
            "herdr not found on PATH — install from https://herdr.dev/docs/install".to_string()
        },
    }
}

fn check_role_config(name: &str, label: &str, rc: &crate::config::RoleConfig) -> PreconditionCheck {
    // M151 S4: the precondition gate sources its accepted-set from
    // the v1 harness registry so the error message and the on-ramp
    // (`herdr integration install <name>`) live in one place. A
    // future harness added to the registry shows up here
    // automatically; until then `mp config set <role>.harness <new>`
    // is the supported hand-rail.
    match &rc.harness {
        Some(h) => match crate::harness::HarnessRegistry::v1().get(h) {
            Ok(_) => PreconditionCheck {
                name: name.to_string(),
                ok: true,
                message: format!("{label}.harness = {h}"),
            },
            Err(err) => PreconditionCheck {
                // `config_set` already rejects unknown harnesses, but
                // a hand-edited config can still smuggle one through.
                // Surface the registry's structured error verbatim so
                // the user sees the supported list and the on-ramp
                // hint in the same message they would get from
                // `mp agent harness start-command <name>`.
                name: name.to_string(),
                ok: false,
                message: format!("{label}.harness = {h}: {err}"),
            },
        },
        None => {
            let suggested = crate::harness::HarnessRegistry::v1().supported_names();
            PreconditionCheck {
                name: name.to_string(),
                ok: false,
                message: format!(
                    "{label}.harness is not set — run `mp config set {label}.harness <{}>`",
                    suggested.join("|")
                ),
            }
        }
    }
}

/// M209 AC-05: run the unified legacy-to-autopilot role resolution
/// at precondition time. Surfaces a typed diagnostic when the new
/// `autopilot.roles.<role>.*` surface and the legacy
/// `agent.<role>.*` surface disagree on `harness` or `model`, and
/// reports the resolved harness so `mp watch --dry-run` (and JSON
/// output) reflects what the spawn will actually use.
fn check_role_config_resolution(
    cfg: &ProjectConfig,
    label: &str,
    role: crate::autopilot::role::Role,
) -> PreconditionCheck {
    let check_name = format!("{label}_role_resolved");
    let autopilot_key = role.as_str();
    let ovr = cfg.autopilot.roles.get(autopilot_key);
    let legacy_runner = if matches!(role, crate::autopilot::role::Role::Runner) {
        Some(cfg.runner_config())
    } else {
        None
    };
    let legacy_coordinator = if matches!(role, crate::autopilot::role::Role::Orchestrator) {
        Some(cfg.coordinator_config())
    } else {
        None
    };
    match crate::autopilot::resolve_with_legacy_fallback(
        role,
        ovr,
        legacy_runner,
        legacy_coordinator,
    ) {
        Ok(resolved) => PreconditionCheck {
            name: check_name,
            ok: true,
            message: format!(
                "{label}.role resolved: harness={} skill={}",
                resolved.harness, resolved.skill
            ),
        },
        Err(err) => PreconditionCheck {
            name: check_name,
            ok: false,
            message: format!("{label}.role resolution failed: {err}"),
        },
    }
}

/// Verify the log file path is writable. Strategy: ensure the parent
/// directory exists (create it if missing), then probe-write a single
/// byte and remove the probe file. If the file already exists we leave
/// it alone (mp watch appends to it).
fn check_log_path_writable(log_path: &Path) -> PreconditionCheck {
    let parent = log_path.parent();
    let Some(parent) = parent else {
        return PreconditionCheck {
            name: "log_path_writable".to_string(),
            ok: false,
            message: format!("log path has no parent dir: {}", log_path.display()),
        };
    };

    if let Err(e) = std::fs::create_dir_all(parent) {
        return PreconditionCheck {
            name: "log_path_writable".to_string(),
            ok: false,
            message: format!("cannot create log parent dir {}: {e}", parent.display()),
        };
    }

    let probe = log_path.with_extension("probe");
    if let Err(e) = std::fs::write(&probe, b"") {
        return PreconditionCheck {
            name: "log_path_writable".to_string(),
            ok: false,
            message: format!("log path {} is not writable: {e}", log_path.display()),
        };
    }
    let _ = std::fs::remove_file(&probe);

    PreconditionCheck {
        name: "log_path_writable".to_string(),
        ok: true,
        message: format!("log path writable: {}", log_path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn empty_config() -> ProjectConfig {
        ProjectConfig::default()
    }

    fn config_with_harnesses(runner: &str, coordinator: &str) -> ProjectConfig {
        let mut cfg = ProjectConfig::default();
        cfg.agent.runner.harness = Some(runner.to_string());
        cfg.agent.coordinator.harness = Some(coordinator.to_string());
        cfg
    }

    #[test]
    fn missing_harnesses_fail_with_actionable_messages() {
        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join("watch.log");
        let report = check_preconditions(&empty_config(), &log);

        assert!(!report.ok, "empty config must fail preconditions");
        let failed = report.failed();
        let names: Vec<&str> = failed.iter().map(|c| c.name.as_str()).collect();
        assert!(
            names.contains(&"runner_config_present"),
            "runner config missing must be reported: {names:?}"
        );
        assert!(
            names.contains(&"coordinator_config_present"),
            "coordinator config missing must be reported: {names:?}"
        );
        // Each message should point at the mp config set command.
        for c in &failed {
            if c.name == "runner_config_present" || c.name == "coordinator_config_present" {
                assert!(
                    c.message.contains("mp config set"),
                    "message should suggest the fix: {}",
                    c.message
                );
            }
        }
    }

    #[test]
    fn harnesses_present_pass_role_checks() {
        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join("watch.log");
        let report = check_preconditions(&config_with_harnesses("opencode", "pi"), &log);

        let role_checks: Vec<_> = report
            .checks
            .iter()
            .filter(|c| c.name.ends_with("_config_present"))
            .collect();
        assert_eq!(role_checks.len(), 2);
        assert!(role_checks.iter().all(|c| c.ok));
    }

    #[test]
    fn log_path_writable_when_parent_creatable() {
        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join(".mp").join("watch.log");
        let report = check_preconditions(&config_with_harnesses("opencode", "opencode"), &log);
        let log_check = report
            .checks
            .iter()
            .find(|c| c.name == "log_path_writable")
            .unwrap();
        assert!(log_check.ok, "{}", log_check.message);
        // create_dir_all in the check should have produced the parent.
        assert!(tmp.path().join(".mp").is_dir());
    }

    #[test]
    fn log_path_rejects_root_relative_garbage() {
        // A path with no parent component — guard against the
        // `parent.is_none()` branch in `check_log_path_writable`.
        let report = check_preconditions(
            &config_with_harnesses("opencode", "opencode"),
            Path::new(""),
        );
        let log_check = report
            .checks
            .iter()
            .find(|c| c.name == "log_path_writable")
            .unwrap();
        assert!(!log_check.ok);
    }

    #[test]
    fn invalid_harness_value_in_hand_edited_config_is_surfaced() {
        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join("watch.log");
        let mut cfg = empty_config();
        // config_set rejects this, but a hand-edit could still produce
        // it — precondition check should not silently accept it.
        cfg.agent.runner.harness = Some("tmux".to_string());
        cfg.agent.coordinator.harness = Some("opencode".to_string());
        let report = check_preconditions(&cfg, &log);
        let runner_check = report
            .checks
            .iter()
            .find(|c| c.name == "runner_config_present")
            .unwrap();
        assert!(!runner_check.ok, "tmux harness must be rejected");
        let coord_check = report
            .checks
            .iter()
            .find(|c| c.name == "coordinator_config_present")
            .unwrap();
        assert!(coord_check.ok);
    }

    #[test]
    fn default_log_path_is_under_mp_subdir_of_plan_dir() {
        let plan_dir = Path::new("/tmp/some-plan");
        let log = default_log_path(plan_dir);
        assert!(log.ends_with(".mp/watch.log"), "got {}", log.display());
    }
}
