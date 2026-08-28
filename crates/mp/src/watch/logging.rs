//! S10 / AC-08: structured logging for `mp watch`.
//!
//! All herdr interactions (agent start, send, pane read, wait) plus
//! state-machine transitions are logged to a configurable file with
//! RFC3339 timestamps, the active milestone id, and the role label.
//! Default path: `<plan_dir>/.mp/watch.log` (see
//! [`crate::watch::default_log_path`]).
//!
//! Shape: one JSON object per line (JSONL / ndjson). Picked over
//! plain text because the watch log is consumed by both humans
//! (`tail -f`) and tools (`jq`), and a structured shape composes.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::Serialize;

/// One log entry. `kind` is a short tag (e.g. `"herdr_call"`,
/// `"stage_transition"`, `"skip"`). The rest of the fields are
/// optional context — present when the event has them.
#[derive(Debug, Clone, Serialize)]
pub struct WatchLogEntry<'a> {
    pub ts: String,
    pub kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane: Option<&'a str>,
    pub message: String,
    // M197 WP3 / AC-04: spawn diagnostics. The spawn_error event
    // carries the full argv, stdout, stderr, and exit code so
    // operators can diagnose a launch failure without rerunning
    // the watch command with extra logging. These fields are
    // None for every non-spawn event; serde omits them via the
    // `Option` guards above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

impl<'a> WatchLogEntry<'a> {
    /// Construct a minimal entry; chain the builder methods to add
    /// optional context.
    pub fn new(kind: &'a str, message: impl Into<String>) -> Self {
        Self {
            ts: rfc3339_now(),
            kind,
            milestone_id: None,
            role: None,
            pane: None,
            message: message.into(),
            command: None,
            argv: None,
            exit_code: None,
            stdout: None,
            stderr: None,
        }
    }

    pub fn milestone(mut self, id: &'a str) -> Self {
        self.milestone_id = Some(id);
        self
    }

    pub fn role(mut self, role: &'a str) -> Self {
        self.role = Some(role);
        self
    }

    pub fn pane(mut self, pane: &'a str) -> Self {
        self.pane = Some(pane);
        self
    }

    /// M197 WP3 / AC-04: structured spawn diagnostics. Sets the
    /// `command` / `argv` / `exit_code` / `stdout` / `stderr`
    /// fields on a `spawn_error` entry. The `command` is the
    /// short subcommand tag (`"pane split"` or `"agent start"`)
    /// and `argv` is the full argv the herdr layer would have
    /// run. Capturing both lets an operator distinguish between
    /// "pane split failed" and "agent start failed" without
    /// re-reading the watch log by hand.
    pub fn spawn_error(
        mut self,
        command: &'a str,
        argv: Vec<String>,
        exit_code: Option<i32>,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
    ) -> Self {
        self.command = Some(command);
        self.argv = Some(argv);
        self.exit_code = exit_code;
        self.stdout = Some(stdout.into());
        self.stderr = Some(stderr.into());
        self
    }
}

/// Append-only watch logger. Cheap to clone (Mutex-protected file
/// handle) so the state machine + herdr layer can share one without
/// threading it through every function.
#[derive(Clone)]
pub struct WatchLogger {
    inner: std::sync::Arc<Mutex<WatchLoggerInner>>,
}

impl std::fmt::Debug for WatchLogger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = self
            .inner
            .try_lock()
            .map(|g| g.path.clone())
            .unwrap_or_default();
        f.debug_struct("WatchLogger").field("path", &path).finish()
    }
}

struct WatchLoggerInner {
    path: PathBuf,
    /// When None, no writes happen. Lets tests construct a logger
    /// without touching disk.
    sink: Option<Box<dyn Write + Send>>,
}
impl WatchLogger {
    /// Open (or create) the log file at `path` for append. The parent
    /// directory is created if missing.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create log parent {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("open watch log {}", path.display()))?;
        Ok(Self {
            inner: std::sync::Arc::new(Mutex::new(WatchLoggerInner {
                path: path.to_path_buf(),
                sink: Some(Box::new(file)),
            })),
        })
    }

    /// Construct an in-memory logger that drops every entry. Useful
    /// for tests that exercise the entry-building API without disk
    /// I/O.
    pub fn null() -> Self {
        Self {
            inner: std::sync::Arc::new(Mutex::new(WatchLoggerInner {
                path: PathBuf::from("/dev/null"),
                sink: Some(Box::new(std::io::sink())),
            })),
        }
    }

    /// Construct a logger that records to an in-memory buffer. Tests
    /// read the buffer back via `captured`.
    pub fn in_memory() -> (Self, std::sync::Arc<Mutex<Vec<u8>>>) {
        let buf: std::sync::Arc<Mutex<Vec<u8>>> = std::sync::Arc::new(Mutex::new(Vec::new()));
        struct Adapter(std::sync::Arc<Mutex<Vec<u8>>>);
        impl Write for Adapter {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let logger = Self {
            inner: std::sync::Arc::new(Mutex::new(WatchLoggerInner {
                path: PathBuf::from("/memory"),
                sink: Some(Box::new(Adapter(buf.clone()))),
            })),
        };
        (logger, buf)
    }

    /// Append one entry as a single JSONL line.
    pub fn log(&self, entry: &WatchLogEntry<'_>) -> Result<()> {
        let mut line = serde_json::to_vec(entry)
            .with_context(|| format!("serialize log entry kind={}", entry.kind))?;
        line.push(b'\n');
        // Recover from poisoning: if a previous write panicked while
        // holding the lock, recover the inner state instead of
        // crashing the watch. A logging failure must not kill an
        // autonomous 30-minute run.
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = guard.path.clone();
        if let Some(sink) = guard.sink.as_mut() {
            sink.write_all(&line)
                .with_context(|| format!("write watch log entry to {}", path.display()))?;
            let _ = sink.flush();
        }
        Ok(())
    }

    /// Path the logger writes to (for inclusion in the CLI report).
    pub fn path(&self) -> PathBuf {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .path
            .clone()
    }
}

/// RFC3339 (UTC) timestamp for "now". Falls back to a placeholder
/// when the system clock is unavailable so the log never panics.
fn format_rfc3339_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO);
    let secs = dur.as_secs() as i64;
    let nanos = dur.subsec_nanos();

    // Convert epoch seconds to a Y-M-D H:M:S tuple without pulling in
    // a datetime crate. Civil-from-days algorithm (Howard Hinnant).
    let z = secs.div_euclid(86_400);
    let s = secs.rem_euclid(86_400);
    let days = z + 719_468;
    let era = (if days >= 0 { days } else { days - 146_096 }) / 146_097;
    let doe = (days - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let hour = s / 3600;
    let minute = (s / 60) % 60;
    let second = s % 60;
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}Z")
}

/// Public so callers / tests can reuse the same timestamp shape.
pub fn rfc3339_now() -> String {
    format_rfc3339_now()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn open_creates_parent_dir_if_missing() {
        let env = tempfile::TempDir::new().unwrap();
        let nested = env.path().join("a/b/c/watch.log");
        let logger = WatchLogger::open(&nested).unwrap();
        logger.log(&WatchLogEntry::new("test", "hello")).unwrap();
        assert!(nested.is_file());
        let text = fs::read_to_string(&nested).unwrap();
        assert!(text.contains("\"kind\":\"test\""));
        assert!(text.contains("hello"));
        assert!(
            text.ends_with('\n'),
            "each entry should be newline-terminated"
        );
    }

    #[test]
    fn entry_builder_attaches_optional_context() {
        let e = WatchLogEntry::new("herdr_call", "agent start")
            .milestone("42")
            .role("runner")
            .pane("%5");
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"milestone_id\":\"42\""));
        assert!(s.contains("\"role\":\"runner\""));
        assert!(s.contains("\"pane\":\"%5\""));
        assert!(s.contains("\"kind\":\"herdr_call\""));
        assert!(s.contains("\"message\":\"agent start\""));
    }

    #[test]
    fn entry_omits_none_optional_fields() {
        let e = WatchLogEntry::new("skip", "lifecycle=draft");
        let s = serde_json::to_string(&e).unwrap();
        assert!(
            !s.contains("milestone_id"),
            "optional fields should be skipped when None: {s}"
        );
        assert!(!s.contains("role"));
        assert!(!s.contains("pane"));
        // M197 WP3 / AC-04: spawn-diagnostics fields are also
        // optional and must be omitted when None. Otherwise a
        // non-spawn event would carry `"argv": null, "exit_code":
        // null, "stdout": null, "stderr": null` and clutter
        // downstream parsers.
        assert!(
            !s.contains("command"),
            "non-spawn entry should omit command: {s}"
        );
        assert!(!s.contains("argv"), "non-spawn entry should omit argv: {s}");
        assert!(
            !s.contains("exit_code"),
            "non-spawn entry should omit exit_code: {s}"
        );
        assert!(
            !s.contains("stdout"),
            "non-spawn entry should omit stdout: {s}"
        );
        assert!(
            !s.contains("stderr"),
            "non-spawn entry should omit stderr: {s}"
        );
    }

    #[test]
    fn spawn_error_entry_carries_diagnostic_fields() {
        // M197 WP3 / AC-04: the spawn_error event carries the
        // command, full argv, exit code, stdout, and stderr so an
        // operator can diagnose a launch failure from the log
        // alone. Each field is a separate JSON property, not a
        // stringified blob, so `jq` can grep / project them.
        let e = WatchLogEntry::new("spawn_error", "herdr agent start failed")
            .role("runner")
            .spawn_error(
                "agent start",
                vec![
                    "agent".into(),
                    "start".into(),
                    "role-runner-1".into(),
                    "--kind".into(),
                    "opencode".into(),
                    "--pane".into(),
                    "%7".into(),
                ],
                Some(1),
                "stdout text\n",
                "herdr: workspace full\n",
            );
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"kind\":\"spawn_error\""), "{s}");
        assert!(s.contains("\"command\":\"agent start\""), "{s}");
        assert!(s.contains("\"exit_code\":1"), "{s}");
        assert!(s.contains("\"stdout\":\"stdout text\\n\""), "{s}");
        assert!(s.contains("\"stderr\":\"herdr: workspace full\\n\""), "{s}");
        // argv is a JSON array, not a stringified blob.
        assert!(s.contains("\"argv\":["), "{s}");
        assert!(s.contains("\"--kind\""), "{s}");
        assert!(s.contains("\"opencode\""), "{s}");
        assert!(s.contains("\"--pane\""), "{s}");
        assert!(s.contains("\"%7\""), "{s}");
        assert!(s.contains("\"role\":\"runner\""), "{s}");
    }

    #[test]
    fn in_memory_logger_captures_entries() {
        let (logger, buf) = WatchLogger::in_memory();
        logger
            .log(&WatchLogEntry::new("stage", "execute").milestone("7"))
            .unwrap();
        logger
            .log(&WatchLogEntry::new("stage", "external_review").milestone("7"))
            .unwrap();
        let captured = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        let lines: Vec<&str> = captured.lines().collect();
        assert_eq!(lines.len(), 2, "two entries → two lines");
        assert!(lines[0].contains("execute"));
        assert!(lines[1].contains("external_review"));
    }

    #[test]
    fn null_logger_never_writes() {
        let logger = WatchLogger::null();
        // The null logger's path is /dev/null; .log() should succeed
        // without producing any observable file.
        logger.log(&WatchLogEntry::new("x", "y")).unwrap();
    }

    #[test]
    fn rfc3339_timestamp_has_expected_shape() {
        let ts = rfc3339_now();
        // YYYY-MM-DDTHH:MM:SS.nnnnnnnnnZ = 30 chars.
        assert_eq!(ts.len(), 30, "ts = {ts}");
        assert!(ts.starts_with("20"));
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.as_bytes()[4], b'-');
        assert_eq!(ts.as_bytes()[10], b'T');
    }

    #[test]
    fn multiple_loggers_appending_to_same_file_do_not_interleave_single_entries() {
        // Sequential appends from two logger handles should each
        // produce a complete JSONL line.
        let env = tempfile::TempDir::new().unwrap();
        let path = env.path().join("watch.log");
        let a = WatchLogger::open(&path).unwrap();
        let b = WatchLogger::open(&path).unwrap();
        a.log(&WatchLogEntry::new("a", "first")).unwrap();
        b.log(&WatchLogEntry::new("b", "second")).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"kind\":\"a\""));
        assert!(lines[1].contains("\"kind\":\"b\""));
    }

    #[test]
    fn path_returns_the_opened_path() {
        let env = tempfile::TempDir::new().unwrap();
        let path = env.path().join("watch.log");
        let logger = WatchLogger::open(&path).unwrap();
        assert_eq!(logger.path(), path);
    }
}
