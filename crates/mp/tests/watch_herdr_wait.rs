//! M149 S4 / AC-02 + S5 / AC-02, AC-03: herdr prompt delivery with
//! readiness gate, and lifecycle completion detection.
//!
//! S4 verifies the readiness-gated `send_prompt` shape against a fake
//! herdr: it waits for `agent wait --status idle`, then issues
//! `agent send <target> <text>` followed by `pane send-keys <pane>
//! Enter`. S5 verifies the lifecycle poll treats plan.json lifecycle
//! as the sole completion gate and tolerates agent-status failures.

mod common;

use crate::common::TestEnv;
use mp::watch::{
    deliver_prompt, lifecycle_advanced_past, read_agent_status, read_lifecycle_via_mp, send_prompt,
    wait_for_lifecycle_with, wait_for_readiness_with, LifecycleTarget, PaneHandle,
    ReadinessOptions, WaitOptions, WaitOutcome,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

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

fn record_log_path(dir: &Path) -> PathBuf {
    dir.join("herdr-calls.log")
}

fn fake_with_logging(log: &Path, body: &str) -> String {
    format!(
        r#"echo "argv: $*" >> "{log}"
{body}"#,
        log = log.display()
    )
}

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
    let log = record_log_path(&bin_dir);
    fs::create_dir_all(&bin_dir).unwrap();
    let bin = install_fake_herdr(&bin_dir, &fake_with_logging(&log, "echo ok"));

    let p = pane("%5");
    deliver_prompt(&bin, &p, "do the thing").unwrap();

    let log_text = fs::read_to_string(&log).unwrap();
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
    let bin = install_fake_herdr(&bin_dir, &body);

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
    let log = record_log_path(&bin_dir);
    fs::create_dir_all(&bin_dir).unwrap();
    // Fake that reports idle on the first agent-wait call.
    let body = fake_with_logging(
        &log,
        r#"case "$2" in
  wait) echo '{"status":"idle"}' ;;
  send) echo ok ;;
  *) echo ok ;;
esac"#,
    );
    let bin = install_fake_herdr(&bin_dir, &body);

    let p = pane("%9");
    let opts = ReadinessOptions {
        timeout_ms: 1_000,
        poll_interval_ms: 1,
    };
    send_prompt(&bin, &p, "go", &opts).unwrap();

    let log_text = fs::read_to_string(&log).unwrap();
    // Readiness call must precede the send.
    let wait_idx = log_text.find("agent wait %9 --status idle").unwrap();
    let send_idx = log_text.find("agent send %9 go").unwrap();
    assert!(wait_idx < send_idx);
}

#[test]
fn wait_for_readiness_times_out_when_never_idle() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    // Fake that always reports working.
    let bin = install_fake_herdr(
        &bin_dir,
        r#"case "$2" in
  wait) echo '{"status":"working"}' ;;
  *) echo ok ;;
esac"#,
    );

    let p = pane("%4");
    // Small timeout + small poll → loop bails in ~50ms of real time.
    // Use real Instant::now (no fake-clock gymnastics) so the timeout
    // check actually fires.
    let opts = ReadinessOptions {
        timeout_ms: 50,
        poll_interval_ms: 5,
    };
    let err = wait_for_readiness_with(&bin, &p, &opts, Instant::now).unwrap_err();
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
    fs::create_dir_all(&bin_dir).unwrap();
    let bin = install_fake_herdr(&bin_dir, r#"echo '{"status":"idle"}'"#);

    let p = pane("%3");
    let opts = ReadinessOptions {
        timeout_ms: 1_000,
        poll_interval_ms: 1,
    };
    wait_for_readiness_with(&bin, &p, &opts, Instant::now).unwrap();
}

#[test]
fn deliver_prompt_propagates_send_failure() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let bin = install_fake_herdr(
        &bin_dir,
        r#"case "$2" in
  send) echo "send failed" 1>&2; exit 2 ;;
  *) echo ok ;;
esac"#,
    );
    let p = pane("%6");
    let err = deliver_prompt(&bin, &p, "text").unwrap_err();
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
    fs::create_dir_all(&bin_dir).unwrap();
    let bin = install_fake_herdr(&bin_dir, r#"echo '{"status":"working"}'"#);
    let p = pane("%7");
    let status = read_agent_status(&bin, &p).unwrap();
    assert_eq!(status, "working");
}

#[test]
fn read_agent_status_falls_back_to_idle_on_zero_exit_success() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    // No JSON, exit 0 —synthesize "idle".
    let bin = install_fake_herdr(&bin_dir, r#"echo "ok""#);
    let p = pane("%8");
    let status = read_agent_status(&bin, &p).unwrap();
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
