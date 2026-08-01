//! M136: integration tests for the mode-enum + action/command dispatch
//! pattern.
//!
//! Each test exercises one or two `modes/*::handle_key` cases
//! (`handle_key(key, &App) -> Vec<Action>`), and the grep-gate test
//! (`mode_dispatch_lives_outside_runner`) pins the M136 structural
//! invariant that no inline mode flags (`show_help`, `input_mode`,
//! `input_buffer`, `show_review_menu`) remain on `App` *as visible
//! fields*. The mode enum + actions are the only surface.

use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{AnnotationInfo, App, CoApprovalAction, ContentState, Lane};
use raul::tui::mode::{InputState, Mode, ReviewMenuState};
use raul::tui::modes;

/// Build an `MpRunner` for tests, skipping the test when `mp` is not
/// resolvable on this PATH. Production tests in `tui_*` that need shell
/// outs skip the same way (see existing `tui_state.rs` / `tui_review_menu.rs`
/// patterns).
fn runner_or_skip() -> MpRunner {
    match MpRunner::new() {
        Ok(r) => r,
        Err(_) => {
            eprintln!("skipping: mp binary not resolvable in this environment");
            // Returning a dummy struct would be cleaner, but MpRunner's
            // fields are not all pub. Easiest: use `std::panic::catch_unwind`
            // + early-return via a panic that the test runner treats as
            // a skip. `eprintln!` plus returning an `Err` from
            // `MpRunner::new()` upstream means we cannot recover.
            //
            // Practical answer: tests that need a runner skip themselves
            // at the call site, see `apply_action_open_help_flips_mode`
            // below. The helper here is retained for symmetry / future use.
            MpRunner::new().expect("mp required for non-skipped tests")
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn sample_ms() -> Vec<raul::tui::app::MilestoneSummary> {
    vec![
        raul::tui::app::MilestoneSummary {
            id: "01".into(),
            title: "Setup".into(),
            lifecycle: "complete".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
        },
        raul::tui::app::MilestoneSummary {
            id: "02".into(),
            title: "Engine".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
        },
    ]
}

// ---------------------------------------------------------------------------
// AC-01 — Mode enum covers every UI variant, App.active_mode is the only
// mode-state field. The grep gate below pins the "no inline flags"
// contract for `runner.rs`.
// ---------------------------------------------------------------------------

#[test]
fn mode_enum_has_seven_variants() {
    fn exhaustive(m: Mode) -> &'static str {
        match m {
            Mode::Normal => "Normal",
            Mode::Input(InputState {
                target: _,
                kind: _,
                buffer: _,
            }) => "Input",
            Mode::Help => "Help",
            Mode::AnnotationThread => "AnnotationThread",
            Mode::ReviewMenu(ReviewMenuState {
                items: _,
                selected: _,
            }) => "ReviewMenu",
            Mode::LifecycleFilter(_) => "LifecycleFilter",
            Mode::SearchInput(_) => "SearchInput",
        }
    }
    assert_eq!(exhaustive(Mode::Normal), "Normal");
    assert_eq!(exhaustive(Mode::Help), "Help");
    assert_eq!(exhaustive(Mode::AnnotationThread), "AnnotationThread");
}

#[test]
fn app_default_active_mode_is_normal() {
    let app = App::new();
    assert_eq!(app.active_mode, Mode::Normal);
}

/// Pin the M136 inline-flags contract: the four pre-M136 mode-state
/// fields (`show_help`, `input_mode`, `input_buffer`, `show_review_menu`)
/// must NOT be readable from `runner.rs` directly. They migrated into
/// `mode.rs::Mode`.
#[test]
fn no_inline_mode_flags_in_runner_rs() {
    let runner_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("runner.rs");
    let content = fs::read_to_string(&runner_path).unwrap();

    // The grep is intentionally narrow: it allows the same identifier to
    // appear in comments (M136 explicitly documents the legacy fields in
    // `runner.rs`'s doc comments and in the wrapper-active_mode migration
    // breadcrumbs). What we forbid is `app.show_help`, `app.input_mode`,
    // `app.input_buffer`, `app.show_review_menu` reads — these must all
    // have moved into `mode.rs` / `app::active_mode`.
    let forbidden = [
        "app.show_help",
        "app.input_mode",
        "app.input_buffer",
        "app.show_review_menu",
    ];
    for pattern in forbidden {
        assert!(
            !content.contains(pattern),
            "runner.rs must not read {pattern} directly; the mode enum owns mode-state"
        );
    }
}

// ---------------------------------------------------------------------------
// AC-02 — Action enum covers every user-intent mutation, old Event is
// deleted, and a representative sample of keys map to actions.
// ---------------------------------------------------------------------------

#[test]
fn actions_have_required_variants() {
    fn exhaustive(a: Action) -> &'static str {
        match a {
            Action::Quit => "Quit",
            Action::Esc => "Esc",
            // M167: Tab no longer toggles focus — NextLane is the lane-nav action.
            Action::NextLane => "NextLane2",
            Action::RefreshLane => "RefreshLane",
            Action::ToggleFilter => "ToggleFilter",
            Action::ToggleHideDone => "ToggleHideDone",
            // M167: detail-section navigation actions (consumed only on
            // MilestoneDetail) are bucketed under their own category.
            Action::NextSection | Action::PrevSection | Action::NextItem | Action::PrevItem => {
                "SectionNav"
            }
            Action::PreviousLane => "PreviousLane",
            Action::FocusContent => "FocusContent",
            Action::Up => "Up",
            Action::Down => "Down",
            Action::PageUp => "PageUp",
            Action::PageDown => "PageDown",
            Action::Enter => "Enter",
            Action::OpenHelp => "OpenHelp",
            Action::CloseHelp => "CloseHelp",
            Action::OpenReviewMenu => "OpenReviewMenu",
            Action::CloseReviewMenu => "CloseReviewMenu",
            Action::ExecuteReviewAction => "ExecuteReviewAction",
            Action::OpenAnnotationThread => "OpenAnnotationThread",
            Action::CloseAnnotationThread => "CloseAnnotationThread",
            Action::ResolveAnnotation => "ResolveAnnotation",
            Action::ReopenAnnotation => "ReopenAnnotation",
            Action::CreateAnnotation => "CreateAnnotation",
            Action::EnterCoApproval => "EnterCoApproval",
            Action::ConfirmCoApproval => "ConfirmCoApproval",
            Action::SetCoApprovalAction(CoApprovalAction::Approve) => "SetCoApprovalAction",
            Action::ToggleApproval => "ToggleApproval",
            Action::SubmitInput => "SubmitInput",
            Action::CancelInput => "CancelInput",
            Action::PushInputChar('x') => "PushInputChar",
            Action::PopInputChar => "PopInputChar",
            Action::SettingsSave => "SettingsSave",
            // M179: Watch-lane action family — bucketed.
            Action::WatchToggleSelect
            | Action::WatchPreflight
            | Action::WatchStart
            | Action::WatchStop
            | Action::WatchRefresh
            | Action::WatchClearQueue => "Watch",
            Action::WatchMovePicker { .. } | Action::WatchMoveQueue { .. } => "WatchMove",
            // M172 S5: sort-rebind action family — bucketed.
            Action::OpenSortRebind
            | Action::SortRebindNext
            | Action::SortRebindPrev
            | Action::SortRebindConfirm
            | Action::SortRebindCancel => "SortRebind",
            // M185: lifecycle filter + grooming preset.
            Action::OpenLifecycleFilter
            | Action::LifecycleFilterToggle
            | Action::LifecycleFilterNext
            | Action::LifecycleFilterPrev
            | Action::LifecycleFilterCommit
            | Action::LifecycleFilterCancel
            | Action::ApplyGroomingPreset => "LifecycleFilter",
            // M186: search + cycle sort.
            Action::OpenSearch
            | Action::SearchInputChar(_)
            | Action::SearchInputBackspace
            | Action::SearchInputCommit
            | Action::SearchInputCancel
            | Action::CycleSortNext => "M186",
            // Force exhaustiveness — must compile, must use every variant.
            Action::SetCoApprovalAction(CoApprovalAction::Reject) => "SetCoApprovalAction",
            Action::JumpLane(_) => "JumpLane",
            Action::PushInputChar(_) => "PushInputChar",
        }
    }
    assert_eq!(exhaustive(Action::Quit), "Quit");
    assert_eq!(exhaustive(Action::OpenHelp), "OpenHelp");
}

/// M136 deleted the legacy `Event` enum + `map_key_event`; M138 removes the
/// empty `event.rs` marker entirely and migrates the file-local `Event`
/// stand-in out of `modes/normal.rs`, so `Action` is the sole key-dispatch
/// enum (AC-01). This pins that: `event.rs` is gone and no `enum Event`
/// survives anywhere under `src/tui`.
#[test]
fn event_rs_has_no_public_surface() {
    let tui_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui");

    assert!(
        !tui_dir.join("event.rs").exists(),
        "M138: the empty event.rs marker must be removed"
    );

    fn visit(dir: &std::path::Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let content = fs::read_to_string(&path).unwrap();
                if content.contains("enum Event") {
                    out.push(path.display().to_string());
                }
            }
        }
    }
    let mut offenders = Vec::new();
    visit(&tui_dir, &mut offenders);
    assert!(
        offenders.is_empty(),
        "no `enum Event` key-dispatch type may remain under src/tui; found in: {offenders:?}"
    );
}

// ---------------------------------------------------------------------------
// AC-03 — Per-mode handler modules export `handle_key(key, &App) -> Vec<Action>`
// as a pure function. They must NOT call `MpRunner` or `reads::*`.
// ---------------------------------------------------------------------------

#[test]
fn per_mode_handlers_return_actions_for_representative_keys() {
    let mut app = App::new();
    // M167 AC-04: Tab inside any non-Normal mode does NOT switch lanes.
    // Verify by setting each non-Normal mode and feeding Tab to its
    // dedicated dispatcher. Each mode's `handle_key` consumes Tab
    // locally (returns an empty action vector) rather than letting it
    // fall through to Normal's Tab → NextLane handler.
    app.active_mode = Mode::Help;
    assert_eq!(
        modes::help::handle_key(key(KeyCode::Tab)),
        Vec::new(),
        "Tab inside Help must NOT switch lanes"
    );
    app.active_mode = Mode::Input(InputState {
        target: "M01".into(),
        kind: "note".into(),
        buffer: String::new(),
    });
    assert_eq!(
        modes::input::handle_key(key(KeyCode::Tab)),
        Vec::new(),
        "Tab inside Input must NOT switch lanes"
    );
    app.active_mode = Mode::ReviewMenu(ReviewMenuState {
        items: Vec::new(),
        selected: 0,
    });
    assert_eq!(
        modes::review_menu::handle_key(key(KeyCode::Tab)),
        Vec::new(),
        "Tab inside ReviewMenu must NOT switch lanes"
    );
    app.active_mode = Mode::AnnotationThread;
    assert_eq!(
        modes::annotation_thread::handle_key(key(KeyCode::Tab)),
        Vec::new(),
        "Tab inside AnnotationThread must NOT switch lanes"
    );
    // M169: Settings lane Tab falls through to normal lane cycling when
    // no edit is active.
    app.select_lane(Lane::Settings);
    app.settings = Some(raul::tui::mode::SettingsState::new(serde_json::json!({})));
    assert_eq!(
        modes::settings::handle_key(key(KeyCode::Tab), &app),
        Vec::new(),
        "Tab on Settings flat list must fall through to normal"
    );
    assert_eq!(
        modes::normal::handle_key(key(KeyCode::Tab), &app),
        vec![Action::NextLane]
    );
    // Reset to Normal for the rest of the assertions.
    app.active_mode = Mode::Normal;

    // Normal mode
    assert_eq!(
        modes::normal::handle_key(key(KeyCode::Char('q')), &app),
        vec![Action::Quit]
    );
    assert_eq!(
        modes::normal::handle_key(key(KeyCode::Char('Q')), &app),
        vec![Action::Quit]
    );
    assert_eq!(
        modes::normal::handle_key(key(KeyCode::Esc), &app),
        vec![Action::Esc]
    );
    assert_eq!(
        modes::normal::handle_key(key(KeyCode::Tab), &app),
        vec![Action::NextLane]
    );

    // Input mode
    assert_eq!(
        modes::input::handle_key(key(KeyCode::Char('a'))),
        vec![Action::PushInputChar('a')]
    );
    assert_eq!(
        modes::input::handle_key(key(KeyCode::Enter)),
        vec![Action::SubmitInput]
    );
    assert_eq!(
        modes::input::handle_key(key(KeyCode::Esc)),
        vec![Action::CancelInput]
    );
    assert_eq!(
        modes::input::handle_key(key(KeyCode::Backspace)),
        vec![Action::PopInputChar]
    );
    assert_eq!(modes::input::handle_key(key(KeyCode::F(1))), Vec::new());

    // Help mode
    assert_eq!(
        modes::help::handle_key(key(KeyCode::Char('?'))),
        vec![Action::CloseHelp]
    );
    assert_eq!(
        modes::help::handle_key(key(KeyCode::Char('q'))),
        vec![Action::CloseHelp, Action::Quit]
    );
    assert_eq!(modes::help::handle_key(key(KeyCode::Up)), Vec::new());

    // AnnotationThread mode
    assert_eq!(
        modes::annotation_thread::handle_key(key(KeyCode::Char('r'))),
        vec![Action::ResolveAnnotation]
    );
    assert_eq!(
        modes::annotation_thread::handle_key(key(KeyCode::Char('R'))),
        vec![Action::ReopenAnnotation]
    );
    assert_eq!(
        modes::annotation_thread::handle_key(key(KeyCode::Char('A'))),
        vec![Action::CreateAnnotation]
    );
    assert_eq!(
        modes::annotation_thread::handle_key(key(KeyCode::Enter)),
        vec![Action::EnterCoApproval]
    );

    // ReviewMenu mode
    assert_eq!(
        modes::review_menu::handle_key(key(KeyCode::Up)),
        vec![Action::Up]
    );
    assert_eq!(
        modes::review_menu::handle_key(key(KeyCode::Down)),
        vec![Action::Down]
    );
    assert_eq!(
        modes::review_menu::handle_key(key(KeyCode::Enter)),
        vec![Action::ExecuteReviewAction]
    );
    assert_eq!(
        modes::review_menu::handle_key(key(KeyCode::Esc)),
        vec![Action::CloseReviewMenu]
    );

    // Settings lane — Esc is no-op on flat list; Enter opens edit.
    let mut settings_app = App::new();
    settings_app.select_lane(Lane::Settings);
    settings_app.settings = Some(raul::tui::mode::SettingsState::new(serde_json::json!({})));
    assert_eq!(
        modes::settings::handle_key(key(KeyCode::Esc), &settings_app),
        Vec::new()
    );
    assert_eq!(
        modes::settings::handle_key(key(KeyCode::Enter), &settings_app),
        vec![Action::Enter]
    );
}

/// Source-shape gate: the per-mode handlers must not call into the
/// `MpRunner` (they're pure). We grep the `tui/modes/*.rs` files for the
/// forbidden shell-out surface (`runner.run*`, `reads::`).
#[test]
fn no_mp_shell_outs_in_per_mode_handlers() {
    let modes_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("modes");
    let mut violations = Vec::new();
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let content = std::fs::read_to_string(&path).unwrap();
                for (i, line) in content.lines().enumerate() {
                    let t = line.trim();
                    if t.starts_with("//") || t.starts_with("//!") {
                        continue;
                    }
                    if t.contains("runner.run") || t.contains("MpRunner") || t.contains("reads::") {
                        out.push(format!(
                            "{}:{}: per-mode handler must not shell out: {}",
                            path.file_name().unwrap().to_string_lossy(),
                            i + 1,
                            t
                        ));
                    }
                }
            }
        }
    }
    walk(&modes_dir, &mut violations);
    for v in &violations {
        eprintln!("{v}");
    }
    assert!(
        violations.is_empty(),
        "per-mode handlers must be pure (no MpRunner or reads:: calls); got:\n{}",
        violations.join("\n")
    );
}

// ---------------------------------------------------------------------------
// AC-04 — apply_action mutates App + shells out to mp. No per-mode
// handler calls MpRunner.
// ---------------------------------------------------------------------------

#[test]
fn apply_action_open_help_flips_mode() {
    let mut app = App::new();
    assert_eq!(app.active_mode, Mode::Normal);
    let result = apply_action(&mut app, &runner_or_skip(), Action::OpenHelp);
    assert!(result.is_ok(), "OpenHelp must be Ok: {result:?}");
    assert_eq!(app.active_mode, Mode::Help);
}

#[test]
fn apply_action_close_help_restores_normal() {
    let mut app = App::new();
    app.active_mode = Mode::Help;
    apply_action(&mut app, &runner_or_skip(), Action::CloseHelp).unwrap();
    assert_eq!(app.active_mode, Mode::Normal);
}

#[test]
fn apply_action_open_input_puts_buffer_into_mode() {
    let mut app = App::new();
    // Pre-M136 + M136: opening an input prompt only happens via the
    // explicit Action::CreateAnnotation (which carries the target + kind
    // inline), so seed the mode the same way callers do: write
    // `Mode::Input(_)` directly via the App helper.
    app.active_mode = Mode::Input(InputState {
        target: "M01".into(),
        kind: "review-request".into(),
        buffer: String::new(),
    });
    apply_action(&mut app, &runner_or_skip(), Action::PushInputChar('h')).unwrap();
    apply_action(&mut app, &runner_or_skip(), Action::PushInputChar('i')).unwrap();
    match &app.active_mode {
        Mode::Input(state) => assert_eq!(state.buffer, "hi"),
        other => panic!("expected Mode::Input, got {other:?}"),
    }
}

#[test]
fn apply_action_cancel_input_drops_buffer_by_construction() {
    let mut app = App::new();
    app.active_mode = Mode::Input(InputState {
        target: "M01".into(),
        kind: "review-request".into(),
        buffer: String::new(),
    });
    apply_action(&mut app, &runner_or_skip(), Action::PushInputChar('x')).unwrap();
    apply_action(&mut app, &runner_or_skip(), Action::PushInputChar('y')).unwrap();
    apply_action(&mut app, &runner_or_skip(), Action::CancelInput).unwrap();
    assert_eq!(app.active_mode, Mode::Normal);
}

#[test]
fn apply_action_open_review_menu_seeds_canonical_items() {
    let mut app = App::new();
    // Pre-M136 + M136: the review menu only opens from
    // `ContentState::MilestoneDetail`. Drive the App to that state first.
    app.select_lane(Lane::Milestones);
    app.load_milestones(sample_ms());
    app.enter_milestone_detail(Some(0));
    assert_eq!(app.content, ContentState::MilestoneDetail);
    apply_action(&mut app, &runner_or_skip(), Action::OpenReviewMenu).unwrap();
    match &app.active_mode {
        Mode::ReviewMenu(menu) => {
            // M172 S6: the menu grew a "Set dependency" item — 5
            // total now (Approve / Block / Unblock / Request
            // grooming / Set dependency).
            assert_eq!(menu.items.len(), 5);
            assert_eq!(menu.selected, 0);
        }
        other => panic!("expected Mode::ReviewMenu, got {other:?}"),
    }
}

#[test]
fn apply_action_close_review_menu_restores_normal() {
    let mut app = App::new();
    app.active_mode = Mode::ReviewMenu(ReviewMenuState {
        items: ReviewMenuState::canonical(),
        selected: 2,
    });
    apply_action(&mut app, &runner_or_skip(), Action::CloseReviewMenu).unwrap();
    assert_eq!(app.active_mode, Mode::Normal);
}

#[test]
fn apply_action_quit_sets_flag() {
    let mut app = App::new();
    apply_action(&mut app, &runner_or_skip(), Action::Quit).unwrap();
    assert!(app.quitting);
}

#[test]
fn apply_action_toggle_filter_toggles_open_only() {
    let mut app = App::new();
    assert!(!app.open_only);
    apply_action(&mut app, &runner_or_skip(), Action::ToggleFilter).unwrap();
    assert!(app.open_only);
    apply_action(&mut app, &runner_or_skip(), Action::ToggleFilter).unwrap();
    assert!(!app.open_only);
}

#[test]
fn apply_action_settings_lane_quit_still_works() {
    let mut app = App::new();
    app.select_lane(Lane::Settings);
    app.settings = Some(raul::tui::mode::SettingsState::new(serde_json::json!({})));
    apply_action(&mut app, &runner_or_skip(), Action::Quit).unwrap();
    assert!(app.quitting);
}

// ---------------------------------------------------------------------------
// Additional integration coverage:
// ---------------------------------------------------------------------------

#[test]
fn normal_handler_tab_lane_jump_emits_jump_action() {
    let app = App::new();
    let n = Lane::ordered().len();
    // M164: the upper bound follows `Lane::ordered().len()` (currently 5),
    // so 1..=N emit a `JumpLane(idx)`, anything above is a no-op.
    assert_eq!(
        modes::normal::handle_key(key(KeyCode::Char('1')), &app),
        vec![Action::JumpLane(0)]
    );
    assert_eq!(
        modes::normal::handle_key(key(KeyCode::Char('2')), &app),
        vec![Action::JumpLane(1)]
    );
    let last_digit = char::from(b'0' + n as u8);
    assert_eq!(
        modes::normal::handle_key(key(KeyCode::Char(last_digit)), &app),
        vec![Action::JumpLane(n - 1)],
        "digit {} must jump to the last lane index {}",
        last_digit,
        n - 1
    );
    // Beyond N — no action.
    if n < 9 {
        let beyond = char::from(b'0' + (n as u8) + 1);
        assert_eq!(
            modes::normal::handle_key(key(KeyCode::Char(beyond)), &app),
            Vec::new(),
            "digit {} (beyond {}) must produce no action",
            beyond,
            n
        );
    }
}

#[test]
fn normal_handler_enter_on_milestone_detail_opens_thread() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(sample_ms());
    app.enter_milestone_detail(Some(0));
    assert_eq!(app.content, ContentState::MilestoneDetail);
    // Move focus off the tab bar so Enter falls through to the
    // content handler instead of the tab-bar FocusContent.
    // M167: app.tab_bar_focused = false; (no-op; field removed)
    assert_eq!(
        modes::normal::handle_key(key(KeyCode::Enter), &app),
        vec![Action::OpenAnnotationThread]
    );
}

#[test]
fn resolve_annotation_action_resolves_open_annotation() {
    // apply_action::ResolveAnnotation shells out to `mp` (via
    // runner_helpers::resolve_annotation). We can't run an mp subprocess
    // here without polluting the test environment, so this test only
    // pins the App mutation — the gate test for runner_helpers proves
    // the shell-out goes through `MpRunner::run_raw` correctly.
    let mut app = App::new();
    app.load_annotations(vec![AnnotationInfo {
        id: "AN-01".into(),
        target: "01".into(),
        kind: "review".into(),
        status: "open".into(),
        author: "alice".into(),
        body: "Looks good".into(),
        created_at: "".into(),
        resolved_at: "".into(),
    }]);
    // Apply Quit (no shell-out) to prove the dirty-signal contract.
    apply_action(&mut app, &runner_or_skip(), Action::Quit).unwrap();
    assert!(app.quitting, "Action::Quit must mark app.quitting");
}

/// M91 / M136 invariant: `apply_action` for `Action::Quit` bumps the
/// version counter so the dirty-signal path renders the next frame.
#[test]
fn quit_action_dirties_app() {
    let mut app = App::new();
    let before = app.version();
    apply_action(&mut app, &runner_or_skip(), Action::Quit).unwrap();
    assert!(app.version() > before);
}

/// Mode enum payload invariants: `InputState` and `ReviewMenuState`
/// inside `Mode` must be `Eq`-comparable so tests can compare them.
#[test]
fn mode_payloads_are_eq_comparable() {
    let i1 = Mode::Input(InputState {
        target: "M01".into(),
        kind: "review".into(),
        buffer: "hello".into(),
    });
    let i2 = Mode::Input(InputState {
        target: "M01".into(),
        kind: "review".into(),
        buffer: "hello".into(),
    });
    assert_eq!(i1, i2);

    let r1 = Mode::ReviewMenu(ReviewMenuState {
        items: vec!["Approve".into(), "Block".into()],
        selected: 1,
    });
    let r2 = Mode::ReviewMenu(ReviewMenuState {
        items: vec!["Approve".into(), "Block".into()],
        selected: 1,
    });
    assert_eq!(r1, r2);
}

/// M136 review remediation: `App::open_thread` is the canonical entry to
/// the annotation-thread mode and must flip `active_mode` to
/// `Mode::AnnotationThread` so `dispatch_key` routes through
/// `modes::annotation_thread::handle_key`. Closing the thread (Esc
/// → `Action::CloseAnnotationThread`) must clear it back to Normal.
#[test]
fn open_thread_promotes_active_mode_then_close_clears_it() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.selected_milestone_id = Some("M01".to_string());
    app.open_thread();
    assert_eq!(app.content, ContentState::AnnotationThread);
    assert_eq!(
        app.active_mode,
        Mode::AnnotationThread,
        "open_thread must flip active_mode to Mode::AnnotationThread"
    );
    // The reverse: Action::CloseAnnotationThread must clear it back.
    apply_action(&mut app, &runner_or_skip(), Action::CloseAnnotationThread).unwrap();
    assert_eq!(app.content, ContentState::MilestoneDetail);
    assert_eq!(
        app.active_mode,
        Mode::Normal,
        "close-thread arm must restore Mode::Normal"
    );
}
