//! M149 S4 / AC-02 + S5 / AC-02, AC-03: herdr prompt delivery with
//! readiness gate, and lifecycle completion detection.
//!
//! S4 verifies the readiness-gated `send_prompt` shape against a fake
//! herdr: it waits for `agent wait --status idle`, then issues
//! `agent send <target> <text>` followed by `pane send-keys <pane>
//! Enter`. S5 verifies the lifecycle poll treats plan.json lifecycle
//! as the sole completion gate and tolerates agent-status failures.
//!
//! M227 / WP1: the test-only fake-herdr shell scripts are now built
//! via the shared [`crate::common::fake_herdr`] harness so future
//! autopilot suites can compose off the same primitive without
//! re-deriving a script per test. The one custom script (the binary
//! argv capture for the multiline preservation assertion) stays
//! inline because the binary-format trick is not what the harness
//! is for.

mod common;

use crate::common::fake_herdr::{FakeHerdr, FakeHerdrBuilder};
use crate::common::TestEnv;
use mp::autopilot::drive::{
    deliver_prompt, lifecycle_advanced_past, read_agent_status, read_lifecycle_via_mp, send_prompt,
    wait_for_lifecycle_with, wait_for_readiness_with, LifecycleTarget, PaneHandle,
    ReadinessOptions, WaitOptions, WaitOutcome,
};
use std::fs;
use std::time::{Duration, Instant};

fn pane(id: &str) -> PaneHandle {
    PaneHandle {
        label: format!("label-for-{id}"),
        pane_id: id.to_string(),
        reused: false,
    }
}

// ─── S4: prompt delivery + readiness ──────────────────────────────────────────

#[test]
fn deliver_prompt_issues_send_then_enter() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new().install(&bin_dir);

    let p = pane("%5");
    deliver_prompt(fake.path(), &p, "do the thing").unwrap();

    let log_text = fake.read_log();
    assert!(
        log_text.contains("agent send %5 do the thing"),
        "deliver_prompt should call `agent send <pane> <text>`: {log_text}"
    );
    assert!(
        log_text.contains("pane send-keys %5 Enter"),
        "deliver_prompt should follow with `pane send-keys <pane> Enter`: {log_text}"
    );
    // Ordering: send appears before send-keys.
    let send_idx = log_text.find("agent send").unwrap();
    let keys_idx = log_text.find("pane send-keys").unwrap();
    assert!(send_idx < keys_idx, "send must precede send-keys Enter");
}

#[test]
fn deliver_prompt_preserves_multiline_text_at_herdr_boundary() {
    // Review finding #8: `build_prompt` produces multi-paragraph Markdown
    // with embedded newlines. `deliver_prompt` passes this as a single
    // argv element via .args([...]). The fake herdr captures each
    // invocation's argv by writing it to a binary file using length
    // prefixes (4-byte big-endian length + raw bytes). Records are
    // separated by a zero-length part marker. The test reads back the
    // file, finds the `agent send` record (tag = "agent"), and asserts
    // the prompt (argv index 3) round-trips byte-for-byte. The real
    // herdr accepts argv prompt text; we pin the argv shape here so a
    // switch to a different transport doesn't silently truncate.
    //
    // The binary argv capture is bespoke (the shared harness logs
    // argv as text); we install it directly via the helper's
    // install pattern but with a custom body. Keeping the capture
    // format here pins the boundary contract.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let captured = bin_dir.join("argv.bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let body = format!(
        r#"#!/bin/sh
TAG="$1"
shift
python3 - "$TAG" "$@" <<'PYEOF' >> "{captured}"
import sys, struct
tag = sys.argv[1]
parts = sys.argv[2:]
buf = bytearray()
buf += struct.pack('>I', len(tag))
buf += tag.encode('utf-8')
for part in parts:
    encoded = part.encode('utf-8')
    buf += struct.pack('>I', len(encoded))
    buf += encoded
# Record terminator: a zero-length part.
buf += struct.pack('>I', 0)
sys.stdout.buffer.write(bytes(buf))
PYEOF
"#,
        captured = captured.display()
    );
    let bin = bin_dir.join("herdr");
    fs::write(&bin, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms).unwrap();
    }

    let p = pane("%7");
    let prompt = "# header\n\nline one\nline two\n\n- bullet\n- bullet\n";
    deliver_prompt(&bin, &p, prompt).unwrap();

    let captured_bytes = fs::read(&captured).unwrap_or_default();
    // Records are concatenated: [tag-len:4][tag:N][part-len:4][part:N]...[0-len:4].
    // The zero-length part marks the end of a record.
    let mut records: Vec<(String, Vec<String>)> = Vec::new();
    let mut pos = 0;
    while pos + 4 <= captured_bytes.len() {
        let tag_len = u32::from_be_bytes(captured_bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + tag_len > captured_bytes.len() {
            break;
        }
        let tag = String::from_utf8_lossy(&captured_bytes[pos..pos + tag_len]).into_owned();
        pos += tag_len;
        let mut parts: Vec<String> = Vec::new();
        loop {
            if pos + 4 > captured_bytes.len() {
                break;
            }
            let part_len =
                u32::from_be_bytes(captured_bytes[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;
            if part_len == 0 {
                // End of this record.
                break;
            }
            if pos + part_len > captured_bytes.len() {
                break;
            }
            let part = String::from_utf8_lossy(&captured_bytes[pos..pos + part_len]).into_owned();
            pos += part_len;
            parts.push(part);
        }
        records.push((tag, parts));
    }
    let send_record = records
        .iter()
        .find(|(t, _)| t == "agent")
        .expect("agent send record should be in the log");
    assert!(
        send_record.1.len() >= 3,
        "expected at least 3 argv parts (after the tag); got {:?}",
        send_record.1
    );
    assert_eq!(send_record.1[0], "send");
    assert_eq!(send_record.1[1], "%7");
    assert_eq!(
        send_record.1[2], prompt,
        "deliver_prompt must preserve newlines at the herdr boundary; got {:?}",
        send_record.1[2]
    );
}

#[test]
fn send_prompt_blocks_on_readiness_then_delivers() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    // Fake that reports idle on the first agent-wait call.
    let fake = FakeHerdrBuilder::new()
        .agent_wait_response(r#"{"status":"idle"}"#)
        .install(&bin_dir);

    let p = pane("%9");
    let opts = ReadinessOptions {
        timeout_ms: 1_000,
        poll_interval_ms: 1,
        settle_ms: 0,
    };
    send_prompt(fake.path(), &p, "go", &opts).unwrap();

    let log_text = fake.read_log();
    // Readiness call must precede the send.
    let wait_idx = log_text.find("agent wait %9 --status idle").unwrap();
    let send_idx = log_text.find("agent send %9 go").unwrap();
    assert!(wait_idx < send_idx);
}

#[test]
fn wait_for_readiness_times_out_when_never_idle() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    // Fake that always reports working.
    let fake = FakeHerdrBuilder::new()
        .agent_wait_response(r#"{"status":"working"}"#)
        .install(&bin_dir);

    let p = pane("%4");
    // Small timeout + small poll → loop bails in ~50ms of real time.
    // Use real Instant::now (no fake-clock gymnastics) so the timeout
    // check actually fires.
    let opts = ReadinessOptions {
        timeout_ms: 50,
        poll_interval_ms: 5,
        settle_ms: 0,
    };
    let err = wait_for_readiness_with(fake.path(), &p, &opts, Instant::now).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("readiness timeout") && msg.contains("working"),
        "expected readiness-timeout error mentioning status: {msg}"
    );
}

#[test]
fn wait_for_readiness_returns_when_idle_immediately() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .agent_wait_response(r#"{"status":"idle"}"#)
        .install(&bin_dir);

    let p = pane("%3");
    let opts = ReadinessOptions {
        timeout_ms: 1_000,
        poll_interval_ms: 1,
        settle_ms: 0,
    };
    wait_for_readiness_with(fake.path(), &p, &opts, Instant::now).unwrap();
}

#[test]
fn deliver_prompt_propagates_send_failure() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .agent_send_failure(2, "send failed")
        .install(&bin_dir);

    let p = pane("%6");
    let err = deliver_prompt(fake.path(), &p, "text").unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("agent send failed") && msg.contains("send failed"),
        "error should surface herdr stderr: {msg}"
    );
}

#[test]
fn read_agent_status_parses_json_status_field() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .agent_wait_response(r#"{"status":"working"}"#)
        .install(&bin_dir);

    let p = pane("%7");
    let status = read_agent_status(fake.path(), &p).unwrap();
    assert_eq!(status, "working");
}

#[test]
fn read_agent_status_falls_back_to_idle_on_zero_exit_success() {
    // No JSON, exit 0 → synthesize "idle". The shared harness emits
    // an empty payload from `agent read` and `agent wait` by default;
    // `read_agent_status` parses JSON first and falls back when the
    // shape is unparseable.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new().install(&bin_dir);
    let p = pane("%8");
    let status = read_agent_status(fake.path(), &p).unwrap();
    assert_eq!(status, "idle");
}

// ─── S5: lifecycle completion detection ───────────────────────────────────────

#[test]
fn lifecycle_target_strings_match_plan_dot_json() {
    assert_eq!(LifecycleTarget::InProgress.as_str(), "in-progress");
    assert_eq!(LifecycleTarget::SelfReviewed.as_str(), "self-reviewed");
    assert_eq!(LifecycleTarget::Reviewed.as_str(), "reviewed");
    assert_eq!(LifecycleTarget::Complete.as_str(), "complete");
}

#[test]
fn lifecycle_advanced_past_progression_is_total() {
    // Total order over the watch-driven states.
    let cases = [
        ("approved", LifecycleTarget::InProgress, false),
        ("in-progress", LifecycleTarget::InProgress, false),
        ("self-reviewed", LifecycleTarget::InProgress, true),
        ("reviewed", LifecycleTarget::SelfReviewed, true),
        ("complete", LifecycleTarget::Reviewed, true),
        ("complete", LifecycleTarget::Complete, false),
    ];
    for (cur, target, expected) in cases {
        assert_eq!(
            lifecycle_advanced_past(cur, target),
            expected,
            "advanced_past({cur}, {target:?}) should be {expected}"
        );
    }
}

#[test]
fn wait_for_lifecycle_returns_reached_when_already_at_target() {
    let opts = WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: 0,
    };
    let outcome = wait_for_lifecycle_with(
        || Ok("self-reviewed".to_string()),
        LifecycleTarget::SelfReviewed,
        || Ok("working".to_string()),
        &opts,
        Instant::now,
    )
    .unwrap();
    assert_eq!(outcome, WaitOutcome::Reached);
}

#[test]
fn wait_for_lifecycle_polls_then_reaches() {
    let opts = WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: 0,
    };
    let counter = std::sync::atomic::AtomicU32::new(0);
    let read = || {
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(if n < 3 {
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
fn wait_for_lifecycle_tolerates_status_reader_errors() {
    // Even if the agent-status reader always errors, the wait still
    // completes via the lifecycle reader.
    let opts = WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: 0,
    };
    let outcome = wait_for_lifecycle_with(
        || Ok("complete".to_string()),
        LifecycleTarget::Complete,
        || Err(anyhow::anyhow!("herdr gone")),
        &opts,
        Instant::now,
    )
    .unwrap();
    assert_eq!(outcome, WaitOutcome::Reached);
}

#[test]
fn wait_for_lifecycle_flags_hung_agent_on_status_stall() {
    // Constant status + constant lifecycle + tiny stall → error.
    let opts = WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: 3,
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
        "expected hung-agent error: {msg}"
    );
}

// ─── S5: lifecycle reader via real mp binary ─────────────────────────────────

#[test]
fn read_lifecycle_via_mp_returns_current_state() {
    let env = TestEnv::new();
    let create_json = r#"{
        "title": "lifecycle read target",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": { "outcome": "lifecycle read" },
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

    // Use the test binary as `mp_bin` (it's the same one that ran create).
    let mp_bin = common::mp_bin();
    let lifecycle = read_lifecycle_via_mp(mp_bin, env.tmp.path(), &id).unwrap();
    assert_eq!(
        lifecycle, "draft",
        "a freshly-created milestone should be in draft lifecycle"
    );
}

// ─── M246 WP1 / AC-01: settle window before prompt delivery ────────────────
//
// `read_agent_status` falls back to `idle` whenever `herdr agent wait
// --timeout 0` exits 0 — including the window where a freshly
// spawned pane's harness TUI is still booting. The pre-M246 gate
// returned on the *first* idle read, so a booting harness passed the
// gate and the prompt was silently dropped on the floor.
//
// These tests script the fake herdr with a *sequence* of `agent wait`
// responses and drive `wait_for_readiness_with` through its injected
// `now` closure. The closure reads the fake's call counter, so virtual
// time advances one `TICK` per loop iteration and no test sleeps in
// real time: `agent wait` is the only subprocess inside the loop, so
// the counter is constant within an iteration and `now()` is stable
// between reads.

/// Virtual milliseconds per readiness-loop iteration. The injected
/// clock advances by this much for each `agent wait` call.
const TICK_MS: u64 = 100;

/// Build a `now` closure whose virtual clock is `TICK_MS` per
/// `agent wait` call observed in the fake's counter file.
///
/// `now()` is called several times per loop iteration but
/// `read_agent_status` (which bumps the counter) runs exactly once,
/// so every `now()` inside one iteration returns the same instant.
fn clock_from_counter(fake: &FakeHerdr) -> impl FnMut() -> Instant {
    let path = fake
        .agent_wait_calls_path()
        .expect("scripted fake must expose a counter path")
        .to_path_buf();
    let base = Instant::now();
    move || {
        let calls = fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        base + Duration::from_millis(calls * TICK_MS)
    }
}

/// Count the `agent wait` invocations in the fake's argv log — the
/// number of status reads the gate performed.
fn status_reads(fake: &FakeHerdr) -> usize {
    fake.read_log()
        .lines()
        .filter(|l| l.contains("agent wait"))
        .count()
}

#[test]
fn settle_delivers_only_after_continuous_idle_window() {
    // Sequence `idle, working, idle…`: the first read says idle (the
    // pre-M246 false positive during harness boot), the second says
    // working, and every read after that is idle.
    //
    // A first-idle gate would deliver after ONE status read. The
    // settle gate must instead read past the `working` blip and then
    // hold a full window. With TICK_MS = 100 and settle = 1000ms the
    // uninterrupted-idle case needs 10 reads; this sequence needs
    // more, because the streak only starts on the third read.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .agent_wait_sequence(&[
            r#"{"status":"idle"}"#,
            r#"{"status":"working"}"#,
            r#"{"status":"idle"}"#,
        ])
        .install(&bin_dir);

    let p = pane("%11");
    let opts = ReadinessOptions {
        // Generous: the virtual clock only advances on reads, so a
        // 60s budget can never trip before the streak matures.
        timeout_ms: 60_000,
        poll_interval_ms: 1,
        settle_ms: 1_000,
    };
    wait_for_readiness_with(fake.path(), &p, &opts, clock_from_counter(&fake)).unwrap();

    let reads = status_reads(&fake);
    assert!(
        reads > 10,
        "must read past the first idle AND the working blip, then hold \
         a full {settle}ms window; only {reads} status reads were made",
        settle = opts.settle_ms,
    );
}

#[test]
fn settle_resets_window_on_mid_window_non_idle() {
    // The discriminator between "reads N times" and "re-serves the
    // whole window after a non-idle read".
    //
    // Run A: idle forever → the streak starts on read 1 and
    //        matures after settle/TICK reads.
    // Run B: idle, working, idle… → the streak starts on read 3,
    //        so delivery needs strictly more reads than run A.
    //
    // A settle implementation that failed to reset would report the
    // SAME count for both runs, and this test fails.
    let settle_ms = 1_000u64;
    let runs: [(&str, &[&str]); 2] = [
        ("uninterrupted", &[r#"{"status":"idle"}"#]),
        (
            "mid-window-working",
            &[
                r#"{"status":"idle"}"#,
                r#"{"status":"working"}"#,
                r#"{"status":"idle"}"#,
            ],
        ),
    ];

    let mut counts: Vec<(&str, usize)> = Vec::new();
    for (label, seq) in &runs {
        let env = TestEnv::new();
        let bin_dir = env.tmp.path().join("fake-bin");
        let fake = FakeHerdrBuilder::new()
            .agent_wait_sequence(seq)
            .install(&bin_dir);

        let p = pane("%12");
        let opts = ReadinessOptions {
            timeout_ms: 60_000,
            poll_interval_ms: 1,
            settle_ms,
        };
        wait_for_readiness_with(fake.path(), &p, &opts, clock_from_counter(&fake)).unwrap();
        counts.push((label, status_reads(&fake)));
    }

    let base = counts[0].1;
    let reset = counts[1].1;
    // The streak starts when the first idle read is *served*, so an
    // uninterrupted run matures on read `settle/TICK + 1` (one read
    // to open the window, then settle/TICK ticks to fill it).
    assert_eq!(
        base,
        (settle_ms / TICK_MS) as usize + 1,
        "an uninterrupted idle streak should mature after exactly \
         settle/TICK + 1 reads; got {base}"
    );
    assert!(
        reset > base,
        "a mid-window non-idle read must reset the settle window, so \
         the flaky sequence needs more reads than the uninterrupted \
         one: reset={reset} base={base}"
    );
}

#[test]
fn settle_zero_preserves_legacy_first_idle_behavior() {
    // `settle_ms = 0` is the documented escape hatch: it must return
    // on the FIRST idle read, exactly as the pre-M246 gate did.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .agent_wait_sequence(&[r#"{"status":"idle"}"#])
        .install(&bin_dir);

    let p = pane("%13");
    let opts = ReadinessOptions {
        timeout_ms: 60_000,
        poll_interval_ms: 1,
        settle_ms: 0,
    };
    wait_for_readiness_with(fake.path(), &p, &opts, clock_from_counter(&fake)).unwrap();

    assert_eq!(
        status_reads(&fake),
        1,
        "settle_ms=0 must deliver on the first idle read"
    );
}

#[test]
fn settle_timeout_still_bounds_the_whole_wait() {
    // `timeout_ms` bounds the WHOLE wait, including a streak that is
    // accruing but has not matured. Here the harness reports idle
    // forever while settle (10s) exceeds timeout (2s) — the gate must
    // time out rather than wait out the settle window.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .agent_wait_sequence(&[r#"{"status":"idle"}"#])
        .install(&bin_dir);

    let p = pane("%14");
    let opts = ReadinessOptions {
        timeout_ms: 2_000,
        poll_interval_ms: 1,
        settle_ms: 10_000,
    };
    let err =
        wait_for_readiness_with(fake.path(), &p, &opts, clock_from_counter(&fake)).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("readiness timeout"),
        "a settle window longer than timeout_ms must still time out: {msg}"
    );
    assert_eq!(
        status_reads(&fake),
        (2_000 / TICK_MS) as usize,
        "the wait must stop at timeout_ms even mid-settle"
    );
}

#[test]
fn settle_delays_prompt_delivery_until_window_matures() {
    // End-to-end through `send_prompt` (the composite the state
    // machine actually calls): with a flaky boot sequence the send
    // must not appear until the streak has matured.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    let fake = FakeHerdrBuilder::new()
        .agent_wait_sequence(&[
            r#"{"status":"idle"}"#,
            r#"{"status":"working"}"#,
            r#"{"status":"idle"}"#,
        ])
        .install(&bin_dir);

    let p = pane("%15");
    let opts = ReadinessOptions {
        timeout_ms: 60_000,
        poll_interval_ms: 1,
        settle_ms: 500,
    };
    send_prompt(fake.path(), &p, "go", &opts).unwrap();

    let log = fake.read_log();
    let wait_idx = log
        .find("agent wait %15 --status idle")
        .expect("readiness gate must run");
    let send_idx = log
        .find("agent send %15 go")
        .expect("prompt must be delivered");
    assert!(wait_idx < send_idx, "readiness must precede delivery");
    assert!(
        status_reads(&fake) > 1,
        "send_prompt must not deliver on the first (boot-time) idle read"
    );
}

// ─── M246 WP2 / AC-02: stall rule (deterministic clock) ────────────────────
//
// These tests drive `wait_for_lifecycle_with` through its injected
// `now` closure. Virtual time advances one whole minute per clock
// read, so a 90-minute working run costs a few milliseconds of real
// time and the assertions never depend on iteration counts (the loop
// reads the clock a variable number of times per iteration).
//
// Pre-M246 the rule was "agent_status unchanged for stall_timeout",
// so a build longer than the timeout was reported as hung even while
// the runner was demonstrably working. That regression is pinned
// here.

/// Virtual time per clock read: one minute.
const STALL_TICK: Duration = Duration::from_secs(60);

/// Monotonic virtual clock. Each [`Self::tick`] advances by
/// [`STALL_TICK`]; tests read [`Self::minutes`] to decide when the
/// scripted agent status should change.
struct VirtualClock {
    elapsed: std::cell::Cell<Duration>,
}

impl VirtualClock {
    fn new() -> Self {
        Self {
            elapsed: std::cell::Cell::new(Duration::ZERO),
        }
    }
    fn tick(&self) -> Instant {
        let next = self.elapsed.get() + STALL_TICK;
        self.elapsed.set(next);
        Instant::now() + next
    }
    fn minutes(&self) -> u64 {
        self.elapsed.get().as_secs() / 60
    }
}

/// A 30-minute stall timeout in milliseconds — the production
/// default, spelled out so the arithmetic in these tests is readable.
const STALL_30_MIN_MS: u64 = 30 * 60 * 1_000;

#[test]
fn stall_working_runner_not_stalled() {
    // The headline regression. A runner that reports `working`
    // continuously — a long build or test run — while the lifecycle
    // stays at `in-progress` must not be declared stalled just
    // because it outlasts the stall timeout. 90 minutes of working is
    // 3x the 30-minute timeout and still under the 4x hard ceiling,
    // so this isolates the working rule from the ceiling.
    let opts = WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: STALL_30_MIN_MS,
    };
    let clock = VirtualClock::new();
    let working_for = 90u64;
    let outcome = wait_for_lifecycle_with(
        // The target is a LIFECYCLE state, so the scripted change has
        // to happen on the lifecycle reader: the runner works for 90
        // virtual minutes, then completes the milestone.
        || {
            if clock.minutes() < working_for {
                Ok("in-progress".to_string())
            } else {
                Ok("self-reviewed".to_string())
            }
        },
        // (The ceiling test carries the hang-guard; this one reaches
        // its target at 90m, so it always terminates.)
        LifecycleTarget::SelfReviewed,
        // Agent status never changes for the whole run.
        || Ok("working".to_string()),
        &opts,
        || clock.tick(),
    )
    .expect("a working runner must not be declared stalled");

    assert_eq!(outcome, WaitOutcome::Reached);
    assert!(
        clock.minutes() >= working_for,
        "the loop should have run the full {working_for}m of virtual time; got {}m",
        clock.minutes()
    );
}

#[test]
fn stall_idle_runner_stalls() {
    // The other half, and the guarantee the old rule gave us: a
    // runner that is NOT working and does not advance the lifecycle
    // past the stall timeout IS stalled. 31 minutes of idle against a
    // 30-minute timeout.
    let opts = WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: STALL_30_MIN_MS,
    };
    let clock = VirtualClock::new();
    let err = wait_for_lifecycle_with(
        || Ok("in-progress".to_string()),
        LifecycleTarget::SelfReviewed,
        || Ok("idle".to_string()),
        &opts,
        || clock.tick(),
    )
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("hung") && msg.contains("idle"),
        "an idle runner past the stall timeout must be flagged: {msg}"
    );
    // The stall must fire *at* the configured 30m, not before it
    // (an early fire would mean a healthy runner is flagged) and not
    // anywhere near the 120m hard ceiling. The window is a few
    // minutes wide because the virtual clock advances one tick per
    // loop iteration.
    let fired_at = clock.minutes();
    assert!(
        (30..=35).contains(&fired_at),
        "must stall at ~30m (the configured timeout), not before and not near the 120m ceiling; got {fired_at}m"
    );
}

#[test]
fn stall_hard_ceiling_catches_hung_working_runner() {
    // The ceiling half. Pausing the timer on `working` would make a
    // hung working runner immortal, so an independent ceiling fires at
    // 4 x stall_timeout (120 min) with no progress at any status.
    let opts = WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: STALL_30_MIN_MS,
    };
    let clock = VirtualClock::new();
    let err = wait_for_lifecycle_with(
        // Safety net: if the ceiling ever regresses, the wait would
        // spin forever (a working status never accrues stall time), so
        // the lifecycle completes after 10 virtual hours. That turns a
        // hang in CI into an ordinary assertion failure below.
        || {
            if clock.minutes() >= 600 {
                Ok("self-reviewed".to_string())
            } else {
                Ok("in-progress".to_string())
            }
        },
        LifecycleTarget::SelfReviewed,
        || Ok("working".to_string()),
        &opts,
        || clock.tick(),
    )
    .expect_err("a hung working runner must be caught by the hard ceiling, not run forever");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("hung") && msg.contains("working"),
        "a hung working runner must still be caught by the hard ceiling: {msg}"
    );
    // The loop reads the clock once per iteration, so the ceiling can
    // be observed up to a few minutes past the deadline; what matters
    // is that it fires at ~120m (4 x 30m) rather than never.
    let fired_at = clock.minutes();
    assert!(
        (120..=125).contains(&fired_at),
        "the ceiling must fire at ~120m (4 x 30m); got {fired_at}m"
    );
}

#[test]
fn stall_lifecycle_advance_resets_the_timer() {
    // Progress is progress: a lifecycle change that is not a
    // completion resets the accrued stall time, so a runner that
    // advances the milestone every 20 minutes is never flagged even
    // though its agent-status string never changes.
    let opts = WaitOptions {
        poll_interval_ms: 1,
        stall_timeout_ms: STALL_30_MIN_MS,
    };
    let clock = VirtualClock::new();
    let outcome = wait_for_lifecycle_with(
        || {
            if clock.minutes() < 90 {
                // Advance every 20 virtual minutes. Both values sit
                // below the target, so neither short-circuits the
                // wait — this is genuine forward movement, not a
                // disguised completion.
                if (clock.minutes() / 20) % 2 == 0 {
                    Ok("approved".to_string())
                } else {
                    Ok("in-progress".to_string())
                }
            } else {
                Ok("self-reviewed".to_string())
            }
        },
        LifecycleTarget::SelfReviewed,
        || Ok("idle".to_string()),
        &opts,
        || clock.tick(),
    )
    .expect("an idle runner whose lifecycle keeps advancing is not stalled");
    assert_eq!(outcome, WaitOutcome::Reached);
}
