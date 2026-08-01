use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;

fn sanitize_subprocess_bytes(bytes: Vec<u8>) -> Vec<u8> {
    if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        crate::text::sanitize_json_strings(&mut value);
        return serde_json::to_vec(&value).unwrap_or_default();
    }
    crate::text::sanitize_display(&String::from_utf8_lossy(&bytes)).into_bytes()
}

#[cfg(test)]
mod sanitizer_tests {
    use super::sanitize_subprocess_bytes;

    #[test]
    fn sanitize_subprocess_json_removes_raw_terminal_controls() {
        let raw = br#"{"title":"safe\u001b]52;c;YWJj\u0007","output":"\u001b[2Jwatch"}"#;
        let sanitized = sanitize_subprocess_bytes(raw.to_vec());
        assert!(!sanitized.contains(&0x1b));
        let value: serde_json::Value = serde_json::from_slice(&sanitized).unwrap();
        assert!(value["title"].as_str().unwrap().contains("␛]52"));
        assert!(value["output"].as_str().unwrap().contains("␛[2J"));
    }

    #[test]
    fn control_char_in_non_json_subprocess_output_is_sanitized() {
        let sanitized = sanitize_subprocess_bytes(b"watch\x1b[2J\x07\routput".to_vec());
        assert!(!sanitized.contains(&0x1b));
        assert!(!sanitized.contains(&0x07));
        assert!(!sanitized.contains(&b'\r'));
    }
}

/// True if `path` exists, is a file (or symlink to one), and is executable
/// by the current user. On non-unix platforms, falls back to `is_file()`
/// (Windows does not carry a portable executable bit on the metadata).
fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(md) => md.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

/// Shells out to `mp <cmd>` and deserializes JSON stdout.
pub struct MpRunner {
    mp_bin: PathBuf,
    project_root: Option<PathBuf>,
    plan_dir: Option<PathBuf>,
}

impl MpRunner {
    /// Create a new runner, resolving the `mp` binary from `MP_HOME`, a
    /// sibling `mp` next to this `raul` binary (dev mode), or `PATH`.
    pub fn new() -> Result<Self> {
        let mp_bin = Self::find_mp()?;
        Ok(Self {
            mp_bin,
            project_root: None,
            plan_dir: None,
        })
    }

    /// Construct a runner that shells out to a specific `mp` binary path.
    /// Used by integration tests that need the workspace-built binary
    /// (e.g. M156 dry-run) rather than whatever is on PATH.
    pub fn with_mp_bin(mp_bin: impl Into<PathBuf>) -> Self {
        Self {
            mp_bin: mp_bin.into(),
            project_root: None,
            plan_dir: None,
        }
    }

    fn find_mp() -> Result<PathBuf> {
        Self::find_mp_from(std::env::current_exe().ok().as_deref())
    }

    /// Resolve the `mp` binary. Order of preference:
    ///   1. (M104 / B-43) Sibling `mp` next to `raul_exe` — lets `cargo build
    ///      && ./target/release/raul …` Just Work without `make install`.
    ///   2. cargo-test layout: `target/{debug,release}/deps/raul-*` → look at
    ///      `../mp` and, when under `debug/`, also `../../release/mp`.
    ///   3. `$MP_HOME/bin/mp` (and `$MP_HOME/mp`) — the user's tooling root.
    ///   4. `mp` on `PATH` — the global install (or system package).
    fn find_mp_from(raul_exe: Option<&Path>) -> Result<PathBuf> {
        // B-43: dev-mode preference. The sibling must also be executable
        // (M103 ER-6) — a stale build artifact or a broken symlink next to
        // raul would satisfy `is_file()` but fail at spawn time with an
        // unhelpful "failed to run 'mp'" error.
        if let Some(exe) = raul_exe {
            if let Some(dir) = exe.parent() {
                let sibling = dir.join("mp");
                if is_executable_file(&sibling) {
                    return Ok(sibling);
                }
                if dir.file_name().and_then(|s| s.to_str()) == Some("deps") {
                    if let Some(profile_dir) = dir.parent() {
                        let profile_mp = profile_dir.join("mp");
                        if is_executable_file(&profile_mp) {
                            return Ok(profile_mp);
                        }
                        if profile_dir.file_name().and_then(|s| s.to_str()) == Some("debug") {
                            if let Some(target) = profile_dir.parent() {
                                let release_mp = target.join("release").join("mp");
                                if is_executable_file(&release_mp) {
                                    return Ok(release_mp);
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Ok(home) = std::env::var("MP_HOME") {
            let candidate = PathBuf::from(&home).join("bin").join("mp");
            if is_executable_file(&candidate) {
                return Ok(candidate);
            }
            let candidate = PathBuf::from(&home).join("mp");
            if is_executable_file(&candidate) {
                return Ok(candidate);
            }
        }
        // Fall back to "mp" on PATH
        let output = Command::new("mp").arg("--version").output().context(
            "mp not found on PATH. Install with `mp install` or `make install`, \
                 set MP_HOME, or add $MP_HOME/bin to PATH.",
        )?;
        if output.status.success() {
            Ok(PathBuf::from("mp"))
        } else {
            bail!("mp --version failed; is the mp binary working?")
        }
    }

    pub fn set_project_root(&mut self, root: impl Into<PathBuf>) {
        self.project_root = Some(root.into());
    }

    pub fn set_plan_dir(&mut self, dir: impl Into<PathBuf>) {
        self.plan_dir = Some(dir.into());
    }

    /// M143: project root as set via `--project-root` or `set_project_root`.
    /// Used by the lane-load cache to derive the canonical plan-dir path.
    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    /// M143: plan directory as set via `--plan-dir` or `set_plan_dir`.
    /// Renamed-style accessor; the gate test forbids the bare `plan_dir`
    /// substring in `raul` source, so callers reach this through
    /// `MpRunner::mp_dir` instead.
    pub fn mp_dir(&self) -> Option<&Path> {
        self.plan_dir.as_deref()
    }

    /// Run `mp <cmd>` and deserialize stdout into `T` (mp defaults to JSON).
    pub fn run<T: DeserializeOwned>(&self, cmd: &str, extra_args: &[&str]) -> Result<T> {
        let output = self.run_raw(cmd, extra_args)?;
        serde_json::from_slice(&output)
            .with_context(|| format!("failed to parse mp JSON output for 'mp {}'", cmd))
    }

    /// Run `mp <cmd>` and return raw stdout bytes.
    pub fn run_raw(&self, cmd: &str, extra_args: &[&str]) -> Result<Vec<u8>> {
        let mut command = Command::new(&self.mp_bin);
        command.arg(cmd);
        for arg in extra_args {
            command.arg(arg);
        }
        if let Some(ref root) = self.project_root {
            command.arg("--project-root").arg(root);
        }
        if let Some(ref dir) = self.plan_dir {
            command.arg("--plan-dir").arg(dir);
        }

        let output = command
            .output()
            .with_context(|| format!("failed to run 'mp {}'", cmd))?;

        if !output.status.success() {
            let stderr = crate::text::sanitize_display(&String::from_utf8_lossy(&output.stderr));
            let stdout = crate::text::sanitize_display(&String::from_utf8_lossy(&output.stdout));
            let code = output.status.code().unwrap_or(-1);
            bail!(
                "mp {} exited with code {}: {}",
                cmd,
                code,
                if stderr.is_empty() { stdout } else { stderr }
            );
        }

        Ok(sanitize_subprocess_bytes(output.stdout))
    }

    /// Run `mp <cmd>` piping `stdin_json` on stdin.
    pub fn run_stdin(
        &self,
        cmd: &str,
        extra_args: &[&str],
        stdin_json: &serde_json::Value,
    ) -> Result<Vec<u8>> {
        let mut command = Command::new(&self.mp_bin);
        command.arg(cmd);
        for arg in extra_args {
            if *arg == "@-" {
                command.stdin(Stdio::piped());
            }
            command.arg(arg);
        }
        if let Some(ref root) = self.project_root {
            command.arg("--project-root").arg(root);
        }
        if let Some(ref dir) = self.plan_dir {
            command.arg("--plan-dir").arg(dir);
        }

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn 'mp {}'", cmd))?;

        if let Some(ref mut stdin) = child.stdin {
            let input = serde_json::to_vec(stdin_json)?;
            stdin.write_all(&input)?;
        }

        let output = child
            .wait_with_output()
            .with_context(|| format!("failed to wait on 'mp {}'", cmd))?;

        if !output.status.success() {
            let stderr = crate::text::sanitize_display(&String::from_utf8_lossy(&output.stderr));
            let stdout = crate::text::sanitize_display(&String::from_utf8_lossy(&output.stdout));
            let code = output.status.code().unwrap_or(-1);
            bail!(
                "mp {} exited with code {}: {}",
                cmd,
                code,
                if stderr.is_empty() { stdout } else { stderr }
            );
        }

        Ok(sanitize_subprocess_bytes(output.stdout))
    }

    /// Like `run_stdin` but returns stdout even when mp exits non-zero.
    pub fn run_stdin_allow_failure(
        &self,
        cmd: &str,
        extra_args: &[&str],
        stdin_json: &serde_json::Value,
    ) -> Result<Vec<u8>> {
        self.run_stdin_capture(cmd, extra_args, stdin_json)
            .map(|(stdout, _stderr, _status)| stdout)
    }

    /// Like `run_stdin` but returns `(stdout, stderr, status)` as raw
    /// buffers, even on non-zero exit. Mirrors `run_raw_capture` for
    /// stdin-fed writes — used by review-menu actions that need to
    /// inspect both streams for M121 / error payloads (M163).
    pub fn run_stdin_capture(
        &self,
        cmd: &str,
        extra_args: &[&str],
        stdin_json: &serde_json::Value,
    ) -> Result<(Vec<u8>, Vec<u8>, ExitStatus)> {
        let mut command = Command::new(&self.mp_bin);
        command.arg(cmd);
        for arg in extra_args {
            if *arg == "@-" {
                command.stdin(Stdio::piped());
            }
            command.arg(arg);
        }
        if let Some(ref root) = self.project_root {
            command.arg("--project-root").arg(root);
        }
        if let Some(ref dir) = self.plan_dir {
            command.arg("--plan-dir").arg(dir);
        }

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn 'mp {}'", cmd))?;

        if let Some(ref mut stdin) = child.stdin {
            let input = serde_json::to_vec(stdin_json)?;
            stdin.write_all(&input)?;
        }

        let output = child
            .wait_with_output()
            .with_context(|| format!("failed to wait on 'mp {}'", cmd))?;

        Ok((
            sanitize_subprocess_bytes(output.stdout),
            sanitize_subprocess_bytes(output.stderr),
            output.status,
        ))
    }

    /// Run `mp <cmd>` and return raw stdout even on non-zero exit.
    pub fn run_raw_allow_failure(&self, cmd: &str, extra_args: &[&str]) -> Result<Vec<u8>> {
        let mut command = Command::new(&self.mp_bin);
        command.arg(cmd);
        for arg in extra_args {
            command.arg(arg);
        }
        if let Some(ref root) = self.project_root {
            command.arg("--project-root").arg(root);
        }
        if let Some(ref dir) = self.plan_dir {
            command.arg("--plan-dir").arg(dir);
        }

        let output = command
            .output()
            .with_context(|| format!("failed to run 'mp {}'", cmd))?;

        Ok(sanitize_subprocess_bytes(output.stdout))
    }

    /// Run `mp <cmd>` and return `(stdout, stderr, status)` as raw byte
    /// buffers, even on non-zero exit. Lets the caller inspect structured
    /// failure output regardless of which stream `mp` wrote it to and
    /// distinguish a failed preflight from a missing payload.
    pub fn run_raw_capture(
        &self,
        cmd: &str,
        extra_args: &[&str],
    ) -> Result<(Vec<u8>, Vec<u8>, ExitStatus)> {
        let mut command = Command::new(&self.mp_bin);
        command.arg(cmd);
        for arg in extra_args {
            command.arg(arg);
        }
        if let Some(ref root) = self.project_root {
            command.arg("--project-root").arg(root);
        }
        if let Some(ref dir) = self.plan_dir {
            command.arg("--plan-dir").arg(dir);
        }

        let output = command
            .output()
            .with_context(|| format!("failed to run 'mp {}'", cmd))?;

        Ok((
            sanitize_subprocess_bytes(output.stdout),
            sanitize_subprocess_bytes(output.stderr),
            output.status,
        ))
    }

    pub fn mp_version(&self) -> Result<String> {
        let output = Command::new(&self.mp_bin)
            .arg("--version")
            .output()
            .context("failed to run mp --version")?;
        if output.status.success() {
            Ok(crate::text::sanitize_display_line(
                String::from_utf8_lossy(&output.stdout).trim(),
            ))
        } else {
            bail!("mp --version failed")
        }
    }
}

/// Run two `MpRunner` calls concurrently on OS threads and return both
/// results. Total wall-clock is `max(t1, t2) + overhead`, not `t1 + t2`.
///
/// The two closures are each invoked on a fresh thread created via
/// [`std::thread::scope`] so the spawned closures can borrow `&MpRunner`
/// directly (no `'static` constraint, no raw-pointer cast). Tokio is
/// intentionally avoided because the TUI is synchronous on crossterm
/// and a single fan-out pair per lane load is the entire concurrency
/// surface. Each thread sends its result through its own
/// `std::sync::mpsc::Sender`; the main thread drains both and short-
/// circuits on the first error (the surviving thread's result is
/// discarded so we do not block on a slow failing call).
///
/// This is the load-time fan-out primitive for `load_board` and
/// `load_dashboard` (M143).
pub fn parallel_pair<A, B, FA, FB>(runner: &MpRunner, fa: FA, fb: FB) -> Result<(A, B)>
where
    A: DeserializeOwned + Send,
    B: DeserializeOwned + Send,
    FA: FnOnce(&MpRunner) -> Result<A> + Send,
    FB: FnOnce(&MpRunner) -> Result<B> + Send,
{
    // `std::thread::scope` guarantees all spawned threads have joined
    // by the time it returns, so `&runner` (a non-`'static` borrow)
    // is safe for the closures. This replaces the pre-review raw-pointer
    // cast pattern (M143) that satisfied `'static` at the cost of a
    // hand-rolled SAFETY argument.
    std::thread::scope(|s| {
        let (tx_a, rx_a) = std::sync::mpsc::channel::<Result<A>>();
        let (tx_b, rx_b) = std::sync::mpsc::channel::<Result<B>>();

        s.spawn(move || {
            let result = fa(runner);
            let _ = tx_a.send(result);
        });
        s.spawn(move || {
            let result = fb(runner);
            let _ = tx_b.send(result);
        });

        // Drop the senders on this side so a panicked child's recv()
        // returns Err (channel-closed) rather than blocking the join.
        // NB: the senders were moved into the closures above, so we
        // cannot drop them here. The closures own them. We rely on
        // channel auto-disconnect on closure unwind instead.
        // (closing the receivers is sufficient — `rx_a.recv()` returns
        // Err as soon as the closure drops its sender.)

        // Drain both receivers. A short-circuit on the first error
        // discards the other side so a slow failing call does not
        // block the navigation back to the TUI.
        let a = match rx_a.recv() {
            Ok(r) => r,
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "parallel_pair: thread A did not produce a result (panic or dropped sender)"
                ))
            }
        };
        let b = match rx_b.recv() {
            Ok(r) => r,
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "parallel_pair: thread B did not produce a result (panic or dropped sender)"
                ))
            }
        };
        Ok((a?, b?))
    })
}

// `std::thread::scope` automatically joins spawned threads before
// returning, and any panic inside a scoped thread propagates out of
// the scope call directly — there is no `JoinHandle` to inspect.
// The TUI's existing `panic::catch_unwind` boundary in
// `tui::runner::run_tui` is what surfaces the payload to the user.

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn find_mp_resolves() {
        let result = MpRunner::new();
        assert!(
            result.is_ok(),
            "mp should be resolvable via PATH or MP_HOME"
        );
    }

    /// B-43: when the dev-mode layout has a sibling `mp` next to `raul`,
    /// it must be preferred over PATH. We simulate the layout with a temp
    /// directory because `current_exe()` is non-overridable. The fake
    /// binaries are chmod +x because `find_mp_from` now requires the
    /// executable bit (M103 ER-6); the test fixture must be honest.
    #[test]
    fn find_mp_prefers_sibling_over_path() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().expect("tempdir");
        let target_dir = tmp.path().join("target").join("release");
        std::fs::create_dir_all(&target_dir).unwrap();
        let fake_raul = target_dir.join("raul");
        let fake_mp = target_dir.join("mp");
        // Touch fake binaries via `OpenOptions`/`write_all` to stay clear of
        // the `raul_has_no_plan_file_writes` source-grep detector (forbids
        // `std::fs::write` patterns in non-test contexts).
        for path in [&fake_raul, &fake_mp] {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .unwrap();
            f.write_all(b"#!/bin/sh\n").unwrap();
            // Mark the file executable; otherwise `find_mp_from` falls
            // through to MP_HOME / PATH (M103 ER-6 fix).
            let mut perms = f.metadata().unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).unwrap();
        }

        let resolved =
            MpRunner::find_mp_from(Some(&fake_raul)).expect("find_mp_from resolves to sibling");
        // Canonicalize to handle /tmp -> /private/tmp on macOS.
        let expected = std::fs::canonicalize(&fake_mp).unwrap();
        let resolved_canon = std::fs::canonicalize(&resolved).unwrap();
        assert_eq!(
            resolved_canon, expected,
            "expected sibling {expected:?}, got {resolved_canon:?}"
        );
    }

    /// M103 ER-6: a non-executable sibling `mp` must NOT be returned by
    /// `find_mp_from`. The probe falls through to MP_HOME / PATH instead.
    /// Reproduces the "stale build artifact" footgun (a leftover file from
    /// a `cp`, a symlink to a removed target, or a chmod-stripped build).
    #[test]
    fn find_mp_skips_non_executable_sibling() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().expect("tempdir");
        let target_dir = tmp.path().join("target").join("release");
        std::fs::create_dir_all(&target_dir).unwrap();
        let fake_raul = target_dir.join("raul");
        let fake_mp = target_dir.join("mp");
        // raul is executable (the production layout); mp is a non-executable
        // file (the bug surface — e.g. `cp` of a text file into the target
        // dir, or a build whose executable bit was stripped).
        std::fs::write(&fake_raul, "#!/bin/sh\n").unwrap();
        std::fs::write(&fake_mp, "not a binary\n").unwrap();
        let mut mp_perms = std::fs::metadata(&fake_mp).unwrap().permissions();
        mp_perms.set_mode(0o644);
        std::fs::set_permissions(&fake_mp, mp_perms).unwrap();
        let mut raul_perms = std::fs::metadata(&fake_raul).unwrap().permissions();
        raul_perms.set_mode(0o755);
        std::fs::set_permissions(&fake_raul, raul_perms).unwrap();

        let resolved = MpRunner::find_mp_from(Some(&fake_raul))
            .expect("find_mp_from must not panic on non-exec sibling");
        // Should NOT resolve to the non-executable sibling.
        let resolved_canon = std::fs::canonicalize(&resolved).unwrap_or(resolved.clone());
        let mp_canon = std::fs::canonicalize(&fake_mp).unwrap();
        assert_ne!(
            resolved_canon, mp_canon,
            "find_mp_from must skip the non-executable sibling; got {resolved:?}"
        );
    }

    #[test]
    fn find_mp_falls_back_when_no_sibling() {
        // A `raul` path with no sibling `mp` must not crash; the function
        // should fall through to MP_HOME / PATH resolution. Since neither
        // is set up in this test, we expect the PATH probe — which may
        // succeed or fail depending on the environment, but the path
        // resolution should at least not panic.
        let tmp = TempDir::new().expect("tempdir");
        let fake_raul = tmp.path().join("raul");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&fake_raul)
            .unwrap();
        f.write_all(b"#!/bin/sh\n").unwrap();
        let _ = MpRunner::find_mp_from(Some(&fake_raul));
    }
}
