//! M184 external-review regressions (F-01..F-02).

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{is_actionable_backlog_id, App, BacklogLine, ContentState, Lane};
use tempfile::TempDir;

fn bl(id: &str, status: &str) -> BacklogLine {
    BacklogLine {
        id: id.into(),
        title: format!("item {id}"),
        priority: "normal".into(),
        status: status.into(),
        resolution: String::new(),
        preview: String::new(),
        ..Default::default()
    }
}

/// F-01: real mp backlog prefix `B-*` must appear on Lane::Backlog.
#[test]
fn m184_f01_b_prefix_visible_on_backlog_lane() {
    let mut app = App::new();
    app.load_backlog(vec![
        bl("B-85", "active"),
        bl("B-87", "active"),
        bl("ID-10", "open"),
        bl("TW-01", "open"),
    ]);
    app.select_lane(Lane::Backlog);
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert!(
        ids.contains(&"B-85"),
        "canonical B-* must show; got {ids:?}"
    );
    assert!(
        ids.contains(&"B-87"),
        "canonical B-* must show; got {ids:?}"
    );
    assert!(ids.contains(&"TW-01"));
    assert!(!ids.contains(&"ID-10"), "ID-* belongs on Ideas");
    assert!(is_actionable_backlog_id("B-85"));
}

/// F-02: load_backlog_detail must read the `backlog` array from
/// `mp list backlog` (not the non-existent `items` key).
#[test]
fn m184_f02_enter_loads_backlog_detail_from_list_shape() {
    let dir = TempDir::new().unwrap();
    let bin = dir.path().join("mp");
    let mut f = fs::File::create(&bin).unwrap();
    f.write_all(
        br#"#!/usr/bin/env bash
if [ "$1" = "list" ] && [ "$2" = "backlog" ]; then
  cat <<'JSON'
{"backlog":[{"id":"B-99","description":"detail target","priority":"high","status":"active","resolution":""}]}
JSON
  exit 0
fi
echo '{}'
exit 0
"#,
    )
    .unwrap();
    f.sync_all().ok();
    drop(f);
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();

    let mut runner = MpRunner::with_mp_bin(&bin);
    runner.set_project_root(dir.path());

    let mut app = App::new();
    app.load_backlog(vec![bl("B-99", "active")]);
    app.select_lane(Lane::Backlog);
    app.selected_index = 0;

    apply_action(&mut app, &runner, Action::Enter).expect("enter");
    assert_eq!(
        app.content,
        ContentState::BacklogDetail,
        "Enter must open backlog detail"
    );
    let detail = app
        .backlog_detail
        .as_ref()
        .expect("F-02: backlog_detail must be populated from list backlog shape");
    assert_eq!(detail["id"].as_str(), Some("B-99"));
    assert_eq!(detail["description"].as_str(), Some("detail target"));
}
