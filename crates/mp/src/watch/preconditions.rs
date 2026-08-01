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
    let checks = vec![
        check_herdr_on_path(),
        check_role_config("runner_config_present", "agent.runner", cfg.runner_config()),
        check_role_config(
            "coordinator_config_present",
            "agent.coordinator",
            cfg.coordinator_config(),
        ),
        check_log_path_writable(log_path),
    ];

    let ok = checks.iter().all(|c| c.ok);
    PreconditionReport { ok, checks }
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
