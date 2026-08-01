//! M150 S4 / AC-01: bridge report emission tests.
//!
//! Verify that `mp milestone complete` and `mp reviews pass` emit
//! exactly one `herdr pane report-agent` call with the stage-done
//! sentinel when running inside a herdr pane, and zero calls when
//! the env vars are unset. The tests use a fake `herdr` binary on
//! PATH so the assertions are observable end-to-end through the
//! subprocess boundary (not mocked at the library layer).

mod common;

use crate::common::{mp_bin, repo_root, TestEnv};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

/// Per-test mutex: subprocess tests share the global PATH for the
/// duration of the test process. Serialize them so a parallel test
/// can't insert another `herdr` script into PATH mid-test.
static PATH_LOCK: Mutex<()> = Mutex::new(());

/// RAII env guard: snapshots `PATH` + `HERDR_PANE_ID` on construction
/// and restores them on `Drop`. `Drop` also runs while a test panic
/// is unwinding, so a panic mid-assertion can't leak env mutations
/// into the next test. Tests that mutate these env vars MUST hold
/// an `EnvGuard` instance (typically `_env`) for the duration of the
/// mutation; the trailing underscore documents the intentional
/// "held, never read" pattern.
struct EnvGuard {
    saved_path: Option<String>,
    saved_pane: Option<String>,
    restored: bool,
    /// Path entry the test prepended (so Drop can strip it
    /// specifically — see `set_test_env` for why).
    prepended_path: Option<String>,
}

impl EnvGuard {
    fn new() -> Self {
        Self {
            saved_path: std::env::var("PATH").ok(),
            saved_pane: std::env::var("HERDR_PANE_ID").ok(),
            restored: false,
            prepended_path: None,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        // First: strip the prepended entry if we know it. Stripping
        // by exact entry prevents leaving a stale PATH entry pointing
        // at a TempDir that the next test will create and own.
        if let Some(strip) = &self.prepended_path {
            if let Ok(current) = std::env::var("PATH") {
                let filtered: Vec<String> = current
                    .split(':')
                    .filter(|d| *d != strip.as_str())
                    .map(String::from)
                    .collect();
                let joined = filtered.join(":");
                unsafe {
                    std::env::set_var("PATH", &joined);
                }
            }
        }
        // Then restore the original PATH snapshot to handle the case
        // where the saved_path was captured before we modified PATH.
        if let Some(v) = &self.saved_path {
            unsafe {
                std::env::set_var("PATH", v);
            }
        }
        match &self.saved_pane {
            Some(v) => unsafe {
                std::env::set_var("HERDR_PANE_ID", v);
            },
            None => unsafe {
                std::env::remove_var("HERDR_PANE_ID");
            },
        }
    }
}

/// Set `HERDR_PANE_ID` and prepend `path` to `PATH` for the lifetime
/// of the returned `EnvGuard`. Prepending (not overwriting) keeps the
/// shell's PATH lookup intact so fake scripts can still find
/// `cat` / `echo`.
///
/// The guard's Drop **strips** the prepended `path` entry from PATH
/// before restoring the saved snapshot. Without the strip, a later
/// test that doesn't use `set_test_env` could see a stale `path`
/// entry pointing at a now-deleted TempDir (each test owns its own
/// TempDir that drops at end of scope); under parallel test
/// execution the parent's PATH would carry a phantom directory,
/// and the next test's `which_herdr` would skip stale entries or
/// invoke the real herdr at the recycled path — both surface as
/// silent integration failures. Stripping the prepended entry by
/// exact match avoids both.
fn set_test_env(path: &Path, pane: Option<&str>) -> EnvGuard {
    let mut guard = EnvGuard::new();
    let s = path.display().to_string();
    let prev = std::env::var("PATH").unwrap_or_default();
    let new = format!("{}:{}", s, prev);
    unsafe {
        std::env::set_var("PATH", &new);
    }
    match pane {
        Some(v) => unsafe {
            std::env::set_var("HERDR_PANE_ID", v);
        },
        None => unsafe {
            std::env::remove_var("HERDR_PANE_ID");
        },
    }
    guard.prepended_path = Some(s);
    guard
}

fn install_fake_herdr(dir: &Path, body: &str) -> std::path::PathBuf {
    let script = format!("#!/bin/sh\n{body}\n");
    let bin = dir.join("herdr");
    fs::write(&bin, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms).unwrap();
    }
    bin
}

fn record_log_path(dir: &Path) -> std::path::PathBuf {
    dir.join("herdr-calls.log")
}

/// Run `mp` with explicit `set` and `unset` env controls so a test
/// can guarantee `HERDR_PANE_ID` is *absent* (not just absent from
/// the helper's view). `TestEnv::run_with_env` only sets vars; it
/// doesn't unset inherited ones, which would defeat the
/// "no HERDR_PANE_ID" assertion below.
fn run_mp_with_env(
    env: &TestEnv,
    set: &[(&str, &str)],
    unset: &[&str],
    args: &[&str],
) -> std::process::Output {
    let install_dir = env.tmp.path().join("install-target");
    let args: Vec<&str> = args.to_vec();
    let mut cmd = Command::new(mp_bin());
    cmd.current_dir(env.tmp.path())
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", &install_dir);
    for (k, v) in set {
        cmd.env(k, v);
    }
    for k in unset {
        cmd.env_remove(k);
    }
    cmd.args(&args);
    cmd.output().expect("spawn mp")
}

fn create_ready_milestone(env: &TestEnv, id: &str, title: &str) -> String {
    let create_json = format!(
        r#"{{
            "id": "{id}",
            "title": "{title}",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "intent": {{ "outcome": "bridge report test" }},
            "problem": {{ "description": "p" }},
            "scope": {{ "in_scope": ["x"], "out_of_scope": ["y", "z"] }},
            "acceptance_criteria": [
                {{ "description": "ac", "verification": "manual: yes" }}
            ]
        }}"#
    );
    let created = env.run_json(&[
        "milestone",
        "create",
        "--json",
        &create_json,
        "--format",
        "json",
    ]);
    let new_id = created["milestone"]["id"].as_str().unwrap().to_string();
    // Promote through the public ceremony so complete is allowed.
    let approve = env.run(&["milestone", "approve", &new_id]);
    assert!(
        approve.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let start = env.run(&["milestone", "set-status", &new_id, "in-progress"]);
    assert!(
        start.status.success(),
        "set-status in-progress failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );
    let setup = run_mp_with_env(
        env,
        &[],
        &["HERDR_PANE_ID"],
        &["milestone", "complete", &new_id, "--evidence", "setup"],
    );
    assert!(
        setup.status.success(),
        "setup milestone complete failed: {}",
        String::from_utf8_lossy(&setup.stderr)
    );
    new_id
}

// ─── emit_stage_done_best_effort directly (library surface) ──────────────────

#[test]
fn emit_stage_done_best_effort_noop_when_herdr_pane_id_unset() {
    let _g = PATH_LOCK.lock().unwrap();
    // Without HERDR_PANE_ID, the helper is a pure no-op regardless
    // of whether herdr is on PATH. We assert by counting subprocess
    // invocations on a fake herdr: there should be none.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = record_log_path(&bin_dir);
    fs::create_dir_all(&bin_dir).unwrap();
    let _bin = install_fake_herdr(
        &bin_dir,
        &format!(r#"echo "argv: $*" >> "{log}""#, log = log.display()),
    );

    let _env = set_test_env(&bin_dir, None);

    let emitted =
        mp::watch::emit_stage_done_best_effort("milestone-complete", Some("test-milestone"));
    assert!(!emitted, "should be a no-op without HERDR_PANE_ID");

    let log_text = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        log_text.is_empty(),
        "fake herdr should not have been invoked: {log_text}"
    );
}

#[test]
fn emit_stage_done_best_effort_invokes_herdr_once_with_sentinel() {
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = record_log_path(&bin_dir);
    fs::create_dir_all(&bin_dir).unwrap();
    let _bin = install_fake_herdr(
        &bin_dir,
        &format!(r#"echo "argv: $*" >> "{log}""#, log = log.display()),
    );

    let _env = set_test_env(&bin_dir, Some("wA:p3"));

    // Warm up the fake herdr so first-iteration shell cold-start
    // (~220ms on macOS) doesn't run inside the bounded subprocess
    // timeout (500ms). Without this, parallel test load makes the
    // cold start exceed the deadline and the helper swallows the
    // herdr call as a timeout — surfacing as a flaky
    // log-doesn't-exist assertion. The warmup writes a line to the
    // log which we then clear.
    let _ = Command::new(bin_dir.join("herdr"))
        .args(["pane", "report-agent", "warmup"])
        .output();
    let _ = fs::remove_file(&log);

    let emitted =
        mp::watch::emit_stage_done_best_effort("milestone-complete", Some("test-milestone"));
    assert!(
        emitted,
        "should have emitted when HERDR_PANE_ID + herdr set"
    );

    let log_text = fs::read_to_string(&log).unwrap_or_else(|e| {
        panic!(
            "expected herdr-calls.log at {} but read failed: {e}",
            log.display()
        )
    });
    // Exactly one report-agent call (the canonical --source mp --agent mp-runner
    // --custom-status mp-stage-done shape).
    let report_lines: Vec<&str> = log_text
        .lines()
        .filter(|l| l.contains("pane report-agent"))
        .collect();
    assert_eq!(
        report_lines.len(),
        1,
        "exactly one pane report-agent call expected; got: {report_lines:?}"
    );
    let line = report_lines[0];
    assert!(
        line.contains("--source mp"),
        "argv should pin --source mp: {line}"
    );
    assert!(
        line.contains("--agent mp-runner"),
        "argv should pin --agent mp-runner: {line}"
    );
    assert!(
        line.contains("--custom-status mp-stage-done"),
        "argv should pin --custom-status sentinel: {line}"
    );
    // F-05: --message carries the milestone id.
    assert!(
        line.contains("--message test-milestone"),
        "argv should carry milestone id in --message (F-05): {line}"
    );
    // F-05: --seq is removed (herdr 0.7.3 rejects string values).
    assert!(
        !line.contains("--seq"),
        "argv should NOT include --seq (herdr 0.7.3 rejects strings; F-05): {line}"
    );
    assert!(
        line.contains("wA:p3"),
        "argv should target HERDR_PANE_ID: {line}"
    );
}

// ─── Best-effort swallow path (F-10) ──────────────────────────────────────────

#[test]
fn emit_stage_done_best_effort_swallows_herdr_failure_without_panicking() {
    // The helper is documented as best-effort: a non-zero exit from
    // `herdr pane report-agent` must NOT panic, propagate as Err, or
    // block the producer. We feed it a fake herdr that exits 1 and
    // assert the call returns false silently — proving the swallow
    // path is wired end-to-end through the subprocess boundary.
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let _bin = install_fake_herdr(&bin_dir, "exit 1");

    let _env = set_test_env(&bin_dir, Some("wA:p3"));

    // Warm up so first-iteration cold-start doesn't dominate timing.
    let _ = Command::new(bin_dir.join("herdr")).output();

    let emitted =
        mp::watch::emit_stage_done_best_effort("milestone-complete", Some("test-milestone"));
    assert!(
        !emitted,
        "herdr failure must be swallowed silently — false, not Err/panic"
    );
}

#[test]
fn mp_milestone_complete_succeeds_when_herdr_fails() {
    // End-to-end: even when `herdr pane report-agent` exits non-zero,
    // the underlying `mp milestone complete` write must succeed. This
    // pins the best-effort contract from the producer side: the
    // sentinel emission never fails the milestone complete call.
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    // Fake herdr that always fails — the bridge is broken end-to-end.
    let _bin = install_fake_herdr(&bin_dir, "exit 2");

    let new_path = bin_dir.display().to_string();
    let id = create_ready_milestone(&env, "152", "bridge broken-e2e");

    // Use set_test_env so the parent's PATH/HERDR_PANE_ID are
    // managed through the EnvGuard (the previous RestorePathOnly
    // approach leaked the bin_dir entry into the parent's PATH and
    // caused cross-test interference when nextest ran tests in
    // parallel within a binary). HERDR_PANE_ID is intentionally set
    // for parity with the broken-bridge end-to-end.
    let _env = set_test_env(&bin_dir, Some("wA:p3"));

    // Warm up the fake herdr so its shell cold-start doesn't push
    // the subprocess past the 500ms deadline.
    let _ = Command::new(bin_dir.join("herdr")).output();

    let out = env.run_with_env(
        &[("PATH", &new_path), ("HERDR_PANE_ID", "wA:p3")],
        &[
            "milestone",
            "complete",
            &id,
            "--evidence",
            "bridge-broken-test",
        ],
    );
    assert!(
        out.status.success(),
        "mp milestone complete must NOT regress when herdr fails: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// ─── End-to-end via real mp binary ───────────────────────────────────────────

#[test]
fn mp_milestone_complete_inside_herdr_pane_emits_report_agent_call() {
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = record_log_path(&bin_dir);
    fs::create_dir_all(&bin_dir).unwrap();
    // Fake herdr: pane report-agent logs argv; pane get returns an
    // envelope without custom_status (so emit_stage_done_best_effort
    // never sees a stale sentinel from a previous test).
    let body = format!(
        r#"case "$1 $2" in
  "pane report-agent") echo "argv: $*" >> "{log}" ;;
  "pane get") echo '{{"id":"cli:pane:get","result":{{"pane":{{"custom_status":"","pane_id":"wA:p3"}}}}}}' ;;
  *) echo ok ;;
esac"#,
        log = log.display()
    );
    let _bin = install_fake_herdr(&bin_dir, &body);

    let id = create_ready_milestone(&env, "150", "bridge report target");

    // Use set_test_env so the parent's PATH/HERDR_PANE_ID are
    // managed through the EnvGuard (avoids stale PATH entries that
    // cause cross-test interference under parallel execution).
    let _env = set_test_env(&bin_dir, Some("wA:p3"));

    // Warm up the fake herdr so its shell cold-start (~220ms on
    // macOS) doesn't run inside the bounded subprocess timeout
    // (500ms). Parallel test load pushes cold-start past the
    // deadline; warm-up amortizes it. The warmup writes a line to
    // the log which we then clear so the assertion below sees only
    // the real `mp milestone complete` call.
    let _ = Command::new(bin_dir.join("herdr"))
        .args(["pane", "report-agent", "warmup"])
        .output();
    let _ = fs::remove_file(&log);

    let out = env.run_with_env(
        &[("PATH", &bin_dir.display().to_string())],
        &[
            "milestone",
            "complete",
            &id,
            "--evidence",
            "bridge-emit-test",
        ],
    );
    assert!(
        out.status.success(),
        "mp milestone complete should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let log_text = fs::read_to_string(&log).unwrap_or_else(|e| {
        panic!(
            "expected herdr-calls.log at {} but read failed: {e}",
            log.display()
        )
    });
    let report_lines: Vec<&str> = log_text
        .lines()
        .filter(|l| l.contains("pane report-agent"))
        .collect();
    assert_eq!(
        report_lines.len(),
        1,
        "mp milestone complete should emit exactly one report-agent call: {report_lines:?}"
    );
    let line = report_lines[0];
    assert!(
        line.contains("--custom-status mp-stage-done"),
        "report-agent argv should pin --custom-status: {line}"
    );
    assert!(
        !line.contains("--seq"),
        "report-agent argv should NOT include --seq (F-05): {line}"
    );
    // Per F-05: --message carries the actual milestone id, not a
    // generic tag. Pin to the specific id created above.
    assert!(
        line.contains(&format!("--message {id}")),
        "report-agent argv must carry the actual milestone id in --message (F-05): {line}"
    );
}

#[test]
fn mp_milestone_complete_outside_herdr_pane_skips_report_agent() {
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = record_log_path(&bin_dir);
    fs::create_dir_all(&bin_dir).unwrap();
    let _bin = install_fake_herdr(
        &bin_dir,
        &format!(r#"echo "argv: $*" >> "{log}""#, log = log.display()),
    );

    let id = create_ready_milestone(&env, "151", "bridge report no-op target");

    // HERDR_PANE_ID is unset → the helper should be a no-op even
    // though herdr IS on PATH. Use set_test_env with HERDR_PANE_ID
    // = None (removes it). The EnvGuard removes our prepended PATH
    // entry on drop.
    let _env = set_test_env(&bin_dir, None);

    // Warm up the fake herdr.
    let _ = Command::new(bin_dir.join("herdr")).output();

    let out = run_mp_with_env(
        &env,
        &[("PATH", &bin_dir.display().to_string())],
        &["HERDR_PANE_ID"],
        &["milestone", "complete", &id, "--evidence", "no-herdr-pane"],
    );
    assert!(
        out.status.success(),
        "mp milestone complete should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let log_text = fs::read_to_string(&log).unwrap_or_default();
    let report_lines: Vec<&str> = log_text
        .lines()
        .filter(|l| l.contains("pane report-agent"))
        .collect();
    assert!(
        report_lines.is_empty(),
        "no report-agent call expected without HERDR_PANE_ID: {report_lines:?}"
    );
}
