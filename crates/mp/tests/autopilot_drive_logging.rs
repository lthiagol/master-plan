//! M149 S10 / AC-08: structured logging for `mp watch`.
//!
//! Exercises the DriveLogger end-to-end: entries are JSONL, optional
//! fields are skipped when absent, the parent dir is created on
//! first open, and multiple entries land as separate lines.

mod common;

use mp::autopilot::drive::{DriveLogEntry, DriveLogger};

#[test]
fn open_creates_parent_dir_and_appends_entries_as_jsonl() {
    let env = tempfile::TempDir::new().unwrap();
    let log_path = env.path().join("nested/dir/autopilot.log");
    let logger = DriveLogger::open(&log_path).unwrap();

    logger
        .log(
            &DriveLogEntry::new("stage", "execute milestone")
                .milestone("42")
                .role("runner")
                .pane("%5"),
        )
        .unwrap();
    logger
        .log(&DriveLogEntry::new("transition", "approved->in-progress").milestone("42"))
        .unwrap();

    assert!(log_path.is_file());
    let text = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "two entries → two JSONL lines");
    assert!(lines[0].contains("\"kind\":\"stage\""));
    assert!(lines[0].contains("\"milestone_id\":\"42\""));
    assert!(lines[0].contains("\"role\":\"runner\""));
    assert!(lines[0].contains("\"pane\":\"%5\""));
    // Optional fields are skipped when absent.
    assert!(
        !lines[1].contains("\"role\""),
        "second entry has no role; the field should be omitted"
    );
    assert!(lines[1].contains("\"kind\":\"transition\""));
    // Each line is parseable JSON.
    for line in &lines {
        assert!(serde_json::from_str::<serde_json::Value>(line).is_ok());
    }
}

#[test]
fn log_records_timestamps_in_rfc3339() {
    let env = tempfile::TempDir::new().unwrap();
    let log_path = env.path().join("autopilot.log");
    let logger = DriveLogger::open(&log_path).unwrap();
    logger.log(&DriveLogEntry::new("test", "ts")).unwrap();
    let text = std::fs::read_to_string(&log_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    let ts = v["ts"].as_str().unwrap();
    assert!(ts.starts_with("20"));
    assert!(ts.ends_with('Z'));
    assert_eq!(ts.as_bytes()[10], b'T');
}

#[test]
fn in_memory_logger_captures_entries_in_order() {
    let (logger, buf) = DriveLogger::in_memory();
    for i in 0..5 {
        logger
            .log(&DriveLogEntry::new("iter", format!("entry {i}")))
            .unwrap();
    }
    let captured = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    let lines: Vec<&str> = captured.lines().collect();
    assert_eq!(lines.len(), 5);
    for (i, line) in lines.iter().enumerate() {
        assert!(line.contains(&format!("entry {i}")));
    }
}

#[test]
fn path_helper_returns_the_opened_path() {
    let env = tempfile::TempDir::new().unwrap();
    let log_path = env.path().join("autopilot.log");
    let logger = DriveLogger::open(&log_path).unwrap();
    assert_eq!(logger.path(), log_path);
}

#[test]
fn default_watch_log_path_lives_under_plan_dir_mp_subdir() {
    // Precondition-check default. The default-log-path contract is
    // covered by autopilot_drive_preconditions.rs; this test pins that the
    // logger accepts that path shape without error.
    let env = tempfile::TempDir::new().unwrap();
    let plan_dir = env.path();
    let log_path = mp::autopilot::drive::default_log_path(plan_dir);
    let logger = DriveLogger::open(&log_path).unwrap();
    logger
        .log(&DriveLogEntry::new("boot", "watch starting"))
        .unwrap();
    assert!(log_path.ends_with(".mp/autopilot.log"));
    assert!(log_path.is_file());
}
