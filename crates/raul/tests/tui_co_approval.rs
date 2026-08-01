use std::io::Write;

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{AnnotationInfo, App, CoApprovalAction, CoApprovalState};
use tempfile::TempDir;

fn approval(status: &str) -> AnnotationInfo {
    AnnotationInfo {
        id: "AN-191".into(),
        target: "191".into(),
        kind: "approval-request".into(),
        status: status.into(),
        author: "reviewer".into(),
        body: "approve".into(),
        created_at: "2026-07-19T12:00:00Z".into(),
        resolved_at: String::new(),
    }
}

struct Fixture {
    _temp: TempDir,
    runner: MpRunner,
    root: std::path::PathBuf,
}

impl Fixture {
    fn new(initial_status: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let script = root.join("mp");
        let log = root.join("calls");
        let status_file = root.join("ann-status");
        std::fs::write(&status_file, initial_status).unwrap();
        // Status-faithful fake mp: reopen accepts resolved only; resolve
        // accepts open|addressed only. Do not mask with always-exit-0 reopen.
        let body = format!(
            r#"#!/bin/sh
echo "$@" >> "{log}"
STATUS_FILE="{status_file}"
status=$(cat "$STATUS_FILE" 2>/dev/null || echo open)
case "$1 $2" in
  "annotation list")
    echo "{{\"annotations\":[{{\"id\":\"AN-191\",\"target\":\"191\",\"kind\":\"approval-request\",\"status\":\"$status\",\"author\":\"reviewer\",\"body\":\"approve\",\"created_at\":\"\",\"resolved_at\":\"\"}}]}}"
    ;;
  "annotation resolve")
    if [ -f "{fail_resolve}" ]; then echo "resolve failed" >&2; exit 7; fi
    case "$status" in
      open|addressed) echo resolved > "$STATUS_FILE"; echo '{{"ok":true}}' ;;
      resolved) echo "annotation AN-191 is already resolved" >&2; exit 1 ;;
      *) echo "cannot resolve annotation AN-191 from status: $status" >&2; exit 1 ;;
    esac
    ;;
  "annotation reopen")
    if [ -f "{fail_reopen}" ]; then echo "reopen failed" >&2; exit 8; fi
    case "$status" in
      resolved) echo open > "$STATUS_FILE"; echo '{{"ok":true}}' ;;
      open) echo "annotation AN-191 is already open" >&2; exit 1 ;;
      addressed) echo "cannot reopen annotation AN-191 from addressed status; only resolved annotations can be reopened" >&2; exit 1 ;;
      *) echo "cannot reopen annotation AN-191 from status: $status" >&2; exit 1 ;;
    esac
    ;;
  "milestone approve")
    if [ -f "{fail_approve}" ]; then echo "approve failed" >&2; exit 9; fi
    echo '{{"ok":true}}'
    ;;
  *) echo '{{"ok":true}}' ;;
esac
"#,
            log = log.display(),
            status_file = status_file.display(),
            fail_resolve = root.join("fail-resolve").display(),
            fail_reopen = root.join("fail-reopen").display(),
            fail_approve = root.join("fail-approve").display(),
        );
        let mut file = std::fs::File::create(&script).unwrap();
        file.write_all(body.as_bytes()).unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).unwrap();
        Self {
            _temp: temp,
            runner: MpRunner::with_mp_bin(script),
            root,
        }
    }

    fn fail(&self, command: &str) {
        std::fs::File::create(self.root.join(format!("fail-{command}"))).unwrap();
    }

    fn recover(&self, command: &str) {
        std::fs::remove_file(self.root.join(format!("fail-{command}"))).unwrap();
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.root.join("calls")).unwrap_or_default()
    }

    fn ann_status(&self) -> String {
        std::fs::read_to_string(self.root.join("ann-status"))
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

fn app(status: &str, choice: Option<CoApprovalAction>) -> App {
    let mut app = App::new();
    app.enter_co_approval(approval(status), "191".into());
    if let Some(choice) = choice {
        app.set_co_approval_action(choice);
    }
    app
}

#[test]
fn co_approval_enter_without_choice_stays_retryable() {
    let fixture = Fixture::new("open");
    let mut app = app("open", None);
    apply_action(&mut app, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    assert_eq!(app.co_approval_state, CoApprovalState::Choosing);
    assert!(app.flash_message.as_deref().unwrap().contains("Choose"));
    assert!(fixture.calls().is_empty());
}

#[test]
fn co_approval_resolve_and_reopen_failures_return_to_choosing() {
    let fixture = Fixture::new("open");
    fixture.fail("resolve");
    let mut approve = app("open", Some(CoApprovalAction::Approve));
    apply_action(&mut approve, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    assert_eq!(approve.co_approval_state, CoApprovalState::Choosing);
    assert!(approve.flash_message.as_deref().unwrap().contains("failed"));

    // Decline (Reject on open) also uses resolve — surface resolve failure.
    let mut decline = app("open", Some(CoApprovalAction::Reject));
    apply_action(&mut decline, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    assert_eq!(decline.co_approval_state, CoApprovalState::Choosing);
    assert!(decline.flash_message.as_deref().unwrap().contains("failed"));

    // Reopen failure: Reject after a resolved annotation (partial-approve path).
    fixture.recover("resolve");
    fixture.fail("reopen");
    let mut reject = app("resolved", Some(CoApprovalAction::Reject));
    apply_action(&mut reject, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    assert_eq!(reject.co_approval_state, CoApprovalState::Choosing);
    assert!(reject.flash_message.as_deref().unwrap().contains("failed"));
}

#[test]
fn co_approval_approve_failure_can_retry_and_confirms_once() {
    let fixture = Fixture::new("open");
    fixture.fail("approve");
    let mut app = app("open", Some(CoApprovalAction::Approve));

    apply_action(&mut app, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    assert_eq!(app.co_approval_state, CoApprovalState::Choosing);
    assert!(app.flash_message.as_deref().unwrap().contains("approve"));

    fixture.recover("approve");
    apply_action(&mut app, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    assert_eq!(app.co_approval_state, CoApprovalState::Confirmed);
    assert!(app.flash_message.is_none());

    apply_action(&mut app, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    let calls = fixture.calls();
    assert_eq!(calls.matches("milestone approve 191").count(), 2);
    assert_eq!(calls.matches("annotation resolve AN-191").count(), 1);
}

#[test]
fn co_approval_reject_declines_open_without_reopen_or_approve() {
    let fixture = Fixture::new("open");
    let mut app = app("open", Some(CoApprovalAction::Reject));
    apply_action(&mut app, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    assert_eq!(app.co_approval_state, CoApprovalState::Confirmed);
    assert_eq!(fixture.ann_status(), "resolved");
    let calls = fixture.calls();
    assert_eq!(calls.matches("annotation resolve AN-191").count(), 1);
    assert_eq!(calls.matches("annotation reopen").count(), 0);
    assert_eq!(calls.matches("milestone approve").count(), 0);
}

#[test]
fn co_approval_reject_declines_addressed_without_reopen() {
    let fixture = Fixture::new("addressed");
    let mut app = app("addressed", Some(CoApprovalAction::Reject));
    apply_action(&mut app, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    assert_eq!(app.co_approval_state, CoApprovalState::Confirmed);
    assert_eq!(fixture.ann_status(), "resolved");
    let calls = fixture.calls();
    assert_eq!(calls.matches("annotation resolve AN-191").count(), 1);
    assert_eq!(calls.matches("annotation reopen").count(), 0);
}

#[test]
fn co_approval_reject_reopens_resolved_only() {
    let fixture = Fixture::new("resolved");
    let mut app = app("resolved", Some(CoApprovalAction::Reject));
    apply_action(&mut app, &fixture.runner, Action::ConfirmCoApproval).unwrap();
    assert_eq!(app.co_approval_state, CoApprovalState::Confirmed);
    assert_eq!(fixture.ann_status(), "open");
    let calls = fixture.calls();
    assert_eq!(calls.matches("annotation reopen AN-191").count(), 1);
    assert_eq!(calls.matches("annotation resolve").count(), 0);
    assert_eq!(calls.matches("milestone approve").count(), 0);
}

#[test]
fn co_approval_fake_mp_reopen_rejects_open_and_addressed() {
    // Proves the fixture is status-faithful: reopen must not always exit 0.
    // Direct runner call (bypassing Reject) so the mask cannot hide behind
    // the decline path.
    let open = Fixture::new("open");
    let err = open
        .runner
        .run_raw("annotation", &["reopen", "AN-191"])
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("already open") || err.contains("exit"),
        "open reopen must fail; got: {err}"
    );
    assert_eq!(open.ann_status(), "open");

    let addressed = Fixture::new("addressed");
    let err = addressed
        .runner
        .run_raw("annotation", &["reopen", "AN-191"])
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("addressed") || err.contains("exit"),
        "addressed reopen must fail; got: {err}"
    );
    assert_eq!(addressed.ann_status(), "addressed");
}
