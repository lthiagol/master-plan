//! M163: review menu gate — M121 detection, truncation, preflight gate.
//!
//! The acceptance tests pin the AC-01..AC-04 contract for the
//! raul TUI's review-menu error handling without touching the real
//! repository plan or the on-disk `target/debug/mp` binary:
//!
//!   1. `apply_review_menu_enter` formats a focused M121 flash_message
//!      when the fake `mp milestone approve` returns an M121 JSON
//!      payload (AC-01).
//!   2. Long footer text truncates at the first sentence boundary,
//!      includes the details suffix, fits the footer width, and
//!      pressing `?` exposes the full original message (AC-02).
//!   3. Fake `mp plan verify-ac` output with one or more
//!      non-resolvable/unknown ACs sets a closed `preflight_gate` with
//!      the correct unresolvable count (AC-03).
//!   4. The rendered `Approve milestone` text is dim when the gate is
//!      closed and not dim when the gate is open — the search locates
//!      the text by reconstructing each rendered row, not by hunting a
//!      single cell (AC-03).
//!   5. Subprocess failure data emitted on stderr is captured by
//!      `run_raw_capture` and surfaces the focused M121 message even
//!      when stdout is empty (M163 AC-01 + dogfood-log entry 29).
//!   6. Non-M121 failures retain a useful user-facing error (M163
//!      non-M121 contract).
//!
//! The fake `mp` is a shell script in a per-test temp dir. We feed it
//! to `MpRunner::with_mp_bin(...)` so the runner shells out to our
//! scripted binary instead of any real `mp` on PATH. Tests are
//! deterministic, isolated (each gets its own TempDir), and safe to
//! run in parallel (no shared mutable state).

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, InboxLine};
use raul::tui::flash_message;
use raul::tui::inbox_nav::apply_inbox_navigation;
use raul::tui::mode::Mode;
use raul::tui::render;
use raul::tui::view_state;

use tempfile::TempDir;

/// Per-test sandbox: a temp dir that auto-cleans on drop. The fake `mp`
/// script and its companion data files live under `dir.path()`.
struct FakeMp {
    _dir: TempDir,
    bin: PathBuf,
}

impl FakeMp {
    fn new() -> Self {
        let dir = TempDir::new().expect("temp dir");
        let bin = dir.path().join("mp");
        let _ = bin; // assigned in each builder below
        Self { _dir: dir, bin }
    }

    /// Write the fake script (an executable shell program) and return a
    /// ready `MpRunner` pointing at it. The runner gets a no-op
    /// `set_project_root` so `--project-root` is set (defensive — the
    /// script ignores it).
    fn install_script(&mut self, script: &str) -> MpRunner {
        self.bin = self._dir.path().join("mp");
        let mut f = fs::File::create(&self.bin).expect("create fake mp");
        f.write_all(script.as_bytes()).expect("write fake mp");
        f.sync_all().ok();
        drop(f);
        let mut perms = fs::metadata(&self.bin).expect("stat").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&self.bin, perms).expect("chmod");
        let mut runner = MpRunner::with_mp_bin(self.bin.clone());
        runner.set_project_root(self._dir.path().to_path_buf());
        runner
    }
}

/// `mp milestone approve <id>` exits 1 and writes a single M121 JSON
/// payload to *stderr*. Stdout is empty — exactly the case the
/// pre-M163 runner mishandled (the JSON was on stderr and `run_raw`
/// threw away stderr).
const SCRIPT_APPROVE_M121_STDERR: &str = r#"#!/usr/bin/env bash
case "$1 $2" in
    "milestone approve")
        cat >&2 <<'JSON'
{
  "ok": false,
  "errors": [
    {
      "code": "M121",
      "message": "AC AC-09 verification is not gate-passing (unknown): unrecognized command form"
    }
  ]
}
JSON
        exit 1
        ;;
esac
exit 0
"#;

/// `mp milestone approve <id>` exits 1 and writes a single M121 JSON
/// payload to *stdout*. The pre-M163 path handled this case (stdout
/// only) — included so the M163 handler is shown to still work.
const SCRIPT_APPROVE_M121_STDOUT: &str = r#"#!/usr/bin/env bash
# M163 fake: emit M121 failure JSON on stdout, exit 1.
case "$1 $2" in
    "milestone approve")
        echo '{"ok":false,"errors":[{"code":"M121","message":"AC AC-09 verification is not gate-passing (unknown): unrecognized command form"}]}'
        exit 1
        ;;
esac
exit 0
"#;

/// `mp milestone approve <id>` exits 1 and writes a generic non-M121
/// error JSON — the message that reaches the user should still be
/// useful (not the raw 200+ char wall from pre-M163).
const SCRIPT_APPROVE_GENERIC_ERROR: &str = r#"#!/usr/bin/env bash
# M163 fake: emit a non-M121 error JSON, exit 1.
case "$1 $2" in
    "milestone approve")
        echo '{"ok":false,"error":"approval blocked by annotation"}'
        exit 1
        ;;
esac
exit 0
"#;

/// `mp plan verify-ac <id>` emits an `ok: true` top-level with three
/// per-AC entries — `resolved`, `resolved`, `unknown`. M100's real
/// shape; pre-M163 treated the gate as open because `ok: true` and
/// `unresolvable: 0` were the only signals it parsed.
const SCRIPT_VERIFY_AC_HAS_UNKNOWN: &str = r#"#!/usr/bin/env bash
# M163 fake: per-AC statuses — one unknown AC forces a closed gate.
case "$1 $2" in
    "plan verify-ac")
        cat <<'JSON'
{"ok":true,"milestone_id":"163","ac_count":3,"unresolvable":0,"acs":[
  {"ac_id":"AC-01","status":"resolved","verification":"test -f scripts/verify-m-tui-flash-test.sh"},
  {"ac_id":"AC-02","status":"resolved","verification":"test -f scripts/verify-m-tui-flash-truncation.sh"},
  {"ac_id":"AC-03","status":"unknown","verification":"manual: Open raul on M77 or M100"}
]}
JSON
        exit 0
        ;;
esac
exit 0
"#;

/// `mp plan verify-ac <id>` reports all ACs resolved — gate should
/// stay open (the inverse of the unknown case).
const SCRIPT_VERIFY_AC_ALL_RESOLVED: &str = r#"#!/usr/bin/env bash
# M163 fake: all ACs resolved, gate should stay open.
case "$1 $2" in
    "plan verify-ac")
        cat <<'JSON'
{"ok":true,"milestone_id":"163","ac_count":2,"unresolvable":0,"acs":[
  {"ac_id":"AC-01","status":"resolved"},
  {"ac_id":"AC-02","status":"resolved"}
]}
JSON
        exit 0
        ;;
esac
exit 0
"#;

const SCRIPT_VERIFY_AC_RUNTIME_INLINE: &str = r#"#!/usr/bin/env bash
case "$1 $2" in
    "plan verify-ac")
        cat <<'JSON'
{"ok":true,"milestone_id":"163","ac_count":4,"unresolvable":0,"acs":[
  {"ac_id":"AC-01","status":"resolved"},
  {"ac_id":"AC-02","status":"manual"},
  {"ac_id":"AC-03","status":"runtime"},
  {"ac_id":"AC-04","status":"inline"}
]}
JSON
        exit 0
        ;;
esac
exit 0
"#;

const SCRIPT_VERIFY_AC_NONZERO: &str = r#"#!/usr/bin/env bash
case "$1 $2" in
    "plan verify-ac")
        echo 'verify-ac unavailable' >&2
        exit 2
        ;;
esac
exit 0
"#;

const SCRIPT_VERIFY_AC_MALFORMED: &str = r#"#!/usr/bin/env bash
case "$1 $2" in
    "plan verify-ac")
        echo 'not json'
        exit 0
        ;;
esac
exit 0
"#;

/// `mp plan verify-ac <id>` returns exit 0 with a top-level `ok:false`
/// report (e.g. when the gate itself fails). The preflight must close
/// rather than treat the report as a clean pass.
const SCRIPT_VERIFY_AC_OK_FALSE: &str = r#"#!/usr/bin/env bash
case "$1 $2" in
    "plan verify-ac")
        cat <<'JSON'
{"ok":false,"milestone_id":"163","ac_count":1,"unresolvable":0,"acs":[
  {"ac_id":"AC-01","status":"resolved"}
]}
JSON
        exit 0
        ;;
esac
exit 0
"#;

/// `mp plan verify-ac <id>` exits 0 with a top-level `ok:true` but no
/// `acs` array at all (empty report). The preflight must close
/// rather than treat absence of evidence as a clean pass.
const SCRIPT_VERIFY_AC_NO_ACS: &str = r#"#!/usr/bin/env bash
case "$1 $2" in
    "plan verify-ac")
        echo '{"ok":true,"milestone_id":"163","ac_count":0,"unresolvable":0}'
        exit 0
        ;;
esac
exit 0
"#;

/// `mp plan verify-ac <id>` reports a mix — two `unknown` ACs +
/// one `resolved`. M163 must count the unknowns (2), not the
/// reported `unresolvable: 0` top-level field.
const SCRIPT_VERIFY_AC_TWO_UNKNOWN: &str = r#"#!/usr/bin/env bash
case "$1 $2" in
    "plan verify-ac")
        cat <<'JSON'
{"ok":true,"milestone_id":"163","ac_count":3,"unresolvable":0,"acs":[
  {"ac_id":"AC-01","status":"unknown"},
  {"ac_id":"AC-02","status":"resolved"},
  {"ac_id":"AC-03","status":"unknown"}
]}
JSON
        exit 0
        ;;
esac
exit 0
"#;

/// Compose a fake mp that handles both `milestone approve` and
/// `plan verify-ac` in a single binary — used by the AC-04 end-to-end
/// test that drives both surfaces in one test.
fn combined_script(approve: &str, verify_ac: &str) -> String {
    format!(
        "#!/usr/bin/env bash\ncase \"$1 $2\" in\n    \"milestone approve\")\n{}\n        ;;\nesac\ncase \"$1 $2\" in\n    \"plan verify-ac\")\n{}\n        ;;\nesac\nexit 0\n",
        approve.lines().skip(1).collect::<Vec<_>>().join("\n"),
        verify_ac.lines().skip(1).collect::<Vec<_>>().join("\n"),
    )
}

/// Walk every rendered row and check that the substring `needle` is
/// present in the row (assembled from cell symbols). Returns true if
/// any row contains `needle`. This is the post-M163 way to locate text
/// in the buffer — a single ratatui cell holds one symbol, so the
/// pre-M163 test that searched a single cell for `"Approve"` could
/// not work.
fn row_contains(buf: &ratatui::buffer::Buffer, needle: &str) -> bool {
    for y in 0..buf.area().height {
        let mut row = String::new();
        for x in 0..buf.area().width {
            row.push_str(buf[(x, y)].symbol());
        }
        if row.contains(needle) {
            return true;
        }
    }
    false
}

fn text_cell_range(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<(u16, u16, u16)> {
    for y in 0..buf.area().height {
        let mut row = String::new();
        let mut starts = Vec::new();
        for x in 0..buf.area().width {
            starts.push((row.len(), x));
            row.push_str(buf[(x, y)].symbol());
        }
        if let Some(start_byte) = row.find(needle) {
            let end_byte = start_byte + needle.len();
            let start_x = starts
                .iter()
                .find_map(|(byte, x)| (*byte == start_byte).then_some(*x))?;
            let end_x = starts
                .iter()
                .find_map(|(byte, x)| (*byte >= end_byte).then_some(*x))
                .unwrap_or(buf.area().width);
            return Some((y, start_x, end_x));
        }
    }
    None
}

/// Drive `apply_review_menu_enter` to execute the review menu's
/// selected action against `runner`, with `ms_id` as the active
/// milestone. Caller sets up `app` (content + review-menu state).
fn drive_review_enter(app: &mut App, runner: &MpRunner, ms_id: &str) {
    app.selected_milestone_id = Some(ms_id.to_string());
    app.content = raul::tui::app::ContentState::MilestoneDetail;
    app.active_mode = Mode::ReviewMenu(raul::tui::mode::ReviewMenuState {
        items: vec!["Approve milestone".into()],
        selected: 0,
    });
    apply_action(app, runner, Action::ExecuteReviewAction).expect("apply ExecuteReviewAction");
}

// ===========================================================================
// Test 1: AC-01 — focused M121 message on stderr-only emission
// ===========================================================================

#[test]
fn apply_review_menu_enter_emits_focused_message_when_m121_is_on_stderr() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_APPROVE_M121_STDERR);
    let mut app = App::new();
    drive_review_enter(&mut app, &runner, "100");

    let msg = app
        .flash_message
        .as_ref()
        .expect("flash_message should be set after M121 stderr-only");
    // The exact shape is the documented AC-01 contract — must match the
    // regex in AC-04. Stdout was empty (stderr-only emission) and yet
    // the focused message still surfaces because M163 inspects both
    // streams via `run_raw_capture`.
    assert!(
        msg.starts_with("Cannot approve M100"),
        "msg should start with 'Cannot approve M100', got: {msg}"
    );
    assert!(
        matches_focused_m121(msg),
        "msg should match AC-04 anchor pattern; got: {msg}"
    );
    assert!(
        !msg.contains("{\"ok\""),
        "msg must not contain raw JSON; got: {msg}"
    );
    assert!(
        !msg.contains("M121"),
        "msg must not expose the raw error code to the user; got: {msg}"
    );
}

// ===========================================================================
// Test 2: AC-01 — focused M121 message on stdout emission (still works)
// ===========================================================================

#[test]
fn apply_review_menu_enter_emits_focused_message_when_m121_is_on_stdout() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_APPROVE_M121_STDOUT);
    let mut app = App::new();
    drive_review_enter(&mut app, &runner, "77");

    let msg = app.flash_message.expect("flash_message set on M121 stdout");
    assert!(msg.starts_with("Cannot approve M77"), "got: {msg}");
    assert!(
        matches_focused_m121(&msg),
        "AC-04 anchor should match stdout case; got: {msg}"
    );
}

// ===========================================================================
// Test 3: AC-02 — truncation + details hint + `?` exposes full message
// ===========================================================================

#[test]
fn long_flash_truncates_at_sentence_and_question_exposes_full_message() {
    // A real long message — one sentence is too wide for a narrow footer.
    let full = "First sentence. Second sentence is also very long and continues.";
    let _app = App::new();
    let footer = flash_message::format_flash_footer(full, 35);
    // Truncation must include the suffix hint and the trailing space,
    // must not contain the second sentence, and must fit the footer.
    assert!(
        footer.contains("press ? for details"),
        "footer must carry the details hint; got: {footer}"
    );
    assert!(
        footer.ends_with(' '),
        "footer must end with the trailing space; got: {footer}"
    );
    assert!(
        !footer.contains("Second sentence"),
        "footer must not include the second sentence; got: {footer}"
    );
    assert!(
        flash_message::display_width(&footer) <= 35,
        "footer must fit the requested width; got {} cols",
        flash_message::display_width(&footer)
    );

    // When the user presses `?`, the help overlay must surface the
    // *full* original message — the truncated footer is a hint, not
    // the end of the story. Build a synthetic app to drive that path.
    // Use a wider terminal so the centered help overlay (60% width)
    // leaves room for both the static help content and the error body.
    let mut app = App::new();
    app.last_action_error = Some(full.to_string());
    app.active_mode = Mode::Help;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    assert!(
        row_contains(buf, "Last action details"),
        "Help overlay must surface a 'Last action details' section"
    );
    assert!(
        row_contains(buf, "First sentence. Second sentence is also very")
            && row_contains(buf, "long and continues."),
        "Help overlay must render the full original message; got buf:\n{buf:?}"
    );
}

// ===========================================================================
// Test 4: AC-03 — preflight gate is closed when verify-ac has unknown AC
// ===========================================================================

#[test]
fn preflight_gate_is_closed_when_verify_ac_has_unknown_ac() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_VERIFY_AC_HAS_UNKNOWN);
    let gate = raul::tui::runner_helpers::load_preflight_gate(&runner, "163");
    assert!(!gate.open, "gate must be closed when any AC is unknown");
    // One unknown + two resolved = 1 failing AC. The old code used
    // `unresolvable: 0` from the top level and declared the gate open.
    assert_eq!(
        gate.unresolvable_count, 1,
        "unresolvable count must reflect per-AC statuses, not top-level field"
    );
}

#[test]
fn preflight_gate_is_open_when_verify_ac_all_resolved() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_VERIFY_AC_ALL_RESOLVED);
    let gate = raul::tui::runner_helpers::load_preflight_gate(&runner, "163");
    assert!(gate.open, "gate must stay open when every AC is resolved");
    assert_eq!(gate.unresolvable_count, 0);
}

#[test]
fn preflight_gate_counts_multiple_unknown_acs() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_VERIFY_AC_TWO_UNKNOWN);
    let gate = raul::tui::runner_helpers::load_preflight_gate(&runner, "163");
    assert!(!gate.open, "two unknown ACs must close the gate");
    assert_eq!(gate.unresolvable_count, 2);
}

#[test]
fn preflight_gate_accepts_runtime_and_inline_statuses() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_VERIFY_AC_RUNTIME_INLINE);
    let gate = raul::tui::runner_helpers::load_preflight_gate(&runner, "163");
    assert!(gate.open);
    assert_eq!(gate.unresolvable_count, 0);
    assert!(gate.error.is_none());
}

#[test]
fn preflight_gate_closes_and_reports_nonzero_exit() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_VERIFY_AC_NONZERO);
    let gate = raul::tui::runner_helpers::load_preflight_gate(&runner, "163");
    assert!(!gate.open);
    assert!(gate.error.as_deref().is_some_and(|error| {
        error.contains("exited with code 2") && error.contains("verify-ac unavailable")
    }));
}

#[test]
fn opening_review_menu_surfaces_preflight_failure() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_VERIFY_AC_NONZERO);
    let mut app = App::new();
    app.selected_milestone_id = Some("163".to_string());
    app.content = raul::tui::app::ContentState::MilestoneDetail;
    apply_action(&mut app, &runner, Action::OpenReviewMenu).expect("OpenReviewMenu");
    assert!(app.preflight_gate.as_ref().is_some_and(|gate| !gate.open));
    assert!(app
        .flash_message
        .as_deref()
        .is_some_and(|message| message.contains("Approve remains disabled")));
    assert!(app
        .last_action_error
        .as_deref()
        .is_some_and(|details| details.contains("verify-ac unavailable")));
}

#[test]
fn preflight_gate_closes_and_reports_malformed_output() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_VERIFY_AC_MALFORMED);
    let gate = raul::tui::runner_helpers::load_preflight_gate(&runner, "163");
    assert!(!gate.open);
    assert!(gate.error.as_deref().is_some_and(|error| {
        error.contains("malformed JSON") || error.contains("no acceptance criteria")
    }));
}

#[test]
fn preflight_gate_closes_when_top_level_ok_false() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_VERIFY_AC_OK_FALSE);
    let gate = raul::tui::runner_helpers::load_preflight_gate(&runner, "163");
    assert!(!gate.open);
    assert!(gate
        .error
        .as_deref()
        .is_some_and(|error| error.contains("ok=false")));
}

#[test]
fn preflight_gate_closes_when_acs_missing() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_VERIFY_AC_NO_ACS);
    let gate = raul::tui::runner_helpers::load_preflight_gate(&runner, "163");
    assert!(!gate.open);
    assert!(gate
        .error
        .as_deref()
        .is_some_and(|error| error.contains("no acceptance criteria")));
}

#[test]
fn approve_item_renders_dim_when_preflight_gate_is_missing() {
    let mut app = App::new();
    app.selected_milestone_id = Some("163".to_string());
    app.content = raul::tui::app::ContentState::MilestoneDetail;
    app.milestone_detail = Some(serde_json::json!({
        "milestone": {
            "id": "163",
            "title": "Test milestone",
            "lifecycle": "in-progress",
            "spec_status": "ready"
        }
    }));
    app.active_mode = Mode::ReviewMenu(raul::tui::mode::ReviewMenuState {
        items: vec!["Approve milestone".into()],
        selected: 0,
    });
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let palette = app.effective_palette();
    let (y, start_x, end_x) =
        text_cell_range(buf, "Approve milestone").expect("Approve milestone text range");
    assert!(
        (start_x..end_x).all(|x| buf[(x, y)].fg == palette.dim),
        "Approve milestone cells should use the dim foreground when preflight_gate is missing"
    );
}

// ===========================================================================
// Test 5: AC-03 — `Approve milestone` rendered dim when gate is closed
// ===========================================================================

#[test]
fn approve_item_renders_dim_when_preflight_gate_is_closed() {
    // Combine approve (succeeds so we don't pollute the buffer with a
    // flash) + verify-ac (drives the gate state).
    let script = combined_script(
        "# exit 0\n        echo '{\"ok\":true}'\n        exit 0",
        SCRIPT_VERIFY_AC_HAS_UNKNOWN,
    );
    let mut mp = FakeMp::new();
    let runner = mp.install_script(&script);

    let mut app = App::new();
    app.selected_milestone_id = Some("163".to_string());
    app.content = raul::tui::app::ContentState::MilestoneDetail;
    // Drive OpenReviewMenu — this triggers load_preflight_gate.
    apply_action(&mut app, &runner, Action::OpenReviewMenu).expect("OpenReviewMenu");
    assert!(
        app.preflight_gate.is_some(),
        "preflight_gate must be seeded"
    );
    assert!(
        !app.preflight_gate.as_ref().unwrap().open,
        "preflight_gate should be closed (unknown AC in verify-ac)"
    );

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let palette = app.effective_palette();

    let (y, start_x, end_x) =
        text_cell_range(buf, "Approve milestone").expect("Approve milestone text range");
    assert!(
        (start_x..end_x).all(|x| buf[(x, y)].fg == palette.dim),
        "every Approve milestone cell should use the dim foreground when the gate is closed"
    );
}

#[test]
fn approve_item_renders_normal_when_preflight_gate_is_open() {
    let script = combined_script(
        "# exit 0\n        echo '{\"ok\":true}'\n        exit 0",
        SCRIPT_VERIFY_AC_ALL_RESOLVED,
    );
    let mut mp = FakeMp::new();
    let runner = mp.install_script(&script);

    let mut app = App::new();
    app.selected_milestone_id = Some("163".to_string());
    app.content = raul::tui::app::ContentState::MilestoneDetail;
    apply_action(&mut app, &runner, Action::OpenReviewMenu).expect("OpenReviewMenu");
    assert!(app.preflight_gate.as_ref().unwrap().open);

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let palette = app.effective_palette();

    let (y, start_x, end_x) =
        text_cell_range(buf, "Approve milestone").expect("Approve milestone text range");
    assert!(
        (start_x..end_x).all(|x| buf[(x, y)].fg != palette.dim),
        "Approve milestone cells must not be dim when the gate is open"
    );
}

#[test]
fn non_review_flash_clears_stale_details_and_omits_hint() {
    let mut app = App::new();
    app.set_action_error(
        "Review failed. More context follows.",
        "full review details",
    );
    let item = InboxLine {
        id: "bugfix".to_string(),
        kind: "track".to_string(),
        display: "bugfix".to_string(),
        reason: "next".to_string(),
        action: "mp track show bugfix with a deliberately long action string".to_string(),
    };
    apply_inbox_navigation(&mut app, &item);
    assert!(app.last_action_error.is_none());
    let footer = flash_message::format_flash_footer_with_details(
        app.flash_message.as_deref().expect("flash message"),
        35,
        app.last_action_error.is_some(),
    );
    assert!(!footer.contains("press ? for details"));
    assert!(flash_message::display_width(&footer) <= 35);
}

// ===========================================================================
// Test 6: non-M121 failures retain a useful user-facing error
// ===========================================================================

#[test]
fn non_m121_failure_retains_user_facing_error() {
    let mut mp = FakeMp::new();
    let runner = mp.install_script(SCRIPT_APPROVE_GENERIC_ERROR);
    let mut app = App::new();
    drive_review_enter(&mut app, &runner, "200");

    let msg = app
        .flash_message
        .expect("flash_message set on non-M121 failure");
    // The pre-M163 wall was `milestone approve: {"ok":false,"error":...}`.
    // The M163 path produces a focused message — the user-facing label
    // `milestone approve:` is OK to keep as long as the *body* is the
    // actual error, not a 200-char JSON wall.
    assert!(
        msg.contains("approval blocked by annotation"),
        "msg should surface the actual error body, not the JSON wall; got: {msg}"
    );
    assert!(
        !msg.contains("{\"ok\""),
        "msg must not be the raw JSON response; got: {msg}"
    );
    // Non-M121 failures should NOT use the M121-focused prefix.
    assert!(
        !msg.starts_with("Cannot approve M"),
        "non-M121 failure must not use the M121 prefix; got: {msg}"
    );
}

// ===========================================================================
// AC-04 focused-message anchor check
// ===========================================================================

/// Verify `msg` matches the AC-04 anchor pattern
/// `^Cannot approve M\d+: \d+ AC\(s\) have unresolved verifications\. Run: mp plan verify-ac \d+$`.
/// Hand-written matcher — the regex crate is not a dev-dep, and pulling
/// it in just for one AC would balloon the test surface for no benefit.
fn matches_focused_m121(msg: &str) -> bool {
    let prefix = "Cannot approve M";
    if !msg.starts_with(prefix) {
        return false;
    }
    let rest = &msg[prefix.len()..];
    // After `M`, expect digits until `:` — that's the milestone id.
    let (ms_id, after) = match rest.find(':') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => return false,
    };
    if ms_id.is_empty() || !ms_id.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // `:` then ` N AC(s) have unresolved verifications. Run: mp plan verify-ac <id>`
    let tail = after[1..].strip_prefix(' ').unwrap_or(&after[1..]);
    let (ac_count, after) = match tail.find(' ') {
        Some(idx) => (&tail[..idx], &tail[idx..]),
        None => return false,
    };
    if ac_count.is_empty() || !ac_count.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let expected = format!(" AC(s) have unresolved verifications. Run: mp plan verify-ac {ms_id}");
    if !after.starts_with(&expected) {
        return false;
    }
    let after_ms_id = &after[expected.len()..];
    after_ms_id.is_empty()
}

// ===========================================================================
// A no-op smoke test ensuring the FakeMp scaffolding itself works.
// ===========================================================================

#[test]
fn fake_mp_smoke_runs() {
    let mut mp = FakeMp::new();
    let script = r#"#!/usr/bin/env bash
echo "hello from fake mp"
"#;
    let runner = mp.install_script(script);
    let out = runner.run_raw("plan", &["verify-ac", "163"]).expect("run");
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("hello from fake mp"),
        "fake mp should run; got: {s}"
    );
}
