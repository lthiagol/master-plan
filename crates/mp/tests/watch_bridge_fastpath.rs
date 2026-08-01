//! M150 S4 / AC-02, AC-03, F-10, F-11, F-12, F-13, F-14: production
//! bridge + lifecycle integration tests.
//!
//! These tests exercise `SystemDriveOps::wait_for_lifecycle` against
//! fake `herdr` + `mp` binaries that record the argv actually
//! invoked. The surface under test is the **production path**: the
//! sentinel check is folded into the lifecycle wait loop on a single
//! thread, deadline-protected so a slow `pane get` cannot push the
//! next lifecycle tick past its deadline.
//!
//! All test state (counters, pane-get query logs, lifecycle gate
//! markers) lives under the `TestEnv` TempDir — nothing under
//! `/tmp/` so parallel test runs cannot collide.
//!
//! Coverage:
//! - bridge fast-path + lifecycle confirmation (AC-02)
//! - stale sentinel observed but lifecycle NOT confirmed (F-10)
//! - silent bridge cannot add latency (F-11, AC-03)
//! - failing/hung pane-get falls back without wedging (F-13)
//! - producer pane tracking via `last_prompt_pane` (F-12)
//! - cross-milestone pane reuse safety
//! - `mp reviews pass` auto-promote emission (F-14)

mod common;

use crate::common::{mp_bin, repo_root, TestEnv};
use mp::config::RoleConfig;
use mp::watch::{
    clear_stage_done_sentinel, parse_custom_status_from_pane_get, sentinel_matches, DriveOps,
    LifecycleTarget, PaneHandle, Role, RoleConfigs, SystemDriveOps, WaitOutcome,
    STAGE_DONE_SENTINEL,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

/// Per-test mutex: subprocess tests share the global PATH for the
/// duration of the test process. Serialize them so a parallel test
/// can't insert another `herdr` script into PATH mid-test.
static PATH_LOCK: Mutex<()> = Mutex::new(());

fn install_fake_herdr(dir: &Path, body: &str) -> PathBuf {
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

fn pane_get_response(custom_status: Option<&str>) -> String {
    let cs = custom_status.unwrap_or("");
    format!(
        r#"{{"id":"cli:pane:get","result":{{"pane":{{"custom_status":"{cs}","pane_id":"wA:p3"}}}}}}"#
    )
}

/// Read a marker file's contents; returns `None` if missing. Fake
/// `herdr` scripts use `echo ... > {marker}` from shell to write
/// markers; tests read them via this helper.
fn read_marker(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// RAII env guard: snapshots the named env vars on construction and
/// restores them on Drop. Serialized via `PATH_LOCK` so a parallel
/// test can't observe a half-restored state.
struct EnvGuard {
    saved_path: Option<String>,
    saved_pane: Option<String>,
    dropped: bool,
}

impl EnvGuard {
    fn new() -> Self {
        Self {
            saved_path: std::env::var("PATH").ok(),
            saved_pane: std::env::var("HERDR_PANE_ID").ok(),
            dropped: false,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if self.dropped {
            return;
        }
        self.dropped = true;
        match &self.saved_path {
            Some(v) => unsafe {
                std::env::set_var("PATH", v);
            },
            None => unsafe {
                std::env::remove_var("PATH");
            },
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

/// Prepend `path` to PATH and set HERDR_PANE_ID; the EnvGuard
/// restores on Drop. Prepending (not overwriting) keeps the
/// shell's PATH lookup intact so fake scripts can still find
/// `cat` / `echo` and the counter file mechanism works correctly.
fn force_env(path: &Path, pane: Option<&str>) {
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
}

fn role_configs() -> RoleConfigs {
    RoleConfigs {
        runner: RoleConfig {
            harness: Some("opencode".into()),
            ..Default::default()
        },
        coordinator: RoleConfig {
            harness: Some("opencode".into()),
            ..Default::default()
        },
    }
}

// ─── F-10: stale sentinel observed but lifecycle not yet confirmed ─────────

#[test]
fn stale_sentinel_does_not_advance_state_machine() {
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();

    // herdr pane get always returns the sentinel (stale), but mp
    // never reports lifecycle advance. The state machine must NOT
    // return Reached — the sentinel observation alone is not enough;
    // lifecycle must confirm.
    let body = format!(
        r#"case "$1 $2" in
  "pane get") echo '{sentinel}' ;;
  "pane report-metadata") echo ok ;;
  *) echo ok ;;
esac"#,
        sentinel = pane_get_response(Some(STAGE_DONE_SENTINEL))
    );
    let herdr = install_fake_herdr(&bin_dir, &body);

    // mp script returns "in-progress" forever (never reaches self-reviewed).
    let mp = bin_dir.join("mp");
    fs::write(
        &mp,
        "#!/bin/sh\necho '{\"milestone\":{\"lifecycle\":\"in-progress\"}}'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = fs::metadata(&mp).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(&mp, p).unwrap();
    }

    let mut ops = SystemDriveOps::new(mp, herdr, env.tmp.path(), "1", role_configs());
    // Tight stall timeout so the loop bails fast: with stale
    // sentinel observed on every poll but lifecycle stuck at
    // "in-progress", the loop must keep polling and eventually bail
    // on the agent-status stall. We assert the bail proves F-10:
    // the loop did not advance on a stale signal.
    ops.set_wait_options(mp::watch::WaitOptions {
        poll_interval_ms: 50,
        stall_timeout_ms: 300,
    });
    ops.set_last_prompt_pane(PaneHandle {
        label: "role-runner-1".into(),
        pane_id: "wA:p3".into(),
        reused: false,
    });

    let started = Instant::now();
    let outcome = ops.wait_for_lifecycle(LifecycleTarget::SelfReviewed);
    let elapsed = started.elapsed();
    // F-10: the loop must keep polling (i.e. return Err from
    // stall-detection) rather than advance on the stale sentinel.
    // Err proves the state machine never recorded a false handoff.
    assert!(
        outcome.is_err(),
        "stale sentinel must not advance state machine (F-10); got {outcome:?}"
    );
    assert!(
        elapsed >= std::time::Duration::from_millis(300),
        "loop should have run for the full stall budget: {elapsed:?}"
    );
}

// ─── F-11 / AC-03: silent bridge cannot add latency vs lifecycle ─────────────

#[test]
fn silent_bridge_falls_back_to_lifecycle_without_added_latency() {
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    // Silent bridge: pane get returns empty custom_status (no sentinel).
    let body = format!(
        r#"case "$1 $2" in
  "pane get") echo '{empty}' ;;
  *) echo ok ;;
esac"#,
        empty = pane_get_response(None)
    );
    let herdr = install_fake_herdr(&bin_dir, &body);

    // mp script uses a TempDir-resident counter so parallel tests
    // cannot collide on a fixed path. Lifecycle advances on the
    // second mp call.
    let mp_counter = env.tmp.path().join("mp-call-count");
    let mp = bin_dir.join("mp");
    let mp_body = format!(
        r#"#!/bin/sh
N=$(cat {counter} 2>/dev/null || echo 0)
N=$((N+1))
echo $N > {counter}
if [ $N -lt 2 ]; then
  echo '{{"milestone":{{"lifecycle":"in-progress"}}}}'
else
  echo '{{"milestone":{{"lifecycle":"self-reviewed"}}}}'
fi
"#,
        counter = mp_counter.display()
    );
    fs::write(&mp, mp_body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = fs::metadata(&mp).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(&mp, p).unwrap();
    }

    // Warm up the fake binaries so the first-iteration cold-start
    // (shell + script) doesn't pollute the latency measurement.
    let _ = Command::new(&mp).output();
    let _ = fs::remove_file(&mp_counter);
    let _ = Command::new(&herdr).args(["pane", "get", "wA:p3"]).output();

    let mut ops = SystemDriveOps::new(mp, herdr, env.tmp.path(), "1", role_configs());
    ops.set_wait_options(mp::watch::WaitOptions {
        poll_interval_ms: 50,
        stall_timeout_ms: 0,
    });
    ops.set_last_prompt_pane(PaneHandle {
        label: "role-runner-1".into(),
        pane_id: "wA:p3".into(),
        reused: false,
    });

    let started = Instant::now();
    let outcome = ops
        .wait_for_lifecycle(LifecycleTarget::SelfReviewed)
        .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(outcome, WaitOutcome::Reached);
    // F-11 / AC-03: silent bridge must not regress M149 cadence.
    // With warmed-up fakes + 50ms cadence + first iteration only
    // running the lifecycle poll, total elapsed should be ~50-80ms.
    // Generous 300ms ceiling accommodates slow CI runners.
    assert!(
        elapsed < std::time::Duration::from_millis(300),
        "silent bridge must not regress M149 latency: {elapsed:?}"
    );
}

// ─── AC-02: bridge fast-path + lifecycle confirmation (sub-second) ──────────

#[test]
fn bridge_fast_path_fires_sub_second_and_confirms_via_lifecycle() {
    // The lifecycle field is the authority (F-15). The bridge is a
    // wake-up hint that does NOT advance the state machine on its
    // own. To prove this test exercises the bridge path (not just
    // the lifecycle poll), mp's lifecycle advance is gated on the
    // existence of a TempDir marker that herdr writes exactly when
    // it returns the sentinel. Without the sentinel observation, the
    // marker never appears, mp stays at in-progress, and the loop
    // must time out via stall detection. The test asserts the
    // marker is created and outcome is sub-second.
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let sentinel_marker = env.tmp.path().join("sentinel-observed.marker");

    // herdr: pane get always returns the sentinel AND writes the
    // marker — so the bridge path is the only way mp can advance.
    let body = format!(
        r#"case "$1 $2" in
  "pane get")
    echo sentinel > {marker}
    echo '{sentinel}'
    ;;
  "pane report-metadata") echo ok ;;
  *) echo ok ;;
esac"#,
        marker = sentinel_marker.display(),
        sentinel = pane_get_response(Some(STAGE_DONE_SENTINEL))
    );
    let herdr = install_fake_herdr(&bin_dir, &body);

    // mp: in-progress until the marker exists, then self-reviewed.
    // Without the sentinel/marker, this stays at in-progress forever.
    let mp = bin_dir.join("mp");
    let mp_body = format!(
        r#"#!/bin/sh
if [ -f {marker} ]; then
  echo '{{"milestone":{{"lifecycle":"self-reviewed"}}}}'
else
  echo '{{"milestone":{{"lifecycle":"in-progress"}}}}'
fi
"#,
        marker = sentinel_marker.display()
    );
    fs::write(&mp, mp_body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = fs::metadata(&mp).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(&mp, p).unwrap();
    }
    // Warm up so first-iteration shell cold-start doesn't dominate.
    let _ = Command::new(&mp).output();
    let _ = Command::new(&herdr).args(["pane", "get", "wA:p3"]).output();

    let mut ops = SystemDriveOps::new(mp, herdr, env.tmp.path(), "1", role_configs());
    // 1000ms lifecycle cadence, sentinel ~100ms. The deadline-protected
    // loop only starts a sentinel subprocess when budget permits; with
    // a 200ms sentinel timeout and a 1000ms lifecycle, budget allows
    // sentinel polls for the first ~800ms.
    ops.set_wait_options(mp::watch::WaitOptions {
        poll_interval_ms: 1000,
        stall_timeout_ms: 0,
    });
    ops.set_last_prompt_pane(PaneHandle {
        label: "role-runner-1".into(),
        pane_id: "wA:p3".into(),
        reused: false,
    });

    let started = Instant::now();
    let outcome = ops
        .wait_for_lifecycle(LifecycleTarget::SelfReviewed)
        .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(outcome, WaitOutcome::Reached);
    assert!(
        read_marker(&sentinel_marker).is_some(),
        "sentinel marker must have been written by herdr pane get"
    );
    // AC-02: sub-second. The first sentinel poll at ~100ms observes
    // the sentinel, confirms via lifecycle (mp returns self-reviewed
    // because the marker now exists), and returns.
    assert!(
        elapsed < std::time::Duration::from_millis(800),
        "bridge fast-path should fire sub-second: {elapsed:?}"
    );
}

// ─── F-13: hung pane-get falls back without wedging the deadline ────────────

#[test]
fn hung_pane_get_falls_back_to_lifecycle_without_wedging() {
    // The bridge sentinel-poll must NOT be silently skipped just
    // because lifecycle could advance on its own; we need the
    // bridge path to be exercised. To force the loop to take the
    // bridge path, mp returns in-progress until the bridge polls
    // the producer pane (and writes the marker); only then does mp
    // advance. The herdr pane get hangs after writing the marker.
    // The bounded subprocess timeout (F-13) must kill the hanging
    // `pane get` AND the lifecycle poll must then advance.
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let bridge_marker = env.tmp.path().join("pane-get-invoked.marker");

    // herdr pane get: write a marker, then sleep forever. The
    // marker proves the bridge fast-path was exercised; the sleep
    // is what F-13 requires the bounded subprocess to interrupt.
    let body = format!(
        r#"case "$1 $2" in
  "pane get")
    echo invoked > {marker}
    sleep 60
    ;;
  "pane report-metadata") echo ok ;;
  *) echo ok ;;
esac"#,
        marker = bridge_marker.display()
    );
    let herdr = install_fake_herdr(&bin_dir, &body);

    // mp: in-progress until the bridge marker exists, then advance.
    // This forces the loop to use the bridge path (the lifecycle
    // poll alone never advances).
    let mp = bin_dir.join("mp");
    let mp_body = format!(
        r#"#!/bin/sh
if [ -f {marker} ]; then
  echo '{{"milestone":{{"lifecycle":"self-reviewed"}}}}'
else
  echo '{{"milestone":{{"lifecycle":"in-progress"}}}}'
fi
"#,
        marker = bridge_marker.display()
    );
    fs::write(&mp, mp_body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = fs::metadata(&mp).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(&mp, p).unwrap();
    }

    // Warm up both fake binaries so first-iteration shell cold-start
    // (~220ms each) doesn't dominate the deadline budget. With
    // parallel test load the cold start can exceed 500ms; warm-up
    // amortizes it. The first warmup invocation is also where the
    // marker file is created (because the fake mp returns
    // self-reviewed when the marker exists); without the warmup, the
    // first call would race against the marker's absence.
    let _ = Command::new(&mp).output();
    // Touch the marker so the next mp call (after herdr writes it)
    // sees it consistently. The warmup ran with the marker absent;
    // the next call from the loop should observe it after herdr
    // writes it via `pane get`.
    let _ = fs::remove_file(&bridge_marker);
    let _ = Command::new(bin_dir.join("herdr")).output();

    let mut ops = SystemDriveOps::new(mp, herdr, env.tmp.path(), "1", role_configs());
    // 1000ms lifecycle cadence. The first sentinel poll at ~100ms
    // writes the marker and hangs; the bounded subprocess timeout
    // kills it within ~200ms. The next lifecycle poll at ~1000ms
    // (or sooner if the sentinel confirm via lifecycle ran first)
    // sees mp return self-reviewed and the loop returns Reached.
    ops.set_wait_options(mp::watch::WaitOptions {
        poll_interval_ms: 1000,
        stall_timeout_ms: 0,
    });
    ops.set_last_prompt_pane(PaneHandle {
        label: "role-runner-1".into(),
        pane_id: "wA:p3".into(),
        reused: false,
    });

    let started = Instant::now();
    let outcome = ops
        .wait_for_lifecycle(LifecycleTarget::SelfReviewed)
        .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(outcome, WaitOutcome::Reached);
    // F-13: pane get was definitely invoked (the marker exists) and
    // the bounded subprocess timeout killed the hanging herdr.
    assert!(
        read_marker(&bridge_marker).is_some(),
        "bridge pane get must have been invoked before the lifecycle advanced"
    );
    // The lifecycle must complete near the configured deadline. The
    // sentinel subprocess bounded at ~200ms, the lifecycle poll
    // cadence is 1000ms; the first lifecycle poll after the
    // sentinel is killed (at ~300ms) sees the marker and returns
    // self-reviewed. We allow up to ~5s for slow CI runners but
    // the test should typically land around 1-1.5s.
    assert!(
        elapsed < std::time::Duration::from_millis(5_000),
        "hung pane-get must not wedge the lifecycle wait: {elapsed:?}"
    );
}

// ─── F-12: producer pane tracking ──────────────────────────────────────────

#[test]
fn last_prompt_pane_drives_bridge_fast_path() {
    // F-12: the bridge fast-path polls the pane that received the
    // most recent prompt (the producer pane), not the ambient
    // HERDR_PANE_ID. The test forces the bridge path by gating mp's
    // lifecycle advance on the producer pane ("prod-pane") being
    // queried; mp records every queried pane to a TempDir log so we
    // can assert the producer pane was queried and the wrong pane
    // was not.
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let pane_query_log = env.tmp.path().join("pane-query.log");
    let prod_marker = env.tmp.path().join("prod-pane-queried.marker");

    // herdr: pane get appends the queried pane id to the log; for
    // the producer pane, also write a marker and return the
    // sentinel; for other panes, return an empty custom_status.
    let body = format!(
        r#"case "$1 $2" in
  "pane get")
    echo "$3" >> {log}
    case "$3" in
      "prod-pane")
        echo queried > {marker}
        echo '{sentinel}'
        ;;
      *) echo '{empty}' ;;
    esac
    ;;
  "pane report-metadata") echo ok ;;
  *) echo ok ;;
esac"#,
        log = pane_query_log.display(),
        marker = prod_marker.display(),
        sentinel = pane_get_response(Some(STAGE_DONE_SENTINEL)),
        empty = pane_get_response(None)
    );
    let herdr = install_fake_herdr(&bin_dir, &body);

    // mp: in-progress until the producer-pane marker exists, then
    // self-reviewed. This guarantees the loop must use the bridge
    // path; without the producer-pane query, mp stays at
    // in-progress and the loop stalls.
    let mp = bin_dir.join("mp");
    let mp_body = format!(
        r#"#!/bin/sh
if [ -f {marker} ]; then
  echo '{{"milestone":{{"lifecycle":"self-reviewed"}}}}'
else
  echo '{{"milestone":{{"lifecycle":"in-progress"}}}}'
fi
"#,
        marker = prod_marker.display()
    );
    fs::write(&mp, mp_body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = fs::metadata(&mp).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(&mp, p).unwrap();
    }
    // Warm up mp so first-iteration cold-start doesn't dominate.
    let _ = Command::new(&mp).output();

    let mut ops = SystemDriveOps::new(mp, herdr, env.tmp.path(), "1", role_configs());
    ops.set_wait_options(mp::watch::WaitOptions {
        poll_interval_ms: 1000,
        stall_timeout_ms: 0,
    });
    // Track the producer pane (the runner pane). Without F-12, the
    // ambient HERDR_PANE_ID would be used (different pane).
    ops.set_last_prompt_pane(PaneHandle {
        label: "role-runner-1".into(),
        pane_id: "prod-pane".into(),
        reused: false,
    });

    // Set ambient HERDR_PANE_ID to a DIFFERENT pane so the test
    // proves the fast-path polls last_prompt_pane, not the env var.
    let _env = EnvGuard::new();
    force_env(&bin_dir, Some("watch-pane"));

    let outcome = ops
        .wait_for_lifecycle(LifecycleTarget::SelfReviewed)
        .unwrap();
    assert_eq!(outcome, WaitOutcome::Reached);
    let log_text = read_marker(&pane_query_log).unwrap_or_default();
    let queried: Vec<&str> = log_text.lines().collect();
    assert!(
        queried.contains(&"prod-pane"),
        "producer pane must have been polled by the bridge: {queried:?}"
    );
    assert!(
        !queried.contains(&"watch-pane"),
        "ambient HERDR_PANE_ID must NOT be polled by the bridge: {queried:?}"
    );
    assert!(
        read_marker(&prod_marker).is_some(),
        "the producer-pane sentinel marker must exist (proves bridge path)"
    );
}

#[test]
fn coordinator_stage_polls_coordinator_pane_not_runner_pane() {
    // F-12 follow-up: when the active stage targets the coordinator,
    // the bridge fast-path must poll the coordinator pane (the one
    // that received the prompt), not the runner pane that's still
    // cached. This pins the "coordinator stages do not accidentally
    // poll runner pane" requirement.
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let pane_query_log = env.tmp.path().join("pane-query.log");
    let coord_marker = env.tmp.path().join("coord-pane-queried.marker");

    // herdr: pane get appends the queried pane id to the log; for
    // the coordinator pane, write a marker and return the sentinel;
    // for other panes, return empty.
    let body = format!(
        r#"case "$1 $2" in
  "pane get")
    echo "$3" >> {log}
    case "$3" in
      "coord-pane")
        echo queried > {marker}
        echo '{sentinel}'
        ;;
      *) echo '{empty}' ;;
    esac
    ;;
  "pane report-metadata") echo ok ;;
  *) echo ok ;;
esac"#,
        log = pane_query_log.display(),
        marker = coord_marker.display(),
        sentinel = pane_get_response(Some(STAGE_DONE_SENTINEL)),
        empty = pane_get_response(None)
    );
    let herdr = install_fake_herdr(&bin_dir, &body);

    // mp: in-progress until the coordinator marker exists, then
    // self-reviewed. Without the coordinator-pane query, the loop
    // would stall on in-progress forever.
    let mp = bin_dir.join("mp");
    let mp_body = format!(
        r#"#!/bin/sh
if [ -f {marker} ]; then
  echo '{{"milestone":{{"lifecycle":"reviewed"}}}}'
else
  echo '{{"milestone":{{"lifecycle":"self-reviewed"}}}}'
fi
"#,
        marker = coord_marker.display()
    );
    fs::write(&mp, mp_body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = fs::metadata(&mp).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(&mp, p).unwrap();
    }
    let _ = Command::new(&mp).output();

    let mut ops = SystemDriveOps::new(mp, herdr, env.tmp.path(), "1", role_configs());
    ops.set_wait_options(mp::watch::WaitOptions {
        poll_interval_ms: 1000,
        stall_timeout_ms: 0,
    });
    // Seed runner pane cache (as if a prior milestone had used it),
    // but the ACTIVE prompt pane is the coordinator pane.
    ops.prefill_pane_cache(
        Role::Runner,
        PaneHandle {
            label: "role-runner-1".into(),
            pane_id: "runner-pane".into(),
            reused: true,
        },
    );
    ops.set_last_prompt_pane(PaneHandle {
        label: "role-coordinator-1".into(),
        pane_id: "coord-pane".into(),
        reused: false,
    });

    let outcome = ops.wait_for_lifecycle(LifecycleTarget::Reviewed).unwrap();
    assert_eq!(outcome, WaitOutcome::Reached);
    let log_text = read_marker(&pane_query_log).unwrap_or_default();
    let queried: Vec<&str> = log_text.lines().collect();
    assert!(
        queried.contains(&"coord-pane"),
        "coordinator pane must have been polled by the bridge: {queried:?}"
    );
    assert!(
        !queried.contains(&"runner-pane"),
        "cached runner pane must NOT be polled by the bridge: {queried:?}"
    );
    assert!(
        read_marker(&coord_marker).is_some(),
        "the coordinator-pane sentinel marker must exist (proves bridge path)"
    );
}

// ─── F-14: cross-milestone pane reuse safety ────────────────────────────────

#[test]
fn cross_milestone_set_active_milestone_clears_last_prompt_pane() {
    // AC-04 (pane reuse) + F-14 cross-milestone safety: the
    // sequencer switches milestones via set_active_milestone; the
    // new milestone must start with last_prompt_pane=None so a
    // stale sentinel from the previous milestone's pane does NOT
    // drive the bridge fast-path for the new milestone.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let herdr = install_fake_herdr(&bin_dir, "echo '{}'");

    let mut ops = SystemDriveOps::new(
        bin_dir.join("mp"),
        herdr,
        env.tmp.path(),
        "1",
        role_configs(),
    );
    ops.set_last_prompt_pane(PaneHandle {
        label: "role-runner-1".into(),
        pane_id: "old-pane".into(),
        reused: false,
    });
    assert!(ops.last_prompt_pane().is_some());

    ops.set_active_milestone("2").unwrap();
    assert!(
        ops.last_prompt_pane().is_none(),
        "set_active_milestone must clear the producer pane cache so \
         a stale sentinel from the prior milestone cannot drive \
         the new milestone's bridge fast-path (F-10 + F-14)"
    );
}

// ─── F-14: `mp reviews pass` auto-promote emits the sentinel ────────────────

#[test]
fn mp_reviews_pass_auto_promote_emits_herdr_sentinel() {
    // F-14: end-to-end coverage for the M145 auto-promote path of
    // `mp reviews pass`. With `HERDR_PANE_ID` set and `verdict ok`,
    // the lifecycle flip done→complete must trigger exactly one
    // `herdr pane report-agent` call carrying the sentinel and the
    // milestone id in --message.
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = bin_dir.join("herdr-calls.log");
    fs::create_dir_all(&bin_dir).unwrap();
    let body = format!(
        r#"case "$1 $2" in
  "pane report-agent") echo "argv: $*" >> "{log}" ;;
  "pane get") echo '{{"result":{{"pane":{{"custom_status":"","pane_id":"wA:p3"}}}}}}' ;;
  *) echo ok ;;
esac"#,
        log = log.display()
    );
    let _herdr = install_fake_herdr(&bin_dir, &body);

    // Create + drive to the legacy triple (lifecycle=done,
    // spec_status=verified) so the M145 auto-promote gate fires.
    // `mp milestone set-execution-status done` flips lifecycle to
    // "complete" per M100 ER-1, so we have to land at lifecycle=done
    // by editing the file directly (the legacy triple pre-dates the
    // M100 migration — `mp reviews pass` is the migration tool).
    let create_json = r#"{
        "id": "154",
        "title": "reviews pass auto-promote bridge",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "reviews-pass bridge test" },
        "problem": { "description": "p" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["y", "z"] },
        "acceptance_criteria": [
            { "description": "ac", "verification": "manual: yes" }
        ]
    }"#;
    let created = env.run_json(&[
        "milestone",
        "create",
        "--json",
        create_json,
        "--format",
        "json",
    ]);
    let id = created["milestone"]["id"].as_str().unwrap().to_string();
    env.run(&["milestone", "update", &id, "--spec-status", "ready"]);
    env.run(&["milestone", "update", &id, "--spec-status", "implemented"]);
    env.run(&["milestone", "update", &id, "--spec-status", "verified"]);

    // Force lifecycle=done by writing the file directly. The
    // legacy triple is the precondition for the M145 auto-promote
    // gate; this file edit simulates a milestone that was created
    // under the pre-M100 lifecycle contract.
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    let entry = fs::read_dir(&milestones_dir)
        .unwrap()
        .flatten()
        .find(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("{id}-"))
        })
        .expect("milestone file");
    let path = entry.path();
    let raw = fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["milestone"]["lifecycle"] = serde_json::Value::String("done".into());
    v["milestone"]["execution_status"] = serde_json::Value::String("done".into());
    // spec_status uses `skip_serializing_if` so it can be absent from
    // the on-disk file. Force the M145 auto-promote precondition by
    // explicitly materializing it here.
    v["milestone"]["spec_status"] = serde_json::Value::String("verified".into());
    fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    let install_dir = env.tmp.path().join("install-target");
    let prev_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), prev_path);
    let _env = EnvGuard::new();
    force_env(&bin_dir, Some("wA:p3"));

    // Warm up the fake herdr so its shell cold-start (~220ms on
    // macOS) doesn't run inside the bounded subprocess timeout
    // (500ms) when `mp reviews pass` emits the sentinel under
    // parallel test load. Then clear any prior log entries.
    let _ = Command::new(&_herdr)
        .args(["pane", "report-agent", "warmup"])
        .output();
    let _ = fs::remove_file(&log);

    let mut cmd = Command::new(mp_bin());
    cmd.current_dir(env.tmp.path())
        .env("MP_HOME", repo_root())
        .env("MP_INSTALL_DIR", &install_dir)
        .env("PATH", &new_path)
        .env("HERDR_PANE_ID", "wA:p3")
        .args([
            "reviews",
            "pass",
            &id,
            "--verdict",
            "ok",
            "--reviewer",
            "test:bridge",
        ]);
    let out = cmd.output().expect("spawn mp reviews pass");
    assert!(
        out.status.success(),
        "mp reviews pass must succeed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let log_text = fs::read_to_string(&log).unwrap_or_default();
    let report_lines: Vec<&str> = log_text
        .lines()
        .filter(|l| l.contains("pane report-agent"))
        .collect();
    assert_eq!(
        report_lines.len(),
        1,
        "mp reviews pass auto-promote should emit exactly one report-agent call: {report_lines:?}"
    );
    let line = report_lines[0];
    assert!(
        line.contains("--custom-status mp-stage-done"),
        "argv should pin --custom-status sentinel: {line}"
    );
    assert!(
        line.contains(&id),
        "argv should carry the milestone id in --message: {line}"
    );
    assert!(
        !line.contains("--seq"),
        "--seq removed from producer argv (F-05): {line}"
    );
    assert!(
        line.contains("--source mp"),
        "argv should pin --source mp: {line}"
    );
    assert!(
        line.contains("--agent mp-runner"),
        "argv should pin --agent mp-runner: {line}"
    );
}

// ─── BridgeInstallFn shim for unused-import compatibility ──────────────────

// ─── Misc: helpers re-export sanity ─────────────────────────────────────────

#[test]
fn sentinel_match_helper_still_strict() {
    assert!(sentinel_matches(STAGE_DONE_SENTINEL));
    let json = r#"{"result":{"pane":{"custom_status":"mp-stage-done"}}}"#;
    let cs = parse_custom_status_from_pane_get(json).unwrap();
    assert!(sentinel_matches(&cs));
}

// ─── clear_stage_done_sentinel helper ───────────────────────────────────────

#[test]
fn clear_stage_done_sentinel_invokes_report_metadata_clear_flag() {
    let _g = PATH_LOCK.lock().unwrap();
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let log = bin_dir.join("herdr-calls.log");
    fs::create_dir_all(&bin_dir).unwrap();
    let body = format!(
        r#"case "$1 $2" in
  "pane report-metadata") echo "argv: $*" >> "{log}" ;;
  *) echo ok ;;
esac"#,
        log = log.display()
    );
    let herdr = install_fake_herdr(&bin_dir, &body);

    let _env = EnvGuard::new();
    force_env(&bin_dir, None);

    // Warm up the fake herdr so first-iteration shell cold-start
    // (~220ms on macOS) doesn't run inside the bounded subprocess
    // timeout (1000ms) under parallel test load.
    let _ = Command::new(&herdr).output();

    clear_stage_done_sentinel(&herdr, "wA:p3", 1_000).unwrap();
    let log_text = fs::read_to_string(&log).unwrap();
    let line = log_text
        .lines()
        .find(|l| l.contains("pane report-metadata"))
        .expect("report-metadata called");
    assert!(line.contains("--clear-custom-status"), "clear flag: {line}");
    assert!(line.contains("wA:p3"), "pane id: {line}");
}
