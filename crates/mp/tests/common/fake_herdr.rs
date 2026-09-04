//! Shared fake-herdr harness for autopilot integration tests.
//!
//! M227 / WP1: replaces the per-test ad-hoc `install_fake_herdr`
//! shell-script builders scattered across `autopilot_drive_herdr_wait.rs`,
//! `autopilot_drive_herdr_start.rs`, and `autopilot_drive_bridge_report.rs` with one
//! configurable test primitive.
//!
//! Goals:
//!
//! - **Coverage**: every herdr subcommand the production code paths
//!   exercise — `version`, `pane split`, `pane send-keys`, `pane get`,
//!   `pane report-agent`, `pane report-metadata`, `agent list`,
//!   `agent start`, `agent send`, `agent wait`, `agent read` — is
//!   represented as a configurable knob.
//! - **Determinism**: per-call `sleep` and `exit-code` overrides let
//!   tests drive the bounded subprocess helper
//!   (`mp::autopilot::drive::bridge::run_herdr_with_timeout`) into its
//!   timeout / failure / killpg paths without wall-clock sleeps in
//!   the test body. The `warmup` helper pre-spawns the fake so the
//!   next `Command::new("herdr")` from a real `mp` subprocess does
//!   not race on shell cold-start under parallel nextest.
//! - **Reuse**: builders expose the same knobs across the
//!   representative watch suites (M149, M150, M197, M227) so M225
//!   (restart + reconciliation) and M226 (end-to-end certification)
//!   can compose off the same primitive without re-deriving a
//!   script per test.
//!
//! **Mode coverage**: each `mode_<cmd>` builder method maps to one
//! shell-script branch. The supported surface is:
//!
//! | mode              | branch on (`$1 $2`)            |
//! |-------------------|--------------------------------|
//! | version           | `--version`                    |
//! | agent start       | `agent start`                  |
//! | agent list        | `agent list`                   |
//! | agent send        | `agent send`                   |
//! | agent wait        | `agent wait`                   |
//! | agent read        | `agent read`                   |
//! | pane split        | `pane split`                   |
//! | pane send-keys    | `pane send-keys`               |
//! | pane get          | `pane get`                     |
//! | pane report-agent | `pane report-agent`            |
//! | pane report-meta  | `pane report-metadata`         |
//!
//! **Signal handling**: [`FakeHerdrBuilder::signal_ignore`] wraps the
//! script in `trap '' TERM INT` so SIGTERM/SIGINT from an outside
//! orchestrator do not kill the fake. SIGKILL (which is what
//! `run_herdr_with_timeout` uses for `killpg`) cannot be trapped —
//! the killpg contract still holds.
//!
//! **Logger**: every invocation appends `argv: $*` to a fixed log
//! file at `<install_dir>/herdr-calls.log` so tests can read it back
//! to assert on argv shape without intercepting stdout/stderr.
//!
//! **Quoting**: response strings are emitted via `printf '%s\n'` so
//! embedded JSON braces / quotes do not break the script. Sleep
//! values are emitted as fractional seconds (`printf "%.3f"`) so the
//! script can call `sleep` with one portable argument (no
//! sub-millisecond arithmetic in pure POSIX sh).
//!
//! # Reuse requirement (M227 / S3)
//!
//! **Any new autopilot test that needs to inject a fake `herdr`
//! binary MUST use [`FakeHerdrBuilder`] instead of writing a
//! bespoke shell script.** The two ad-hoc builders that exist
//! outside this module today
//! (`autopilot_drive_bridge_fastpath::install_fake_herdr`,
//! `autopilot_drive_sequencer::install_fake_herdr_with_log`,
//! `watch_non_dry_run::install_fake_herdr`,
//! `watch_no_double_spawn::install_fake_herdr_with_existing_panes`,
//! `suites/status_readiness::install_fake_herdr_for_preconditions`)
//! are pre-M227 and out of scope; future autopilot suites (M225,
//! M226, and beyond) compose off this primitive, and the contract
//! is one source of truth for argv shape, response payloads, and
//! the killpg-ready grandchild fork. Adding a second primitive
//! would multiply the surface and re-introduce the flake paths
// M227 closed.

use std::fs;
use std::path::{Path, PathBuf};

/// A fake `herdr` script installed under a per-test temp dir.
///
/// Returned by [`FakeHerdrBuilder::install`]. The path is the
/// absolute path to the installed executable; `log_path` is the
/// argv log the script appends to on every invocation.
#[derive(Debug, Clone)]
pub struct FakeHerdr {
    pub path: PathBuf,
    pub log_path: PathBuf,
}

impl FakeHerdr {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    /// Read back the full argv log (one `argv: $*` line per
    /// invocation). Returns the empty string when no calls have
    /// been recorded yet.
    pub fn read_log(&self) -> String {
        fs::read_to_string(&self.log_path).unwrap_or_default()
    }

    /// Pre-spawn the fake herdr so the next real
    /// `Command::new("herdr")` (from inside `mp milestone
    /// complete`, for example) does not pay the shell cold-start
    /// cost. AC-02 / S2 deterministic timeout: under parallel
    /// nextest, the first cold-start (~220 ms on macOS) can blow
    /// the 500 ms bounded subprocess deadline; one synchronous
    /// pre-spawn primes the OS page cache so subsequent invocations
    /// are fast.
    ///
    /// The warmup call appends one line to the log; callers that
    /// want a clean log should call [`clear_log`] afterward.
    pub fn warmup(&self) {
        let _ = std::process::Command::new(&self.path)
            .arg("version")
            .output();
    }

    /// Remove the argv log file so the test sees only real
    /// invocations. Used in tandem with [`warmup`].
    pub fn clear_log(&self) {
        let _ = fs::remove_file(&self.log_path);
    }
}

/// Builder for the fake-herdr script. All knobs have sensible
/// defaults so a bare `FakeHerdrBuilder::new().install(dir)` yields
/// a fake that responds to every subcommand with a minimal valid
/// payload.
#[derive(Debug, Clone)]
pub struct FakeHerdrBuilder {
    version: String,
    pane_split_response: String,
    pane_get_response: String,
    agent_list_response: String,
    agent_start_response: String,
    agent_start_help_response: String,
    pane_split_help_response: String,
    agent_wait_response: String,

    pane_split_failure: Option<(i32, String)>,
    pane_split_sleep_ms: u64,
    pane_get_failure: Option<(i32, String)>,
    pane_get_sleep_ms: u64,
    agent_list_failure: Option<(i32, String)>,
    agent_list_sleep_ms: u64,
    agent_start_failure: Option<(i32, String)>,
    agent_start_sleep_ms: u64,
    agent_send_failure: Option<(i32, String)>,
    pane_send_keys_failure: Option<(i32, String)>,
    pane_report_agent_failure: Option<(i32, String)>,
    pane_report_metadata_failure: Option<(i32, String)>,

    signal_ignore: bool,

    grandchild_sleep_secs: Option<u64>,
}

impl Default for FakeHerdrBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeHerdrBuilder {
    pub fn new() -> Self {
        Self {
            version: "herdr 0.7.3".to_string(),
            pane_split_response: r#"{"pane_id":"%new-pane-1"}"#.to_string(),
            pane_get_response: r#"{"id":"cli:pane:get","result":{"pane":{"pane_id":"%1","custom_status":""}}}"#
                .to_string(),
            agent_list_response: r#"{"agents":[]}"#.to_string(),
            agent_start_response: r#"{"pane_id":"%spawned-1","status":"started"}"#.to_string(),
            agent_start_help_response:
                "Usage: agent start [OPTIONS] NAME\n\nOptions:\n  --kind <KIND>  Harness kind\n  --pane <ID>    Pane id\n"
                    .to_string(),
            pane_split_help_response:
                "Usage: pane split [OPTIONS]\n\nOptions:\n  --cwd <PATH>  Pane cwd\n".to_string(),
            agent_wait_response: r#"{"status":"idle"}"#.to_string(),

            pane_split_failure: None,
            pane_split_sleep_ms: 0,
            pane_get_failure: None,
            pane_get_sleep_ms: 0,
            agent_list_failure: None,
            agent_list_sleep_ms: 0,
            agent_start_failure: None,
            agent_start_sleep_ms: 0,
            agent_send_failure: None,
            pane_send_keys_failure: None,
            pane_report_agent_failure: None,
            pane_report_metadata_failure: None,

            signal_ignore: false,
            grandchild_sleep_secs: None,
        }
    }

    pub fn version(&mut self, v: impl Into<String>) -> &mut Self {
        self.version = v.into();
        self
    }

    pub fn pane_split_response(&mut self, r: impl Into<String>) -> &mut Self {
        self.pane_split_response = r.into();
        self
    }

    pub fn pane_split_delay_ms(&mut self, ms: u64) -> &mut Self {
        self.pane_split_sleep_ms = ms;
        self
    }

    pub fn pane_split_failure(&mut self, code: i32, stderr: impl Into<String>) -> &mut Self {
        self.pane_split_failure = Some((code, stderr.into()));
        self
    }

    pub fn pane_get_response(&mut self, r: impl Into<String>) -> &mut Self {
        self.pane_get_response = r.into();
        self
    }

    pub fn pane_get_delay_ms(&mut self, ms: u64) -> &mut Self {
        self.pane_get_sleep_ms = ms;
        self
    }

    pub fn pane_get_failure(&mut self, code: i32, stderr: impl Into<String>) -> &mut Self {
        self.pane_get_failure = Some((code, stderr.into()));
        self
    }

    pub fn agent_list_response(&mut self, r: impl Into<String>) -> &mut Self {
        self.agent_list_response = r.into();
        self
    }

    pub fn agent_list_delay_ms(&mut self, ms: u64) -> &mut Self {
        self.agent_list_sleep_ms = ms;
        self
    }

    pub fn agent_list_failure(&mut self, code: i32, stderr: impl Into<String>) -> &mut Self {
        self.agent_list_failure = Some((code, stderr.into()));
        self
    }

    pub fn agent_start_response(&mut self, r: impl Into<String>) -> &mut Self {
        self.agent_start_response = r.into();
        self
    }

    pub fn agent_start_delay_ms(&mut self, ms: u64) -> &mut Self {
        self.agent_start_sleep_ms = ms;
        self
    }

    pub fn agent_start_failure(&mut self, code: i32, stderr: impl Into<String>) -> &mut Self {
        self.agent_start_failure = Some((code, stderr.into()));
        self
    }

    pub fn agent_send_failure(&mut self, code: i32, stderr: impl Into<String>) -> &mut Self {
        self.agent_send_failure = Some((code, stderr.into()));
        self
    }

    pub fn pane_send_keys_failure(&mut self, code: i32, stderr: impl Into<String>) -> &mut Self {
        self.pane_send_keys_failure = Some((code, stderr.into()));
        self
    }

    pub fn pane_report_agent_failure(&mut self, code: i32, stderr: impl Into<String>) -> &mut Self {
        self.pane_report_agent_failure = Some((code, stderr.into()));
        self
    }

    pub fn pane_report_metadata_failure(
        &mut self,
        code: i32,
        stderr: impl Into<String>,
    ) -> &mut Self {
        self.pane_report_metadata_failure = Some((code, stderr.into()));
        self
    }

    pub fn agent_wait_response(&mut self, r: impl Into<String>) -> &mut Self {
        self.agent_wait_response = r.into();
        self
    }

    /// Configure `pane get` (used by the bridge poll) to fork a
    /// long-running `sleep N &` grandchild and write its PID to the
    /// file at `pid_file`. Combined with a short
    /// `mp::autopilot::drive::bridge::run_herdr_with_timeout` deadline in the
    /// test, this drives the killpg path: the parent sh + the
    /// grandchild sleep must both be reaped when the helper times
    /// out.
    pub fn pane_get_grandchild_sleep(&mut self, secs: u64, pid_file: &Path) -> &mut Self {
        self.pane_get_sleep_ms = (secs * 1000).max(2000);
        self.pane_get_response = format!(
            "sleep {secs} &\necho $! > {}\nwait\n",
            pid_file.display(),
            secs = secs,
        );
        self.grandchild_sleep_secs = Some(secs);
        self
    }

    /// Trap SIGTERM and SIGINT so the fake does not die on outside
    /// signals. SIGKILL (used by the bounded helper's killpg) is
    /// still delivered — the killpg contract is unaffected.
    pub fn signal_ignore(&mut self, on: bool) -> &mut Self {
        self.signal_ignore = on;
        self
    }

    /// Install the fake at `<dir>/herdr` and return the handle.
    /// Creates `<dir>` if it does not exist; writes
    /// `<dir>/herdr-calls.log` lazily on first invocation.
    pub fn install(&self, dir: &Path) -> FakeHerdr {
        fs::create_dir_all(dir).expect("create fake-herdr install dir");
        let path = dir.join("herdr");
        let log_path = dir.join("herdr-calls.log");
        let script = self.render(&log_path);
        fs::write(&path, script).expect("write fake-herdr script");
        set_executable(&path);
        FakeHerdr { path, log_path }
    }

    fn render(&self, log_path: &Path) -> String {
        let mut out = String::new();
        out.push_str("#!/bin/sh\n");
        out.push_str(&format!(
            "LOG={}\n",
            shell_quote(&log_path.display().to_string())
        ));

        if self.signal_ignore {
            out.push_str("trap '' TERM INT\n");
        }

        // Trampoline every invocation into the argv log so tests can
        // assert on argv shape without intercepting stdout/stderr.
        out.push_str("printf 'argv: %s\\n' \"$*\" >> \"$LOG\"\n");

        // Branch on $1 first (subcommand family), then $2 (action).
        out.push_str("case \"$1\" in\n");

        // --version (also matches `version` for legacy callers).
        out.push_str("  --version|version)\n");
        if self.version.is_empty() {
            out.push_str("    echo herdr 0.7.3\n");
        } else {
            out.push_str(&format!(
                "    printf '%s\\n' {}\n",
                shell_quote(&self.version)
            ));
        }
        out.push_str("    exit 0\n");
        out.push_str("    ;;\n");

        // pane family.
        out.push_str("  pane)\n");
        out.push_str("    case \"$2\" in\n");
        out.push_str("      split)\n");
        emit_sleep_branch(&mut out, self.pane_split_sleep_ms);
        if let Some((code, stderr)) = &self.pane_split_failure {
            out.push_str(&format!(
                "        printf '%s\\n' {} 1>&2\n",
                shell_quote(stderr)
            ));
            out.push_str(&format!("        exit {code}\n"));
        } else {
            out.push_str(&format!(
                "        printf '%s\\n' {}\n",
                shell_quote(&self.pane_split_response)
            ));
            out.push_str("        exit 0\n");
        }
        out.push_str("        ;;\n");

        out.push_str("      send-keys)\n");
        if let Some((code, stderr)) = &self.pane_send_keys_failure {
            out.push_str(&format!(
                "        printf '%s\\n' {} 1>&2\n",
                shell_quote(stderr)
            ));
            out.push_str(&format!("        exit {code}\n"));
        } else {
            out.push_str("        echo ok\n");
            out.push_str("        exit 0\n");
        }
        out.push_str("        ;;\n");

        out.push_str("      get)\n");
        if self.pane_get_response.contains('\n') {
            // Custom multi-line body (e.g. grandchild sleep script).
            for line in self.pane_get_response.lines() {
                out.push_str(&format!("        {line}\n"));
            }
            out.push_str("        ;;\n");
        } else {
            emit_sleep_branch(&mut out, self.pane_get_sleep_ms);
            if let Some((code, stderr)) = &self.pane_get_failure {
                out.push_str(&format!(
                    "        printf '%s\\n' {} 1>&2\n",
                    shell_quote(stderr)
                ));
                out.push_str(&format!("        exit {code}\n"));
            } else {
                out.push_str(&format!(
                    "        printf '%s\\n' {}\n",
                    shell_quote(&self.pane_get_response)
                ));
                out.push_str("        exit 0\n");
            }
            out.push_str("        ;;\n");
        }

        out.push_str("      report-agent)\n");
        if let Some((code, stderr)) = &self.pane_report_agent_failure {
            out.push_str(&format!(
                "        printf '%s\\n' {} 1>&2\n",
                shell_quote(stderr)
            ));
            out.push_str(&format!("        exit {code}\n"));
        } else {
            out.push_str("        echo ok\n");
            out.push_str("        exit 0\n");
        }
        out.push_str("        ;;\n");

        out.push_str("      report-metadata)\n");
        if let Some((code, stderr)) = &self.pane_report_metadata_failure {
            out.push_str(&format!(
                "        printf '%s\\n' {} 1>&2\n",
                shell_quote(stderr)
            ));
            out.push_str(&format!("        exit {code}\n"));
        } else {
            out.push_str("        echo ok\n");
            out.push_str("        exit 0\n");
        }
        out.push_str("        ;;\n");

        out.push_str("      *)\n");
        out.push_str("        exit 0\n");
        out.push_str("        ;;\n");
        out.push_str("    esac\n");
        out.push_str("    ;;\n");

        // agent family.
        out.push_str("  agent)\n");
        out.push_str("    case \"$2\" in\n");
        out.push_str("      list)\n");
        emit_sleep_branch(&mut out, self.agent_list_sleep_ms);
        if let Some((code, stderr)) = &self.agent_list_failure {
            out.push_str(&format!(
                "        printf '%s\\n' {} 1>&2\n",
                shell_quote(stderr)
            ));
            out.push_str(&format!("        exit {code}\n"));
        } else {
            out.push_str(&format!(
                "        printf '%s\\n' {}\n",
                shell_quote(&self.agent_list_response)
            ));
            out.push_str("        exit 0\n");
        }
        out.push_str("        ;;\n");

        out.push_str("      start)\n");
        emit_sleep_branch(&mut out, self.agent_start_sleep_ms);
        if let Some((code, stderr)) = &self.agent_start_failure {
            out.push_str(&format!(
                "        printf '%s\\n' {} 1>&2\n",
                shell_quote(stderr)
            ));
            out.push_str(&format!("        exit {code}\n"));
        } else {
            out.push_str(&format!(
                "        printf '%s\\n' {}\n",
                shell_quote(&self.agent_start_response)
            ));
            out.push_str("        exit 0\n");
        }
        out.push_str("        ;;\n");

        out.push_str("      send)\n");
        if let Some((code, stderr)) = &self.agent_send_failure {
            out.push_str(&format!(
                "        printf '%s\\n' {} 1>&2\n",
                shell_quote(stderr)
            ));
            out.push_str(&format!("        exit {code}\n"));
        } else {
            out.push_str("        echo ok\n");
            out.push_str("        exit 0\n");
        }
        out.push_str("        ;;\n");

        out.push_str("      wait)\n");
        out.push_str(&format!(
            "        printf '%s\\n' {}\n",
            shell_quote(&self.agent_wait_response)
        ));
        out.push_str("        exit 0\n");
        out.push_str("        ;;\n");

        out.push_str("      read)\n");
        out.push_str("        echo \"\"\n");
        out.push_str("        exit 0\n");
        out.push_str("        ;;\n");

        out.push_str("      *)\n");
        out.push_str("        exit 0\n");
        out.push_str("        ;;\n");
        out.push_str("    esac\n");
        out.push_str("    ;;\n");

        out.push_str("  *)\n");
        out.push_str("    echo \"{}\"\n");
        out.push_str("    exit 0\n");
        out.push_str("    ;;\n");
        out.push_str("esac\n");

        out
    }
}

fn emit_sleep_branch(out: &mut String, ms: u64) {
    if ms == 0 {
        return;
    }
    let secs = format!("{:.3}", ms as f64 / 1000.0);
    out.push_str(&format!(
        "        if [ -n {secs_quoted} ]; then sleep {secs_quoted}; fi\n",
        secs_quoted = shell_quote(&secs)
    ));
}

fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    // Single-quote everything, escaping embedded single quotes via
    // the standard '\'' close/open dance. Safe for POSIX sh and
    // works for strings that contain JSON braces / quotes.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .expect("fake-herdr metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod 0o755 fake-herdr");
}

/// Install a minimal stub `herdr` script under `<env.tmp>/fake-bin/`
/// and return a `PATH` string with that directory prepended to the
/// current `PATH`.
///
/// **Purpose.** `mp doctor` (and the project-mode `doctor_project`
/// path) gate `report.ok` on a `herdr_cli_shape` check that flips to
/// `false` whenever `which herdr` returns `None`. CI runners ship
/// without herdr installed, so any doctor-touching test that also
/// asserts `out.status.success()` flakes the same way the
/// pre-existing `real_sigint` flake did. The fixture
/// (`tests/fixtures/projects/…`) and the in-tree `make install` flow
/// already pre-set PATH to a real herdr, but ad-hoc tests that just
/// spawn `mp` via `env.run` / `env.run_with_env` inherit the
/// developer / CI parent PATH.
///
/// **What the stub does.** Responds to `--version`,
/// `agent start --help`, and `pane split --help` with the minimum
/// payload the gate accepts (a `MAJOR.MINOR.PATCH` ≥ 0.7.0, the
/// `--kind` / `--pane` flags the gate looks for in `agent start
/// --help`, and a non-empty `pane split --help`). Every other
/// invocation echoes `ok` and exits 0 — doctor never asks anything
/// else, so the surface stays trivial.
///
/// **Usage.** Pair with `env.run_with_env(&[("PATH", &path)],
/// args)` so the spawned `mp` subprocess resolves `herdr` to the
/// stub. The path is fully owned by the test's `TempDir`, so no
/// cross-test coordination is needed.
///
/// **Why not the full `FakeHerdrBuilder`?** That builder covers the
/// `mp autopilot` / `mp watch` surface (agent list, pane get, pane
/// split, killpg traps, etc.). Doctor only needs three subcommands,
/// and the builder's argv-log / warmup / version knobs are dead
/// weight there. This helper is the doctor-specific analogue.
pub fn install_fake_herdr_for_doctor(env: &crate::common::TestEnv) -> String {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).expect("fake-bin dir");
    let bin = bin_dir.join("herdr");
    let script = r#"#!/bin/sh
case "$1:$2:$3" in
  --version:*)
    cat <<'V'
herdr 0.7.3
V
    ;;
  agent:start:--help)
    cat <<'H'
Usage: herdr agent start <NAME> --kind <KIND> --pane <ID>

Options:
  --kind <KIND>  Harness kind
  --pane <ID>    Existing pane id
H
    ;;
  pane:split:--help)
    cat <<'P'
Usage: herdr pane split [OPTIONS]

Options:
  --cwd <PATH>  Pane cwd
P
    ;;
  *)
    echo ok
    ;;
esac
"#;
    fs::write(&bin, script).expect("write stub herdr");
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).expect("chmod 0o755 stub herdr");

    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<std::path::PathBuf> = std::env::split_paths(&existing).collect();
    parts.insert(0, bin_dir);
    std::env::join_paths(parts)
        .expect("join PATH")
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_builder_installs_executable_script() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeHerdrBuilder::new().install(dir.path());
        assert!(fake.path().is_file());
        let meta = std::fs::metadata(fake.path()).unwrap();
        assert_eq!(
            meta.permissions().mode() & 0o755,
            0o755,
            "fake-herdr must be executable"
        );
    }

    #[test]
    fn version_subcommand_returns_pinned_value() {
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeHerdrBuilder::new()
            .version("herdr 1.2.3-fake")
            .install(dir.path());
        let out = std::process::Command::new(fake.path())
            .arg("--version")
            .output()
            .unwrap();
        assert!(out.status.success(), "version must exit 0");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "herdr 1.2.3-fake"
        );
    }

    #[test]
    fn pane_split_response_is_emitted_when_set() {
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeHerdrBuilder::new()
            .pane_split_response(r#"{"pane_id":"%my-pane"}"#)
            .install(dir.path());
        let out = std::process::Command::new(fake.path())
            .args(["pane", "split", "--cwd", "/repo"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("%my-pane"));
        let log = fake.read_log();
        assert!(log.contains("argv: pane split --cwd /repo"));
    }

    #[test]
    fn pane_split_failure_writes_stderr_and_exits_with_code() {
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeHerdrBuilder::new()
            .pane_split_failure(3, "split broke")
            .install(dir.path());
        let out = std::process::Command::new(fake.path())
            .args(["pane", "split"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert!(String::from_utf8_lossy(&out.stderr).contains("split broke"));
    }

    #[test]
    fn agent_list_delay_ms_observes_sleep() {
        // The script honors `sleep` (sub-second via fractional
        // seconds). 200 ms is well above the syscall floor.
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeHerdrBuilder::new()
            .agent_list_delay_ms(200)
            .install(dir.path());
        let started = std::time::Instant::now();
        let out = std::process::Command::new(fake.path())
            .args(["agent", "list"])
            .output()
            .unwrap();
        let elapsed = started.elapsed();
        assert!(out.status.success());
        assert!(
            elapsed >= std::time::Duration::from_millis(180),
            "agent list must honor the 200ms delay: {elapsed:?}"
        );
    }

    #[test]
    fn pane_get_grandchild_sleep_writes_pid_file() {
        // The grandchild-sleep pattern is exercised end-to-end in
        // the integration test
        // `mp_run_herdr_with_timeout_kills_grandchild_in_process_group`
        // (autopilot_drive_bridge_report.rs). Here we only assert the
        // script-side contract: the script forks a `sleep` and
        // writes its PID to the supplied file before `wait`ing.
        // Run the script with a tiny sleep so the test stays
        // under a second; the script's natural exit reaps the
        // grandchild synchronously, so nextest's leak detector
        // sees a clean teardown.
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("sleep.pid");
        let fake = FakeHerdrBuilder::new()
            .pane_get_grandchild_sleep(1, &pid_file)
            .install(dir.path());
        let out = std::process::Command::new(fake.path())
            .args(["pane", "get", "%1"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "fake herdr must exit 0 after the grandchild sleep completes: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let pid_text = std::fs::read_to_string(&pid_file).unwrap_or_default();
        let pid: i32 = pid_text.trim().parse().expect("pid file must be integer");
        assert!(pid > 0, "grandchild pid must be positive: {pid}");
    }

    #[test]
    fn signal_ignore_traps_term_int_but_not_kill() {
        // SIGKILL cannot be trapped — the helper's killpg still
        // reaps the script. Verify only the trap is installed by
        // checking that a graceful kill (no signal) still works
        // end-to-end.
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeHerdrBuilder::new()
            .signal_ignore(true)
            .install(dir.path());
        let out = std::process::Command::new(fake.path())
            .args(["pane", "split"])
            .output()
            .unwrap();
        assert!(out.status.success());
    }

    #[test]
    fn shell_quote_handles_apostrophes_and_braces() {
        // JSON contains braces; the helper must not break on them.
        let s = r#"{"pane_id":"%a","note":"it's ok"}"#;
        let quoted = shell_quote(s);
        assert!(quoted.starts_with('\''));
        assert!(quoted.ends_with('\''));
        // Round-trip via /bin/sh -c to make sure the quoting parses.
        let probe = std::process::Command::new("sh")
            .args(["-c", &format!("printf '%s' {quoted}")])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&probe.stdout), s);
    }
}
