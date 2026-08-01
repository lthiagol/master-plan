use std::io::Write;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{AnnotationInfo, App, ContentState};
use raul::tui::runner::test_helpers::handle_mouse;
use tempfile::TempDir;

fn annotation(id: &str, status: &str, kind: &str) -> AnnotationInfo {
    AnnotationInfo {
        id: id.into(),
        target: "191".into(),
        kind: kind.into(),
        status: status.into(),
        author: "reviewer".into(),
        body: id.into(),
        created_at: "2026-07-19T12:00:00Z".into(),
        resolved_at: String::new(),
    }
}

fn app_with_mixed_annotations() -> App {
    let mut app = App::new();
    app.selected_milestone_id = Some("191".into());
    app.content = ContentState::AnnotationThread;
    app.load_annotations(vec![
        annotation("AN-resolved", "resolved", "review"),
        annotation("AN-open", "open", "approval-request"),
        annotation("AN-addressed", "addressed", "review"),
    ]);
    app
}

fn fake_mp() -> (TempDir, MpRunner, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let script = temp.path().join("mp");
    let log = temp.path().join("calls");
    let mut file = std::fs::File::create(&script).unwrap();
    let body = format!(
        r#"#!/bin/sh
echo "$@" >> "{}"
if [ "$1 $2" = "annotation list" ]; then
  echo '{{"annotations":[]}}'
else
  echo '{{"ok":true}}'
fi
"#,
        log.display()
    );
    file.write_all(body.as_bytes()).unwrap();
    let mut permissions = file.metadata().unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    (temp, MpRunner::with_mp_bin(script), log)
}

#[test]
fn visible_annotation_selection_preserves_id_and_reanchors_when_hidden() {
    let mut app = app_with_mixed_annotations();
    app.selected_annotation_index = 1;
    assert_eq!(app.selected_annotation().unwrap().id, "AN-open");

    app.toggle_filter();
    assert_eq!(app.selected_annotation().unwrap().id, "AN-open");
    assert_eq!(app.selected_annotation_index, 0);

    app.open_only = false;
    app.selected_annotation_index = 0;
    app.toggle_filter();
    assert_eq!(app.selected_annotation().unwrap().id, "AN-open");
    assert_eq!(app.selected_annotation_index, 0);
}

#[test]
fn visible_annotation_keyboard_actions_target_highlighted_row() {
    let (_temp, runner, log) = fake_mp();
    let mut app = app_with_mixed_annotations();
    app.open_only = true;
    app.selected_annotation_index = 0;

    apply_action(&mut app, &runner, Action::ResolveAnnotation).unwrap();
    let calls = std::fs::read_to_string(log).unwrap();
    assert!(calls.contains("annotation resolve AN-open"));
}

#[test]
fn visible_annotation_reopen_targets_highlighted_row() {
    let (_temp, runner, log) = fake_mp();
    let mut app = app_with_mixed_annotations();
    app.selected_annotation_index = 0;

    apply_action(&mut app, &runner, Action::ReopenAnnotation).unwrap();
    let calls = std::fs::read_to_string(log).unwrap();
    assert!(calls.contains("annotation reopen AN-resolved"));
}

#[test]
fn visible_annotation_co_approval_uses_highlighted_row() {
    let mut app = app_with_mixed_annotations();
    app.open_only = true;
    app.selected_annotation_index = 0;
    let runner = MpRunner::with_mp_bin("/bin/false");

    apply_action(&mut app, &runner, Action::EnterCoApproval).unwrap();
    assert_eq!(app.co_approval_annotation.as_ref().unwrap().id, "AN-open");
}

#[test]
fn visible_annotation_mouse_and_keyboard_share_projection() {
    let mut app = app_with_mixed_annotations();
    app.open_only = true;
    let runner = MpRunner::with_mp_bin("/bin/false");
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: 3,
        modifiers: KeyModifiers::NONE,
    };

    handle_mouse(&mut app, &runner, click, (80, 24)).unwrap();
    assert_eq!(app.selected_annotation().unwrap().id, "AN-open");
}
