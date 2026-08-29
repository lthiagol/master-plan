//! S3 / AC-02, AC-04: herdr agent start abstraction.
//!
//! herdr is the agent multiplexer that owns pane lifecycle. This
//! module is mp's only seam against the `herdr` CLI — the rest of
//! `mp watch` (state machine, sequencer, prompts) calls into here.
//!
//! M197 WP2 / AC-03: the spawn shape is the herdr 0.7.x
//! `pane create → agent start` two-step. `herdr agent start` now
//! takes a `--kind <KIND>` (one of the harness kinds herdr knows
//! about) and a `--pane <ID>` that points at an already-created
//! pane; cwd is set on the pane via `herdr pane split --cwd
//! <PATH>`, not on the agent start argv. The previous
//! `agent start <name> --cwd <root> -- <harness argv>` shape is
//! gone — adapting to the current upstream API is the
//! forward-looking choice (per M197 DD-01 "herdr alignment
//! strategy"). The matchstick `--` separator and explicit harness
//! argv are no longer needed: herdr's `--kind` is a closed enum,
//! and per-harness flags (model, thinking) are not exposed in the
//! v1 surface; the harness registry still serves as the
//! harness-kind single source of truth.
//!
//! Layering:
//! - Pure helpers (no I/O): [`resolve_harness_kind`],
//!   [`build_pane_split_args`], [`build_start_args`],
//!   [`find_existing_pane`], [`parse_pane_id_from_start_output`],
//!   [`pane_label_for`]. Unit tested in this file.
//! - I/O wrappers: [`pane_split`], [`spawn_pane`], [`list_panes`],
//!   [`ensure_pane`]. These shell out to a `herdr` binary path;
//!   integration tests inject a fake script via `PATH` to verify
//!   the argv shape.
//!
//! Pane reuse (AC-04): [`ensure_pane`] lists existing panes first
//! and returns the matching one when present. The same runner pane
//! is reused across N sequential milestones without restart. The
//! label carries a session counter (`role-<role>-<N>`) so a
//! recreated pane is distinguishable from the prior one in the
//! herdr sidebar; v1 always uses N=1 and S8 owns any increment
//! logic.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::RoleConfig;

/// The two roles `mp watch` drives. S3 keeps it to runner/coordinator;
/// the L5 session-boundary discipline (review vs re-review) is owned
/// by the sequencer (S8), not by this layer.
///
/// Serialized as a kebab-case string so the watch state file
/// (`watch.state.json` — M152) round-trips cleanly via serde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Runner,
    Coordinator,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Runner => "runner",
            Role::Coordinator => "coordinator",
        }
    }
}

/// Per-role herdr pane label. Format: `role-<role>-<N>`. N defaults to
/// 1; the counter is owned by the sequencer (S8) and increments when a
/// pane is recreated mid-session. For S3 the single-`1` shape is
/// enough — it is what herdr displays in the sidebar and what
/// `find_existing_pane` matches against.
pub fn pane_label_for(role: Role, n: u32) -> String {
    format!("role-{}-{}", role.label(), n.max(1))
}

/// Default pane counter for S3 (always 1). S8 may grow this.
pub const DEFAULT_PANE_N: u32 = 1;

/// M197 WP2 / AC-03: resolve the harness kind passed to
/// `herdr agent start --kind <KIND>`. Prefers the explicit
/// `harness` config field; falls back to `opencode` to match
/// legacy M149 behavior. The harness kind is the closed enum
/// herdr accepts (opencode / pi / cursor / ...); the registry
/// validates it before this layer is called so a typo in config
/// never reaches the wire.
pub fn resolve_harness_kind(rc: &RoleConfig) -> String {
    rc.harness
        .clone()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "opencode".to_string())
}

/// Build the argv for `herdr pane split --cwd <PATH>`. Pure; tested
/// directly. Returns owned `String`s so callers can hand them to
/// `Command::args` without lifetime gymnastics. `--cwd` is set
/// here (not on the agent start call) because herdr 0.7.x takes
/// the cwd as a pane property, not an agent property.
pub fn build_pane_split_args(cwd: &Path) -> Vec<String> {
    vec![
        "pane".into(),
        "split".into(),
        "--cwd".into(),
        cwd.to_string_lossy().into_owned(),
    ]
}

/// Build the argv for `herdr agent start <NAME> --kind <KIND>
/// --pane <PANE_ID>`. Pure; tested directly. The 0.7.x shape
/// replaces the M149-era `--cwd <root> -- <harness argv>` form.
pub fn build_start_args(label: &str, kind: &str, pane_id: &str) -> Vec<String> {
    vec![
        "agent".into(),
        "start".into(),
        label.into(),
        "--kind".into(),
        kind.into(),
        "--pane".into(),
        pane_id.into(),
    ]
}

/// A herdr pane handle returned to the watch loop. `reused=true`
/// means an existing pane with the same label was found and no new
/// pane was spawned (AC-04 pane reuse).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaneHandle {
    pub label: String,
    pub pane_id: String,
    pub reused: bool,
}

/// Find a pane by label in the JSON output of `herdr agent list`.
/// Returns the pane's target id when present. Pure over the supplied
/// JSON text so unit tests inject fixtures.
///
/// Accepts both `{"agents": [...]}` envelopes and bare `[...]` shapes
/// (herdr has shipped both across versions) and tolerates `id` /
/// `pane_id` / `target` keys for the pane identifier.
pub fn find_existing_pane(label: &str, herdr_list_json: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(herdr_list_json).ok()?;
    let agents_arr = parsed
        .get("agents")
        .and_then(|v| v.as_array())
        .or_else(|| parsed.as_array())?;
    for agent in agents_arr {
        let name = agent
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| agent.get("label").and_then(|v| v.as_str()))?;
        if name == label {
            let id = agent
                .get("pane_id")
                .and_then(|v| v.as_str())
                .or_else(|| agent.get("id").and_then(|v| v.as_str()))
                .or_else(|| agent.get("target").and_then(|v| v.as_str()))?;
            return Some(id.to_string());
        }
    }
    None
}

/// Parse the pane id out of herdr's `pane split` or `agent start`
/// output. herdr's output format varies across versions (JSON
/// status blob, plain "started `<name>` pane=`<id>`" line). This
/// function tries JSON first, then a conservative regex-lite
/// scan, then falls back to `None` (caller uses the label as a
/// fallback target — herdr accepts the label as a target alias).
pub fn parse_pane_id_from_start_output(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        for key in ["pane_id", "id", "target", "pane"] {
            if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
                return Some(s.to_string());
            }
        }
        if let Some(obj) = v.get("agent").and_then(|x| x.as_object()) {
            for key in ["pane_id", "id", "target"] {
                if let Some(s) = obj.get(key).and_then(|x| x.as_str()) {
                    return Some(s.to_string());
                }
            }
        }
        if let Some(obj) = v.get("pane").and_then(|x| x.as_object()) {
            for key in ["pane_id", "id", "target"] {
                if let Some(s) = obj.get(key).and_then(|x| x.as_str()) {
                    return Some(s.to_string());
                }
            }
        }
    }
    // Best-effort text scan: "pane `<id>`" / "pane_id=`<id>`" /
    // "started ... pane=`<id>`". Herdr output isn't a stable
    // contract; the label fallback in `spawn_pane` covers the gap
    // when this returns None.
    if let Some(idx) = trimmed.rfind("pane=") {
        let rest = &trimmed[idx + "pane=".len()..];
        let id: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != ',')
            .collect();
        if !id.is_empty() {
            return Some(id);
        }
    }
    None
}

// ─── I/O wrappers (shell out to a herdr binary path) ─────────────────────────

/// Path to the `herdr` binary. Resolved once at watch startup via
/// [`which_herdr`]; callers pass the resulting `PathBuf` into
/// [`spawn_pane`] / [`list_panes`] / [`ensure_pane`].
pub fn which_herdr() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("herdr");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run `herdr agent list --format json` and return its stdout. Empty
/// string on failure (so `find_existing_pane` is a no-op when herdr
/// is unreachable — `ensure_pane` then falls through to spawn).
pub fn list_panes(herdr_bin: &Path) -> Result<String> {
    let out = Command::new(herdr_bin)
        .args(["agent", "list", "--format", "json"])
        .output()
        .with_context(|| format!("failed to spawn {} agent list", herdr_bin.display()))?;
    if !out.status.success() {
        bail!(
            "herdr agent list failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// M197 WP3 / AC-04: structured spawn diagnostics. Returned by
/// the herdr I/O wrappers when `pane split` or `agent start`
/// fails. The state machine converts a `SpawnFailure` into a
/// `spawn_error` log entry and a `RunOutcome::SpawnFailed`
/// terminal kind. Carrying the argv + stdout + stderr + exit
/// code as first-class fields is the whole point — operators
/// diagnose a launch failure from the watch log alone, without
/// rerunning with extra logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnFailure {
    /// Short command tag. `"pane split"` or `"agent start"`. The
    /// matching log entry's `command` field.
    pub command: String,
    /// The full argv the herdr layer tried to run. Persisted to
    /// the `spawn_error` log entry as `argv`.
    pub argv: Vec<String>,
    /// `Command::Output::status.code()` on Unix. `None` when the
    /// process was killed by a signal (e.g. timeout from
    /// `killpg`).
    pub exit_code: Option<i32>,
    /// Captured stdout. May be empty; never `None`.
    pub stdout: String,
    /// Captured stderr. May be empty; never `None`.
    pub stderr: String,
}

impl std::fmt::Display for SpawnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "herdr {} failed (exit {:?}): {}",
            self.command, self.exit_code, self.stderr
        )
    }
}

impl std::error::Error for SpawnFailure {}

/// M197 WP3 / AC-04: extract a [`SpawnFailure`] from an
/// `anyhow::Error` chain. The I/O wrappers return
/// `anyhow::Error::new(SpawnFailure { ... })` so the structured
/// fields survive the `.context("...")` chains the state machine
/// layers on top. The state machine uses this to decide whether
/// to emit a `spawn_error` log entry (and whether the sequencer
/// should map the failure to `RunOutcome::SpawnFailed`).
///
/// Walks the error chain via `anyhow::Error::chain` looking for
/// the first error that downcasts to [`SpawnFailure`]. This makes
/// the helper tolerant of `.context("...")` wrappers at the call
/// site (the production `pane_split` / `spawn_pane` exec-failure
/// arms use `.context()` to add operator-readable context; the
/// earlier `downcast_ref`-only implementation silently returned
/// `None` when the wrapper was layered on, which masked the
/// binary-missing failure mode behind a generic `stale` state).
///
/// External review F-03 clarified that [`SpawnFailure`] is
/// auto-`Send + Sync` (all fields are `String` / `Vec<String>` /
/// `Option<i32>`).
pub fn extract_spawn_failure(err: &anyhow::Error) -> Option<SpawnFailure> {
    err.chain()
        .find_map(|e| e.downcast_ref::<SpawnFailure>().cloned())
}

/// M197 WP2 / AC-03: create a fresh pane via `herdr pane split
/// --cwd <PATH>`. Returns the new pane's id (parsed from
/// herdr's JSON / text output; falls back to a synthetic `pane-N`
/// id when the output is unparseable so callers always get a
/// stable handle to use as `--pane <ID>` for the next step).
///
/// On failure (herdr exits non-zero, or the binary itself
/// cannot be exec'd) returns a [`SpawnFailure`] with the
/// command / argv / exit_code / stdout / stderr populated. The
/// state machine converts this to a `spawn_error` log entry and
/// a `RunOutcome::SpawnFailed` terminal kind.
pub fn pane_split(herdr_bin: &Path, cwd: &Path) -> Result<String> {
    let args = build_pane_split_args(cwd);
    let out = match Command::new(herdr_bin).args(&args).output() {
        Ok(out) => out,
        Err(e) => {
            return Err(anyhow::Error::new(SpawnFailure {
                command: "pane split".into(),
                argv: args.clone(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!(
                    "failed to exec {}: {e} (is the herdr binary on PATH and executable?)",
                    herdr_bin.display()
                ),
            })
            .context("herdr pane split exec failure"));
        }
    };
    if !out.status.success() {
        return Err(anyhow::Error::new(SpawnFailure {
            command: "pane split".into(),
            argv: args.clone(),
            exit_code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    if let Some(id) = parse_pane_id_from_start_output(&stdout) {
        return Ok(id);
    }
    // Synthetic fallback so the watch loop always has a target id
    // to send to the next `herdr agent start`. Real herdr always
    // emits a parseable id; the fallback is here so a hostile or
    // very-old herdr binary does not stall the watch driver.
    Ok("pane-?".to_string())
}

/// Run `herdr agent start <NAME> --kind <KIND> --pane <PANE_ID>`.
/// Returns the parsed `PaneHandle` on success. When the pane id
/// can't be parsed from herdr's output, falls back to the label
/// as the target id (herdr accepts agent labels as targets in
/// subsequent commands).
///
/// On failure returns a [`SpawnFailure`]; see [`pane_split`] for
/// the conversion contract.
pub fn spawn_pane(herdr_bin: &Path, label: &str, kind: &str, pane_id: &str) -> Result<PaneHandle> {
    let args = build_start_args(label, kind, pane_id);
    let out = match Command::new(herdr_bin).args(&args).output() {
        Ok(out) => out,
        Err(e) => {
            return Err(anyhow::Error::new(SpawnFailure {
                command: "agent start".into(),
                argv: args.clone(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!(
                    "failed to exec {}: {e} (is the herdr binary on PATH and executable?)",
                    herdr_bin.display()
                ),
            })
            .context("herdr agent start exec failure"));
        }
    };
    if !out.status.success() {
        return Err(anyhow::Error::new(SpawnFailure {
            command: "agent start".into(),
            argv: args.clone(),
            exit_code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let pane_id = parse_pane_id_from_start_output(&stdout).unwrap_or_else(|| label.to_string());
    Ok(PaneHandle {
        label: label.to_string(),
        pane_id,
        reused: false,
    })
}

/// Top-level pane lifecycle entry: list first, reuse if a pane with
/// the same label exists, otherwise create a fresh pane and start
/// the agent inside it. Implements the AC-04 "reuse across
/// milestones" contract; the label counter (N) is owned by the
/// caller so the sequencer can increment it.
pub fn ensure_pane(
    herdr_bin: &Path,
    role: Role,
    pane_n: u32,
    rc: &RoleConfig,
    cwd: &Path,
) -> Result<PaneHandle> {
    let label = pane_label_for(role, pane_n);

    let list_json = list_panes(herdr_bin).unwrap_or_default();
    if let Some(existing_id) = find_existing_pane(&label, &list_json) {
        return Ok(PaneHandle {
            label,
            pane_id: existing_id,
            reused: true,
        });
    }

    // M197 WP2 / AC-03: 0.7.x two-step spawn — create a fresh
    // pane with the right cwd, then start the agent inside that
    // pane. herdr 0.7.x does NOT accept `--cwd` on agent start;
    // cwd is a pane property.
    let kind = resolve_harness_kind(rc);
    let pane_id = pane_split(herdr_bin, cwd)?;
    spawn_pane(herdr_bin, &label, &kind, &pane_id)
}

// ─── S4: prompt delivery with readiness gate ─────────────────────────────────

/// Tune the readiness wait before [`send_prompt`] delivers text. The
/// opencode herdr bridge reports `idle` once the harness has booted;
/// we block on that signal so a freshly-spawned pane does not drop the
/// first prompt on the floor.
#[derive(Debug, Clone, Copy)]
pub struct ReadinessOptions {
    /// Max time to wait for `herdr agent-status` to report `idle`
    /// before giving up. 0 = no timeout (block forever — useful in
    /// tests with mocked herdr).
    pub timeout_ms: u64,
    /// Poll interval for the readiness loop. Defaults to 200ms in
    /// production; tests pass 1ms.
    pub poll_interval_ms: u64,
}

impl Default for ReadinessOptions {
    fn default() -> Self {
        Self {
            timeout_ms: 60_000,
            poll_interval_ms: 200,
        }
    }
}

/// Run `herdr agent wait <target> --status idle --timeout MS`. Returns
/// Ok(()) when the harness reports idle within the timeout; Err with
/// a clear message otherwise. The opencode bridge never reports `done`
/// (verified in `~/.config/opencode/plugins/herdr-agent-state.js`), so
/// `idle` is the right readiness signal — `working` fires mid-milestone.
pub fn wait_for_readiness(
    herdr_bin: &Path,
    pane: &PaneHandle,
    opts: &ReadinessOptions,
) -> Result<()> {
    wait_for_readiness_with(herdr_bin, pane, opts, Instant::now)
}

/// Internal: takes a `now` closure so tests can drive the timeout
/// without sleeping. Production callers use [`wait_for_readiness`].
pub fn wait_for_readiness_with(
    herdr_bin: &Path,
    pane: &PaneHandle,
    opts: &ReadinessOptions,
    mut now: impl FnMut() -> Instant,
) -> Result<()> {
    let start = now();
    let timeout = Duration::from_millis(opts.timeout_ms);
    let poll = Duration::from_millis(opts.poll_interval_ms.max(1));
    loop {
        // M152 S4: bail on graceful shutdown. The drive loop already
        // checks between iterations; this check covers the readiness
        // gate (where the spawn-to-prompt path blocks first) so a
        // Ctrl-C during initial spawn also exits cleanly.
        if crate::watch::shutdown_requested() {
            bail!("graceful shutdown requested");
        }

        let status = read_agent_status(herdr_bin, pane).unwrap_or_else(|_| "unknown".to_string());
        if status == "idle" {
            return Ok(());
        }
        if opts.timeout_ms > 0 && now().duration_since(start) >= timeout {
            bail!(
                "harness readiness timeout ({}ms): agent-status='{}' on pane '{}'. \
                 The harness may still be booting or the bridge may not be installed.",
                opts.timeout_ms,
                status,
                pane.pane_id
            );
        }

        // Interruptible sleep: same pattern as
        // `wait_for_lifecycle_with` — caps shutdown latency at
        // ~100ms instead of `poll_interval_ms`. The test
        // `real_sigint_during_watch_run_exits_zero_and_flushes_state`
        // pins this.
        let mut remaining = poll;
        let slice = Duration::from_millis(100);
        while !remaining.is_zero() {
            if crate::watch::shutdown_requested() {
                bail!("graceful shutdown requested");
            }
            let chunk = remaining.min(slice);
            std::thread::sleep(chunk);
            remaining = remaining.saturating_sub(chunk);
        }
    }
}

/// Run `herdr agent send <target> <text>` followed by
/// `herdr pane send-keys <pane> Enter`. The `agent send` half writes
/// literal text to the pane input line; the `pane send-keys Enter`
/// submits it. Splitting the two is the herdr convention for prompting
/// an agent (per `herdr agent send --help`).
pub fn deliver_prompt(herdr_bin: &Path, pane: &PaneHandle, text: &str) -> Result<()> {
    let send_out = Command::new(herdr_bin)
        .args(["agent", "send", &pane.pane_id, text])
        .output()
        .with_context(|| format!("failed to spawn {} agent send", herdr_bin.display()))?;
    if !send_out.status.success() {
        bail!(
            "herdr agent send failed (exit {:?}): {}",
            send_out.status.code(),
            String::from_utf8_lossy(&send_out.stderr)
        );
    }
    let enter_out = Command::new(herdr_bin)
        .args(["pane", "send-keys", &pane.pane_id, "Enter"])
        .output()
        .with_context(|| format!("failed to spawn {} pane send-keys", herdr_bin.display()))?;
    if !enter_out.status.success() {
        // Review finding #12: partial-failure cleanup. The prompt
        // text is now sitting in the pane input line but never
        // submitted. Send a Ctrl-C to clear the pending input so a
        // subsequent deliver_prompt doesn't concatenate with the
        // stuck text. Failure of the cleanup itself is non-fatal —
        // the caller still gets the original error.
        let _ = Command::new(herdr_bin)
            .args(["pane", "send-keys", &pane.pane_id, "C-c"])
            .output();
        bail!(
            "herdr pane send-keys Enter failed (exit {:?}): {}. Attempted Ctrl-C cleanup.",
            enter_out.status.code(),
            String::from_utf8_lossy(&enter_out.stderr)
        );
    }
    Ok(())
}

/// Send a prompt with the readiness gate: wait for idle, then deliver.
/// Composite of [`wait_for_readiness`] + [`deliver_prompt`]. This is
/// the canonical entry point the state machine (S7) calls.
pub fn send_prompt(
    herdr_bin: &Path,
    pane: &PaneHandle,
    text: &str,
    opts: &ReadinessOptions,
) -> Result<()> {
    wait_for_readiness(herdr_bin, pane, opts)?;
    deliver_prompt(herdr_bin, pane, text)
}

/// Run `herdr agent read <target> --lines N --format text`. Returns
/// the recent pane output for log capture / post-completion inspection.
pub fn read_output(herdr_bin: &Path, pane: &PaneHandle, lines: u32) -> Result<String> {
    let out = Command::new(herdr_bin)
        .args([
            "agent",
            "read",
            &pane.pane_id,
            "--lines",
            &lines.to_string(),
            "--format",
            "text",
        ])
        .output()
        .with_context(|| format!("failed to spawn {} agent read", herdr_bin.display()))?;
    if !out.status.success() {
        bail!(
            "herdr agent read failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run `herdr agent wait <target> --status <status>` (single-shot,
/// no internal polling — herdr's `wait` blocks on the socket until
/// the status matches or its own timeout fires). Used as a liveness
/// probe from S5's stall detection.
pub fn read_agent_status(herdr_bin: &Path, pane: &PaneHandle) -> Result<String> {
    // Use a short timeout so this returns quickly with the current
    // status rather than blocking. 0 timeout means "return current
    // state immediately" per herdr's `agent wait` semantics.
    let out = Command::new(herdr_bin)
        .args([
            "agent",
            "wait",
            &pane.pane_id,
            "--status",
            "idle",
            "--timeout",
            "0",
        ])
        .output()
        .with_context(|| format!("failed to spawn {} agent wait", herdr_bin.display()))?;
    // herdr's wait exits 0 when the status matched, non-zero otherwise.
    // We don't care about the exit code here — we want the *current*
    // status string. Parse it from stdout when JSON-shaped, else
    // synthesize from the exit code.
    let stdout = String::from_utf8_lossy(&out.stdout);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if let Some(s) = v
            .get("status")
            .and_then(|x| x.as_str())
            .or_else(|| v.get("agent_status").and_then(|x| x.as_str()))
        {
            return Ok(s.to_string());
        }
    }
    Ok(if out.status.success() {
        "idle".to_string()
    } else {
        "working".to_string()
    })
}

// ─── S5: lifecycle completion detection ──────────────────────────────────────

use std::time::{Duration, Instant};

/// The lifecycle transitions mp watch drives. Each variant is the
/// post-transition state we wait for, not the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LifecycleTarget {
    /// approved → in-progress (runner claimed & started executing).
    InProgress,
    /// in-progress → self-reviewed (runner done; round-1 self-review filed).
    SelfReviewed,
    /// self-reviewed → reviewed (coordinator external review filed).
    Reviewed,
    /// reviewed → complete (auto-promote once findings resolve).
    Complete,
}

impl LifecycleTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            LifecycleTarget::InProgress => "in-progress",
            LifecycleTarget::SelfReviewed => "self-reviewed",
            LifecycleTarget::Reviewed => "reviewed",
            LifecycleTarget::Complete => "complete",
        }
    }

    /// Parse a target from the on-disk lifecycle string. Returns
    /// `None` for states mp watch does not actively drive toward
    /// (e.g. `draft`, `approved` itself).
    pub fn from_lifecycle(s: &str) -> Option<Self> {
        match s {
            "in-progress" => Some(Self::InProgress),
            "self-reviewed" => Some(Self::SelfReviewed),
            "reviewed" => Some(Self::Reviewed),
            "complete" => Some(Self::Complete),
            _ => None,
        }
    }
}

/// Outcome of a [`wait_for_lifecycle_with`] call. The Ok shape lets
/// the state machine decide whether to advance, retry, or escalate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitOutcome {
    /// Lifecycle reached the target state.
    Reached,
    /// Lifecycle advanced past the target (e.g. we waited for
    /// `in-progress` but the milestone is already `self-reviewed`).
    /// Treated as success — the next loop iteration picks up from
    /// the actual state.
    AdvancedPast,
}

/// Tuning knobs for [`wait_for_lifecycle_with`].
#[derive(Debug, Clone, Copy)]
pub struct WaitOptions {
    /// Poll interval for the lifecycle reader (default 1000ms per
    /// the polling-strategy design decision).
    pub poll_interval_ms: u64,
    /// Stall timeout: if the agent-status string does not change for
    /// this many milliseconds, flag the agent as hung and return Err.
    /// 0 = disable stall detection (tests, infinite waits).
    pub stall_timeout_ms: u64,
}

impl Default for WaitOptions {
    fn default() -> Self {
        Self {
            poll_interval_ms: 1_000,
            stall_timeout_ms: 1_800_000, // 30 min default
        }
    }
}

/// Poll the lifecycle reader until `target` is met or the stall
/// timeout fires. The `read_lifecycle` closure returns the current
/// lifecycle string (e.g. via `mp show milestone <id> --fields
/// milestone.lifecycle`); `read_agent_status` returns the current
/// herdr agent-status string for liveness detection.
///
/// `now` is injected so tests can drive the loop without real sleeps.
#[allow(clippy::too_many_arguments)]
pub fn wait_for_lifecycle_with<L, S, N>(
    mut read_lifecycle: L,
    target: LifecycleTarget,
    mut read_agent_status: S,
    opts: &WaitOptions,
    mut now: N,
) -> Result<WaitOutcome>
where
    L: FnMut() -> Result<String>,
    S: FnMut() -> Result<String>,
    N: FnMut() -> Instant,
{
    let target_str = target.as_str();
    let poll = Duration::from_millis(opts.poll_interval_ms.max(1));

    let mut last_status_change = now();
    let mut prev_status = String::new();

    loop {
        // M152 S4: at the top of each iteration (and just before the
        // `thread::sleep` below), check whether a graceful
        // shutdown was requested. Returning `Err` here surfaces the
        // shutdown signal up the stack — `drive_milestone` /
        // `SystemDriveOps::wait_for_lifecycle` translate that into
        // the `DriveOutcome::Shutdown` variant. The integration test
        // pins this in `tests/watch_signal.rs::real_sigint_during_
        // watch_run_exits_zero_and_flushes_state`.
        if crate::watch::shutdown_requested() {
            bail!("graceful shutdown requested");
        }

        let lifecycle = read_lifecycle().unwrap_or_default();
        if lifecycle == target_str {
            return Ok(WaitOutcome::Reached);
        }
        // If the lifecycle has already advanced past the target, count
        // it as success — the state machine will pick up from the real
        // state on its next iteration.
        if lifecycle_advanced_past(&lifecycle, target) {
            return Ok(WaitOutcome::AdvancedPast);
        }

        // Re-check after the (possibly blocking) read_lifecycle
        // subprocess — `mp show milestone` can take seconds on a
        // cold filesystem; we want shutdown observed immediately.
        if crate::watch::shutdown_requested() {
            bail!("graceful shutdown requested");
        }

        let status = read_agent_status().unwrap_or_else(|_| "unknown".to_string());
        if status != prev_status {
            prev_status = status;
            last_status_change = now();
        } else if opts.stall_timeout_ms > 0 {
            let elapsed = now().duration_since(last_status_change);
            if elapsed >= Duration::from_millis(opts.stall_timeout_ms) {
                bail!(
                    "agent appears hung: agent-status='{}' unchanged for {}ms, \
                     lifecycle='{}' (target='{}')",
                    prev_status,
                    opts.stall_timeout_ms,
                    lifecycle,
                    target_str
                );
            }
        }

        // Replace the unconditional `thread::sleep` with a
        // interruptible version that polls the shutdown flag at
        // 100ms granularity. This caps the worst-case shutdown
        // latency at ~100ms instead of the configured poll
        // interval (default 1000ms) — important for live
        // SIGINT-driven exits.
        let mut remaining = poll;
        let slice = Duration::from_millis(100);
        while !remaining.is_zero() {
            if crate::watch::shutdown_requested() {
                bail!("graceful shutdown requested");
            }
            let chunk = remaining.min(slice);
            std::thread::sleep(chunk);
            remaining = remaining.saturating_sub(chunk);
        }
    }
}

/// Production wrapper that reads lifecycle via `mp` and agent-status
/// via `herdr`. Uses real `Instant::now` + `thread::sleep`. The
/// `project_root` is passed as cwd to the `mp` subprocess so plan
/// discovery works regardless of the watch process's own cwd.
pub fn wait_for_lifecycle(
    mp_bin: &Path,
    herdr_bin: &Path,
    project_root: &Path,
    milestone_id: &str,
    pane: &PaneHandle,
    target: LifecycleTarget,
    opts: &WaitOptions,
) -> Result<WaitOutcome> {
    wait_for_lifecycle_with(
        || read_lifecycle_via_mp(mp_bin, project_root, milestone_id),
        target,
        || read_agent_status(herdr_bin, pane),
        opts,
        Instant::now,
    )
}

/// Run `mp show milestone <id> --fields milestone.lifecycle` and
/// return the bare lifecycle string. `project_root` is supplied as
/// cwd so plan discovery works from any watch process location.
/// Public so tests / library callers can wrap it without a binary
/// spawn.
pub fn read_lifecycle_via_mp(
    mp_bin: &Path,
    project_root: &Path,
    milestone_id: &str,
) -> Result<String> {
    let out = Command::new(mp_bin)
        .current_dir(project_root)
        .args([
            "show",
            "milestone",
            milestone_id,
            "--fields",
            "milestone.lifecycle",
            "--format",
            "json",
        ])
        .output()
        .with_context(|| format!("failed to spawn {} show milestone", mp_bin.display()))?;
    if !out.status.success() {
        bail!(
            "mp show milestone {} failed (exit {:?}): {}",
            milestone_id,
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).context("mp show milestone returned non-JSON")?;
    let lifecycle = v
        .get("milestone")
        .and_then(|m| m.get("lifecycle"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("mp show milestone response missing milestone.lifecycle"))?;
    Ok(lifecycle.to_string())
}

/// Returns true when `current` is past `target` in the canonical
/// progression approved → in-progress → self-reviewed → reviewed →
/// complete. Used by [`wait_for_lifecycle_with`] to treat over-shot
/// transitions as success.
pub fn lifecycle_advanced_past(current: &str, target: LifecycleTarget) -> bool {
    let order = |s: &str| -> usize {
        match s {
            "approved" => 0,
            "in-progress" => 1,
            "self-reviewed" => 2,
            "reviewed" => 3,
            "complete" => 4,
            _ => 0,
        }
    };
    order(current) > order(target.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RoleConfig;

    fn rc_with_harness(h: &str) -> RoleConfig {
        RoleConfig {
            harness: Some(h.to_string()),
            ..Default::default()
        }
    }

    // M197 WP2 / AC-03: the herdr 0.7.x spawn shape drops the
    // explicit `command` argv — herdr's `--kind` is a closed
    // enum, and per-harness flags (model, thinking) are not
    // exposed in the v1 surface. The harness registry still
    // validates the kind before this layer is called. So the
    // resolve-argv tests are replaced by `resolve_harness_kind`
    // tests below.

    #[test]
    fn resolve_harness_kind_uses_config_when_set() {
        assert_eq!(
            resolve_harness_kind(&rc_with_harness("opencode")),
            "opencode"
        );
        assert_eq!(resolve_harness_kind(&rc_with_harness("pi")), "pi");
        assert_eq!(resolve_harness_kind(&rc_with_harness("cursor")), "cursor");
    }

    #[test]
    fn resolve_harness_kind_defaults_to_opencode_when_unset() {
        let rc = RoleConfig::default();
        assert_eq!(resolve_harness_kind(&rc), "opencode");
    }

    #[test]
    fn resolve_harness_kind_ignores_explicit_command_field() {
        // The `command` field predated herdr 0.7.x; the new shape
        // does not consume it. Carrying it in config is harmless
        // (a future per-harness flag translator may use it) but
        // `resolve_harness_kind` only reads `harness`.
        let mut rc = rc_with_harness("opencode");
        rc.command = Some(vec!["my-runner".into(), "--flag".into()]);
        assert_eq!(resolve_harness_kind(&rc), "opencode");
    }

    #[test]
    fn build_pane_split_args_carry_cwd() {
        let args = build_pane_split_args(Path::new("/repo"));
        assert_eq!(
            args,
            vec![
                "pane".to_string(),
                "split".to_string(),
                "--cwd".to_string(),
                "/repo".to_string(),
            ]
        );
    }

    #[test]
    fn build_start_args_shape_matches_herdr_cli() {
        let args = build_start_args("role-runner-1", "opencode", "%3");
        assert_eq!(
            args,
            vec![
                "agent".to_string(),
                "start".to_string(),
                "role-runner-1".to_string(),
                "--kind".to_string(),
                "opencode".to_string(),
                "--pane".to_string(),
                "%3".to_string(),
            ]
        );
    }

    #[test]
    fn build_start_args_passes_kind_through_unchanged() {
        // The harness kind is a closed enum on herdr's side; mp
        // passes it through without rewriting. Validating the
        // kind against the registry is the caller's job
        // (preconditions gate).
        assert!(build_start_args("label", "pi", "%1").contains(&"pi".to_string()));
        assert!(build_start_args("label", "cursor", "%2").contains(&"cursor".to_string()));
    }

    #[test]
    fn pane_label_includes_role_and_counter() {
        assert_eq!(pane_label_for(Role::Runner, 1), "role-runner-1");
        assert_eq!(pane_label_for(Role::Coordinator, 1), "role-coordinator-1");
        assert_eq!(pane_label_for(Role::Runner, 2), "role-runner-2");
    }

    #[test]
    fn pane_label_counter_clamps_to_minimum_one() {
        assert_eq!(pane_label_for(Role::Runner, 0), "role-runner-1");
    }

    #[test]
    fn find_existing_pane_matches_envelope_shape() {
        let json = r#"{"agents": [
            {"name": "role-runner-1", "pane_id": "%5"},
            {"name": "role-coordinator-1", "id": "%7"}
        ]}"#;
        assert_eq!(
            find_existing_pane("role-runner-1", json),
            Some("%5".to_string())
        );
        assert_eq!(
            find_existing_pane("role-coordinator-1", json),
            Some("%7".to_string())
        );
        assert_eq!(find_existing_pane("role-runner-2", json), None);
    }

    #[test]
    fn find_existing_pane_matches_bare_array_shape() {
        let json = r#"[
            {"label": "role-runner-1", "target": "%9"}
        ]"#;
        assert_eq!(
            find_existing_pane("role-runner-1", json),
            Some("%9".to_string())
        );
    }

    #[test]
    fn find_existing_pane_returns_none_on_invalid_json() {
        assert_eq!(find_existing_pane("role-runner-1", "not json"), None);
    }

    #[test]
    fn parse_start_output_json_with_pane_id() {
        let out = r#"{"pane_id": "%12", "status": "started"}"#;
        assert_eq!(parse_pane_id_from_start_output(out), Some("%12".into()));
    }

    #[test]
    fn parse_start_output_json_with_nested_agent() {
        let out = r#"{"agent": {"id": "agent-7"}, "ok": true}"#;
        assert_eq!(parse_pane_id_from_start_output(out), Some("agent-7".into()));
    }

    #[test]
    fn parse_start_output_text_with_pane_equals() {
        let out = "started role-runner-1 pane=%15 cwd=/repo";
        assert_eq!(parse_pane_id_from_start_output(out), Some("%15".into()));
    }

    #[test]
    fn parse_start_output_returns_none_on_unrecognized_shape() {
        assert_eq!(parse_pane_id_from_start_output("hello world"), None);
        assert_eq!(parse_pane_id_from_start_output(""), None);
    }

    // ─── S4 + S5 unit tests ───────────────────────────────────────────────────

    // ─── S4 + S5 unit tests ───────────────────────────────────────────────────
    // (placeholder kept intentionally blank: pure-function cases for
    // S4/S5 live in this module; behavioral cases that need a fake
    // herdr binary live in tests/watch_herdr_wait.rs.)

    #[test]
    fn lifecycle_target_roundtrips_str() {
        assert_eq!(LifecycleTarget::InProgress.as_str(), "in-progress");
        assert_eq!(LifecycleTarget::SelfReviewed.as_str(), "self-reviewed");
        assert_eq!(LifecycleTarget::Reviewed.as_str(), "reviewed");
        assert_eq!(LifecycleTarget::Complete.as_str(), "complete");
    }

    #[test]
    fn lifecycle_target_from_lifecycle_parses_known_states() {
        assert_eq!(
            LifecycleTarget::from_lifecycle("in-progress"),
            Some(LifecycleTarget::InProgress)
        );
        assert_eq!(LifecycleTarget::from_lifecycle("draft"), None);
        assert_eq!(LifecycleTarget::from_lifecycle("approved"), None);
    }

    #[test]
    fn lifecycle_advanced_past_orders_canonical_progression() {
        assert!(lifecycle_advanced_past(
            "self-reviewed",
            LifecycleTarget::InProgress
        ));
        assert!(lifecycle_advanced_past(
            "complete",
            LifecycleTarget::Reviewed
        ));
        assert!(!lifecycle_advanced_past(
            "in-progress",
            LifecycleTarget::SelfReviewed
        ));
        // Equal state is not "advanced past" — the equality branch
        // handles reaching the target.
        assert!(!lifecycle_advanced_past(
            "self-reviewed",
            LifecycleTarget::SelfReviewed
        ));
    }

    #[test]
    fn wait_for_lifecycle_returns_reached_when_target_already_met() {
        let opts = WaitOptions {
            poll_interval_ms: 1,
            stall_timeout_ms: 0,
        };
        // Use a real clock; the function returns on the first poll.
        let outcome = wait_for_lifecycle_with(
            || Ok("self-reviewed".to_string()),
            LifecycleTarget::SelfReviewed,
            || Ok("idle".to_string()),
            &opts,
            Instant::now,
        )
        .unwrap();
        assert_eq!(outcome, WaitOutcome::Reached);
    }

    #[test]
    fn wait_for_lifecycle_treats_overshoot_as_advanced_past() {
        // Lifecycle reader immediately returns a state past the target.
        let opts = WaitOptions {
            poll_interval_ms: 1,
            stall_timeout_ms: 0,
        };
        let outcome = wait_for_lifecycle_with(
            || Ok("complete".to_string()),
            LifecycleTarget::InProgress,
            || Ok("idle".to_string()),
            &opts,
            Instant::now,
        )
        .unwrap();
        assert_eq!(outcome, WaitOutcome::AdvancedPast);
    }

    #[test]
    fn wait_for_lifecycle_polls_until_target_reached() {
        let opts = WaitOptions {
            poll_interval_ms: 1,
            stall_timeout_ms: 0,
        };
        let counter = std::sync::atomic::AtomicU32::new(0);
        let read = || {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // Poll 0..3 → still in-progress; poll 4 → reached.
            Ok(if n < 4 {
                "in-progress".to_string()
            } else {
                "self-reviewed".to_string()
            })
        };
        let outcome = wait_for_lifecycle_with(
            read,
            LifecycleTarget::SelfReviewed,
            || Ok("working".to_string()),
            &opts,
            Instant::now,
        )
        .unwrap();
        assert_eq!(outcome, WaitOutcome::Reached);
    }

    #[test]
    fn wait_for_lifecycle_flags_stall_when_status_never_changes() {
        // Status constant, lifecycle constant, stall timeout tiny → Err.
        // Use a real clock + tiny stall window so the loop bails fast.
        let opts = WaitOptions {
            poll_interval_ms: 1,
            stall_timeout_ms: 5,
        };
        let err = wait_for_lifecycle_with(
            || Ok("in-progress".to_string()),
            LifecycleTarget::SelfReviewed,
            || Ok("working".to_string()),
            &opts,
            Instant::now,
        )
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("hung") && msg.contains("working"),
            "stall error should mention 'hung' + the status: {msg}"
        );
    }

    #[test]
    fn wait_for_lifecycle_tolerates_agent_status_errors() {
        let opts = WaitOptions {
            poll_interval_ms: 1,
            stall_timeout_ms: 0,
        };
        // Status reader always errors; the wait should still reach
        // the target via the lifecycle reader.
        let outcome = wait_for_lifecycle_with(
            || Ok("complete".to_string()),
            LifecycleTarget::Complete,
            || Err(anyhow::anyhow!("herdr unreachable")),
            &opts,
            Instant::now,
        )
        .unwrap();
        assert_eq!(outcome, WaitOutcome::Reached);
    }

    #[test]
    fn wait_options_defaults_are_documented_values() {
        let opts = WaitOptions::default();
        assert_eq!(
            opts.poll_interval_ms, 1_000,
            "polling-strategy default is 1s"
        );
        assert!(
            opts.stall_timeout_ms > 0,
            "stall timeout must be enabled by default"
        );
    }

    #[test]
    fn readiness_options_defaults_are_documented_values() {
        let opts = ReadinessOptions::default();
        assert!(
            opts.timeout_ms > 0,
            "readiness timeout must be enabled by default"
        );
        assert!(opts.poll_interval_ms > 0);
    }
}
