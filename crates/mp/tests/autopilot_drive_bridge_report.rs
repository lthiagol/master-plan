//! M150 S4 / AC-01: bridge report emission tests.
//!
//! Verify that `mp milestone complete` and `mp reviews pass` emit
//! exactly one `herdr pane report-agent` call with the stage-done
//! sentinel when running inside a herdr pane, and zero calls when
//! the env vars are unset. The tests use a fake `herdr` binary on
//! PATH so the assertions are observable end-to-end through the
//! subprocess boundary (not mocked at the library layer).
//!
//! M227 / WP2: the cold-start warmup dance is replaced by the
//! shared [`crate::common::fake_herdr`] harness's `warmup()` +
//! `clear_log()` helpers, which encode the same priming trick as
//! an explicit API. The new test
//! `mp_run_herdr_with_timeout_kills_grandchild` adds the
//! deterministic descendant-termination proof the AC-02 wording
//! calls for: a hung `herdr` that has forked a `sleep` grandchild
//! must have both the parent and the grandchild reaped when the
//! bounded subprocess helper times out, with no fixed sleeps in
//! the test body — readiness synchronization via a pid file.

mod common;

use crate::common::fake_herdr::{FakeHerdr, FakeHerdrBuilder};
use crate::common::{mp_bin, repo_root, TestEnv};
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

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

/// M227 / WP2: warmup + clear-log is the deterministic
/// readiness-synchronization pattern. Pre-spawns the fake herdr so
/// the next `Command::new("herdr")` (from inside `mp milestone
/// complete`) does not pay the shell cold-start cost, then clears
/// the argv log so subsequent assertions see only real calls.
/// Replaces the previous inline `Command::new(...).output()` +
/// `fs::remove_file(&log)` dance.
fn warm_and_clear(fake: &FakeHerdr) {
    fake.warmup();
    fake.clear_log();
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
    let fake = FakeHerdrBuilder::new().install(&bin_dir);

    let _env = set_test_env(&bin_dir, None);

    let emitted = mp::autopilot::drive::emit_stage_done_best_effort(
        "milestone-complete",
        Some("test-milestone"),
    );
    assert!(!emitted, "should be a no-op without HERDR_PANE_ID");

    let log_text = fake.read_log();
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
    let fake = FakeHerdrBuilder::new().install(&bin_dir);

    let _env = set_test_env(&bin_dir, Some("wA:p3"));

    // M227 / WP2: deterministic cold-start priming via the
    // shared harness. Warmup pre-spawns the fake so the real call
    // from `mp milestone complete` does not race on shell
    // cold-start under parallel nextest; clear_log ensures the
    // assertion below sees only the real call.
    warm_and_clear(&fake);

    let emitted = mp::autopilot::drive::emit_stage_done_best_effort(
        "milestone-complete",
        Some("test-milestone"),
    );
    assert!(
        emitted,
        "should have emitted when HERDR_PANE_ID + herdr set"
    );

    let log_text = fake.read_log();
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
    let fake = FakeHerdrBuilder::new()
        .pane_report_agent_failure(1, "no thank you")
        .install(&bin_dir);

    let _env = set_test_env(&bin_dir, Some("wA:p3"));

    // M227 / WP2: deterministic warmup via shared harness.
    warm_and_clear(&fake);

    let emitted = mp::autopilot::drive::emit_stage_done_best_effort(
        "milestone-complete",
        Some("test-milestone"),
    );
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
    let _fake = FakeHerdrBuilder::new()
        .pane_report_agent_failure(2, "broken bridge")
        .install(&bin_dir);

    let new_path = bin_dir.display().to_string();
    let id = create_ready_milestone(&env, "152", "bridge broken-e2e");

    // Use set_test_env so the parent's PATH/HERDR_PANE_ID are
    // managed through the EnvGuard (the previous RestorePathOnly
    // approach leaked the bin_dir entry into the parent's PATH and
    // caused cross-test interference when nextest ran tests in
    // parallel within a binary). HERDR_PANE_ID is intentionally set
    // for parity with the broken-bridge end-to-end.
    let _env = set_test_env(&bin_dir, Some("wA:p3"));

    // M227 / WP2: deterministic warmup via shared harness.
    _fake.warmup();

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
    // Fake herdr: pane report-agent logs argv; pane get returns an
    // envelope without custom_status (so emit_stage_done_best_effort
    // never sees a stale sentinel from a previous test).
    let fake = FakeHerdrBuilder::new()
        .pane_get_response(
            r#"{"id":"cli:pane:get","result":{"pane":{"custom_status":"","pane_id":"wA:p3"}}}"#,
        )
        .install(&bin_dir);

    let id = create_ready_milestone(&env, "150", "bridge report target");

    // Use set_test_env so the parent's PATH/HERDR_PANE_ID are
    // managed through the EnvGuard (avoids stale PATH entries that
    // cause cross-test interference under parallel execution).
    let _env = set_test_env(&bin_dir, Some("wA:p3"));

    // M227 / WP2: deterministic cold-start priming via the shared
    // harness. Warmup amortizes shell cold-start; clear_log
    // ensures the assertion below sees only the real `mp milestone
    // complete` call.
    warm_and_clear(&fake);

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

    let log_text = fake.read_log();
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
    let fake = FakeHerdrBuilder::new().install(&bin_dir);

    let id = create_ready_milestone(&env, "151", "bridge report no-op target");

    // HERDR_PANE_ID is unset → the helper should be a no-op even
    // though herdr IS on PATH. Use set_test_env with HERDR_PANE_ID
    // = None (removes it). The EnvGuard removes our prepended PATH
    // entry on drop.
    let _env = set_test_env(&bin_dir, None);

    // M227 / WP2: deterministic warmup via shared harness.
    fake.warmup();
    fake.clear_log();

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

    let log_text = fake.read_log();
    let report_lines: Vec<&str> = log_text
        .lines()
        .filter(|l| l.contains("pane report-agent"))
        .collect();
    assert!(
        report_lines.is_empty(),
        "no report-agent call expected without HERDR_PANE_ID: {report_lines:?}"
    );
}

// ─── Deterministic process-group timeout (M227 / WP2 / AC-02) ────────────────

/// M227 / WP2 / AC-02: prove that
/// [`mp::autopilot::drive::bridge::run_herdr_with_timeout`] kills the entire
/// process group (parent sh + grandchild sleep) when the deadline
/// fires — without fixed sleeps in the test body.
///
/// **Readiness synchronization:** the fake herdr's `pane get`
/// branch (the bridge poll's subcommand) writes the
/// grandchild's PID to `sleep.pid` before `wait`ing. The helper
/// itself spawns the script and runs `try_wait` every 20 ms;
/// once it has fired `killpg` and `child.wait()` returned, the
/// grandchild must be reaped. The test polls the pid file
/// until it appears (no sleeps — only short poll intervals),
/// joins the helper, and asserts both the timeout error AND the
/// grandchild's absence via `libc::kill(pid, 0) == -1` (ESRCH).
///
/// The unit-level mirror test in
/// `crates/mp/src/autopilot/drive/bridge.rs::run_herdr_with_timeout_kills_entire_process_group`
/// pins the same contract from inside the helper; this test pins
/// it from the integration surface (the helper's public API as
/// invoked by `watch_bridge_report`'s real callers).
#[cfg(unix)]
#[test]
fn mp_run_herdr_with_timeout_kills_grandchild_in_process_group() {
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let pid_file = bin_dir.join("sleep.pid");
    let fake = FakeHerdrBuilder::new()
        .pane_get_grandchild_sleep(30, &pid_file)
        .install(&bin_dir);

    // M227 / WP2: deterministic cold-start priming. The killpg
    // path itself does not depend on cold-start (the helper
    // outlives the warmup), but the warmup keeps the test under
    // the unit's parallel budget.
    fake.warmup();

    // Drive the bounded subprocess helper in a thread; the
    // helper itself spawns the fake herdr script as its child.
    // The script forks `sleep 30 &` (the grandchild we want to
    // prove is reaped) and writes its PID to `pid_file` so we
    // can synchronize on readiness.
    let script_path = fake.path().to_path_buf();
    let helper = std::thread::spawn(move || {
        mp::autopilot::drive::bridge::run_herdr_with_timeout(
            &script_path,
            &["pane", "get", "wA:p3"],
            300,
        )
    });

    // Poll for the pid file (no fixed sleeps — readiness
    // synchronization on the script's own write). 500 × 10 ms
    // gives the script plenty of headroom even under parallel
    // load; the helper fires `killpg` after 300 ms regardless.
    let mut grandchild_pid: i32 = 0;
    for _ in 0..500 {
        if let Ok(s) = std::fs::read_to_string(&pid_file) {
            if let Ok(n) = s.trim().parse::<i32>() {
                if n > 0 {
                    grandchild_pid = n;
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        grandchild_pid > 0,
        "grandchild pid was never written; readiness signal missing: {pid_file:?}"
    );

    let result = helper.join().expect("helper thread panic");
    assert!(
        result.is_err(),
        "bounded helper must time out on a wedged herdr: {result:?}"
    );

    // After the helper returns, the entire process group (sh +
    // sleep grandchild) must be gone. Walk the pid list until
    // ESRCH (kill returns -1) or a small wall-clock budget
    // elapses; the reaping is asynchronous with respect to the
    // helper returning.
    let mut reaped = false;
    for _ in 0..100 {
        // SAFETY: kill(pid, 0) is a liveness probe; no signal is
        // delivered, only the existence check is performed.
        let rc = unsafe { libc::kill(grandchild_pid, 0) };
        if rc == -1 {
            reaped = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        reaped,
        "grandchild pid {grandchild_pid} survived killpg — process-group cleanup is broken"
    );
}
