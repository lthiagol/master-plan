//! M246 WP3 / AC-04: `mp autopilot tail <id>` — stream one
//! milestone's slice of the activity journal.
//!
//! Pins the CLI contract:
//!
//! - only events whose `subject` is the requested id are printed,
//! - oldest first,
//! - one compact JSON object per line (a stream shape, readable while
//!   the run is still writing),
//! - `--since <rfc3339>` filters out anything older,
//! - `--follow` keeps polling and exits after `--idle` seconds with no
//!   new event.
//!
//! The follow path is driven by an in-process appender: the test
//! spawns `mp autopilot tail --follow` and appends to `activity.json`
//! while it runs, so the tail has to observe an event written after
//! it started. That is the behaviour a hand-rolled `tail -f` on
//! activity.json gives an operator today, and the reason this verb
//! exists.

mod common;

use crate::common::TestEnv;
use serde_json::Value;

/// Append one event to the plan's activity journal.
///
/// There is no `mp activity add` verb (the journal is written by the
/// commands that produce events), so the tests write the file the way
/// a live run does — same schema, monotonically increasing timestamps
/// so stream order is unambiguous even inside one wall-clock second.
fn append_event(env: &TestEnv, subject: &str, summary: &str) {
    let journal = env.tmp.path().join("master-plan/activity.json");
    assert!(
        append_raw_event(&journal, subject, summary),
        "appending to the journal must succeed"
    );
}

/// The timestamp the next appended event will carry, so a test can
/// set `--since` exactly between two events.
fn next_timestamp(env: &TestEnv) -> String {
    event_count(&env.tmp.path().join("master-plan/activity.json"))
        .checked_sub(1)
        .map(stamp_for)
        .unwrap_or_else(|| "2026-01-01T00:00:00+00:00".to_string())
}

fn event_count(journal: &std::path::Path) -> u64 {
    std::fs::read_to_string(journal)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v["events"].as_array().map(|a| a.len() as u64))
        .unwrap_or(0)
}

/// Deterministic, strictly increasing RFC3339 stamp for event `n`.
fn stamp_for(n: u64) -> String {
    format!("2026-01-01T00:{:02}:{:02}+00:00", n / 60, n % 60)
}

/// Parse tail stdout into the event objects it printed.
fn lines(out: &std::process::Output) -> Vec<Value> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad line {l:?}: {e}")))
        .collect()
}

fn run_tail(env: &TestEnv, id: &str, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["autopilot", "tail", id];
    args.extend_from_slice(extra);
    env.run(&args)
}

#[test]
fn tail_prints_only_the_requested_subject_oldest_first() {
    let env = TestEnv::new();
    // Interleave two subjects so a filter that ignores `subject` shows
    // up immediately.
    append_event(&env, "7", "first for 7");
    append_event(&env, "8", "interloper for 8");
    append_event(&env, "7", "second for 7");
    append_event(&env, "9", "another interloper for 9");
    append_event(&env, "7", "third for 7");

    let out = run_tail(&env, "7", &[]);
    assert!(
        out.status.success(),
        "tail must exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let events = lines(&out);
    assert_eq!(events.len(), 3, "only the requested subject is printed");
    for e in &events {
        assert_eq!(e["subject"], "7", "no other subject may leak through");
    }
    let summaries: Vec<&str> = events
        .iter()
        .map(|e| e["summary"].as_str().unwrap())
        .collect();
    assert_eq!(
        summaries,
        vec!["first for 7", "second for 7", "third for 7"],
        "events must be oldest first"
    );
    // Timestamps must be non-decreasing across the stream.
    let stamps: Vec<&str> = events
        .iter()
        .map(|e| e["timestamp"].as_str().unwrap())
        .collect();
    let mut sorted = stamps.clone();
    sorted.sort();
    assert_eq!(stamps, sorted, "stream must be in timestamp order");
}

#[test]
fn tail_respects_since() {
    let env = TestEnv::new();
    append_event(&env, "7", "old one");
    // The boundary is the stamp the next event will carry: --since is
    // exclusive, so it drops "old one" and keeps everything after.
    let latest = next_timestamp(&env);
    append_event(&env, "7", "new one");
    assert!(latest < next_timestamp(&env), "stamps must advance");

    let out = run_tail(&env, "7", &["--since", &latest]);
    let events = lines(&out);
    assert_eq!(
        events.len(),
        1,
        "--since must drop everything at or before the boundary"
    );
    assert_eq!(events[0]["summary"], "new one");
}

#[test]
fn tail_prints_nothing_for_a_milestone_with_no_events() {
    let env = TestEnv::new();
    append_event(&env, "8", "only for 8");

    let out = run_tail(&env, "7", &[]);
    assert!(out.status.success());
    assert!(
        lines(&out).is_empty(),
        "a milestone with no events yields an empty stream, not an error"
    );
}

#[test]
fn tail_follow_exits_after_idle_seconds_with_no_new_events() {
    // Nothing will ever be appended: --follow must give up on its own
    // after --idle seconds rather than hanging forever.
    let env = TestEnv::new();
    append_event(&env, "7", "existing");

    let started = std::time::Instant::now();
    let out = run_tail(&env, "7", &["--follow", "--idle", "1"]);
    let elapsed = started.elapsed();

    assert!(
        out.status.success(),
        "an idle timeout is a normal end, not a failure; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let events = lines(&out);
    assert_eq!(events.len(), 1, "the pre-existing event is still printed");
    assert!(
        elapsed < std::time::Duration::from_secs(20),
        "--follow must exit promptly after --idle; took {elapsed:?}"
    );
}

#[test]
fn tail_follow_picks_up_events_written_while_it_runs() {
    // The point of --follow: the tail starts, then the run appends.
    // The appender is an in-process loop writing to activity.json
    // while the child command is blocked in its poll loop.
    let env = TestEnv::new();
    append_event(&env, "7", "before the follow");

    let journal = env.tmp.path().join("master-plan/activity.json");
    let mp_bin = common::mp_bin();

    let mut child = std::process::Command::new(mp_bin)
        .env("MP_HOME", env.tmp.path())
        .env("MP_INSTALL_DIR", env.tmp.path().join("install-target"))
        .env(
            "MP_VERIFY_TRUST_REPOSITORY",
            common::repo_root().to_string_lossy().to_string(),
        )
        .env("MP_VERIFY_ALLOW_SHELL", "1")
        .arg("--plan-dir")
        .arg(env.tmp.path().join("master-plan"))
        .arg("autopilot")
        .arg("tail")
        .arg("7")
        .arg("--follow")
        .arg("--idle")
        .arg("3")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn tail --follow");

    // Give the tail time to drain the journal and enter its poll loop,
    // then append behind its back. The child is still running here —
    // that is the whole point of the test.
    std::thread::sleep(std::time::Duration::from_millis(600));
    assert!(
        !child.try_wait().expect("try_wait").is_some(),
        "tail --follow must still be running when the event is appended"
    );
    let appended = append_raw_event(&journal, "7", "after the follow started");
    assert!(
        appended,
        "the in-process appender must be able to write the journal"
    );

    let output = child.wait_with_output().expect("wait for tail");
    assert!(
        output.status.success(),
        "follow must exit 0; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = lines(&output);
    let summaries: Vec<&str> = events
        .iter()
        .map(|e| e["summary"].as_str().unwrap())
        .collect();
    assert_eq!(
        summaries,
        vec!["before the follow", "after the follow started"],
        "the follow must print the event appended after it started, and nothing else"
    );
}

/// Append one event to the journal file. Returns false when the
/// write did not land, so a test can fail on "the appender never got
/// its turn" rather than silently asserting against a short stream.
fn append_raw_event(journal: &std::path::Path, subject: &str, summary: &str) -> bool {
    let mut log: Value = std::fs::read_to_string(journal)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({"schema_version": 1, "events": []}));
    if !log["events"].is_array() {
        log["events"] = serde_json::json!([]);
    }
    let n = log["events"]
        .as_array()
        .map(|a| a.len() as u64)
        .unwrap_or(0);
    log["events"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "timestamp": stamp_for(n),
            "type": "lifecycle-transition",
            "subject": subject,
            "summary": summary,
        }));
    std::fs::write(journal, serde_json::to_string_pretty(&log).unwrap()).is_ok()
}
