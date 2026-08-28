//! TUI actions separate pure key interpretation from stateful execution:
//!
//! ```text
//!   keypress   → per-mode handler in tui/modes/
//!              → Vec<Action>           ← pure data, no I/O, no App mutation
//!             apply_action(app, action)  ← single home for App mutation
//!                                         and mp subprocess calls
//! ```
//!
//! Variants name user intent, mode handlers map keys to those variants, and
//! `apply_action` owns mutation and subprocess side effects.

use anyhow::Result;

use crate::mp_runner::MpRunner;

use super::app::{App, CoApprovalAction, ContentState, Lane};
use super::mode::{Mode, SettingsEdit, SettingsFocus};
use super::modes::settings::value_for_key;
use super::runner_helpers::{
    co_approval_approve, create_annotation, create_approval_annotation, execute_review_action,
    load_annotations, load_data_for_lane, load_milestone_detail, load_milestones, load_path_data,
    load_preflight_gate, persist_sort_rebind_choice, reopen_annotation, resolve_annotation,
    set_dependency, ReviewActionOutcome,
};

/// M136: a user-intent mutation. Pure data — variants carry any payload they
/// need (indices, ids, characters) but never reference `App` or `MpRunner`.
///
/// Variants are written in roughly the order they're documented in the M136
/// spec, with mode-only actions added at the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // ---- global -------------------------------------------------------------
    /// User wants to quit the TUI.
    Quit,

    /// Esc outside an Input overlay — closes the top-most overlay (help /
    /// review menu) or, on a content screen, focuses the tab bar / goes
    /// back. The exact behavior depends on the current mode, so this
    /// action is only valid in non-Input modes.
    Esc,

    /// User pressed `r` (or `R`) on the Overview lane — re-fetch the
    /// dashboard. Lane-specific refresh keys (`r` on Path) emit
    /// the same action; `apply_action` dispatches to the right loader
    /// based on the active lane.
    RefreshLane,

    /// Toggle the open-only annotation filter (`f`).
    ToggleFilter,

    /// Toggle the hide-done lane filter (`h`). On every flip we write the
    /// new value through `mp config set ui.hide_done` so the setting
    /// survives across sessions.
    ToggleHideDone,

    // ---- tab bar focus ------------------------------------------------------
    /// Move to the previous lane in `Lane::ordered()` (Left / h).
    PreviousLane,

    // ---- M167: detail-section navigation (MilestoneDetail only) ------------
    /// Jump to the next populated section in the milestone detail view (`]`).
    NextSection,
    /// Jump to the previous populated section (`[`).
    PrevSection,
    /// Jump to the next list item across Steps / ACs / Findings (`n`).
    NextItem,
    /// Jump to the previous list item (`p`).
    PrevItem,

    // ---- M172 S5: sort rebind -----------------------------------------------
    /// Open the sort-rebind inline menu (default keybind `S`).
    /// While the menu is open, ArrowUp / ArrowDown cycle through
    /// available sort keys for the active lane; Enter binds.
    OpenSortRebind,
    /// Cycle to the next sort key in the open menu.
    SortRebindNext,
    /// Cycle to the previous sort key.
    SortRebindPrev,
    /// Confirm the highlighted sort key and persist to config.
    SortRebindConfirm,
    /// Cancel the open menu (Esc).
    SortRebindCancel,

    // ---- M185: lifecycle filter ---------------------------------------------
    /// Open the lifecycle filter modal (default capital `F`).
    OpenLifecycleFilter,
    /// Toggle the highlighted lifecycle in the draft set (Space).
    LifecycleFilterToggle,
    /// Move cursor down in the filter modal.
    LifecycleFilterNext,
    /// Move cursor up in the filter modal.
    LifecycleFilterPrev,
    /// Commit the draft filter set and close the modal.
    LifecycleFilterCommit,
    /// Restore the prior filter and close the modal.
    LifecycleFilterCancel,
    /// Apply the Grooming preset (`g` on Milestones).
    ApplyGroomingPreset,

    // ---- M186: search input --------------------------------------------------
    /// Open the search input (default `/`).
    OpenSearch,
    /// Append a character to the search buffer.
    SearchInputChar(char),
    /// Delete the previous character in the search buffer.
    SearchInputBackspace,
    /// Commit the search term (Enter).
    SearchInputCommit,
    /// Cancel the search input (Esc — restore prior term).
    SearchInputCancel,

    // ---- M186: cycle sort ----------------------------------------------------
    /// Cycle to the next sort key for the active lane (default `o`).
    CycleSortNext,
    /// Move to the next lane in `Lane::ordered()` (Right / l).
    NextLane,
    /// Jump to a specific lane by `Lane::ordered()` index (digit keys 1..=N,
    /// where `N = Lane::ordered().len()`).
    JumpLane(usize),
    /// Leave tab-bar focus and re-focus the content pane.
    FocusContent,

    // ---- list / detail content ---------------------------------------------
    /// Move the cursor up in a list, the annotation list, or the detail
    /// scroll. Behavior depends on the current `ContentState`; `apply_action`
    /// delegates to the App mutators.
    Up,
    /// Move the cursor down (mirror of `Up`).
    Down,
    /// Page up — list uses a 10-row page jump; non-list falls through to `Up`.
    PageUp,
    /// Page down (mirror of `PageUp`).
    PageDown,
    /// Enter — drills into the selected item, opens a thread, or fires the
    /// review-menu selection, depending on `content`.
    Enter,

    // ---- help overlay -------------------------------------------------------
    /// Open the help overlay (`?` from a non-Help mode).
    OpenHelp,
    /// Close the help overlay (`?` or `Esc` while help is up).
    CloseHelp,

    // ---- review menu --------------------------------------------------------
    /// Open the review menu from a milestone detail (`m`).
    OpenReviewMenu,
    /// Close the review menu (`Esc`).
    CloseReviewMenu,
    /// Execute the currently-selected review-menu item (Enter while the
    /// menu is open). The menu state lives inside `Mode::ReviewMenu`;
    /// `apply_action` looks up the selected label.
    ExecuteReviewAction,

    // ---- annotation thread + co-approval -----------------------------------
    /// Open the annotation thread view from a milestone detail (Enter on
    /// milestone detail).
    OpenAnnotationThread,
    /// Close the thread view and return to milestone detail (Esc / go_back).
    CloseAnnotationThread,
    /// Resolve the currently-selected annotation (`r` in a thread).
    ResolveAnnotation,
    /// Reopen the currently-selected annotation (`R` in a thread).
    ReopenAnnotation,
    /// Start creating a new annotation — opens the input overlay.
    CreateAnnotation,
    /// Enter co-approval view for the selected approval-request annotation
    /// (Enter on a thread row whose kind is approval-request).
    EnterCoApproval,
    /// Confirm the co-approval flow (Enter on CoApproval content) and run
    /// the chosen `CoApprovalAction` (approve / reject).
    ConfirmCoApproval,
    /// Set the co-approval action without confirming (`p` for approve,
    /// `R` for reject).
    SetCoApprovalAction(CoApprovalAction),

    // ---- milestone-detail approval controls ---------------------------------
    /// Run the approval flow's primary action. On milestone detail, this
    /// is `p` / approve: if an approval-request annotation blocks the
    /// approval, resolve it; otherwise create a new approval-request.
    ToggleApproval,

    // ---- input overlay ------------------------------------------------------
    /// Submit the input buffer (Enter inside an input prompt). Closes the
    /// input overlay and creates the pending annotation.
    SubmitInput,
    /// Cancel the input (Esc inside an input prompt). Drops the buffer.
    CancelInput,
    /// Push a character into the input buffer.
    PushInputChar(char),
    /// Pop the last character from the input buffer (Backspace).
    PopInputChar,

    // ---- settings lane (M169) -----------------------------------------------
    /// Save all staged Settings edits (`s` on the Settings lane).
    SettingsSave,

    // ---- watch lane (M179) -----------------------------------------------
    /// Toggle the picker cursor's selection into the ordered queue
    /// (`Space` / `Enter` on the Watch lane).
    WatchToggleSelect,
    /// Run the dry-run preflight on the current queue (`p`).
    WatchPreflight,
    /// Start the validated queue through the M178 detach surface
    /// (`s` on the Watch lane). Refused if preflight failed or a
    /// live run is already attached.
    WatchStart,
    /// Stop the live watch run through the M178 control surface
    /// (`x` on the Watch lane).
    WatchStop,
    /// Force-refresh the latest status / output snapshot
    /// (`r` on the Watch lane).
    WatchRefresh,
    /// Move the picker cursor up/down on the Watch lane
    /// (`j` / `k`).
    WatchMovePicker { delta: i64 },
    /// Move the queue cursor up/down on the Watch lane
    /// (`J` / `K`).
    WatchMoveQueue { delta: i64 },
    /// Clear all queue selections on the Watch lane (`c`).
    WatchClearQueue,
}

/// Apply an `Action` to `app`. This is the single place that mutates `App`
/// in response to a keypress and the single place that shells out to `mp`.
///
/// The function deliberately drops an `Action` whose semantics don't apply
/// to the current mode (e.g. `CloseHelp` while the user is in
/// `Mode::Normal`). Returning `Ok(())` for a dropped action is the spec — it
/// means "the action would do nothing in this state" rather than "the action
/// is invalid and must error".
///
/// ## Errors
///
/// Any `mp` subprocess failure propagates with `?`. The event loop in
/// `run_loop` reports the error and bails out of the current event; in
/// practice the per-event error is rendered as a `flash_message` and the
/// loop continues — see `run_loop`'s dispatch error handling.
pub fn apply_action(app: &mut App, runner: &MpRunner, action: Action) -> Result<()> {
    let watch_before = app.watch.clone();
    let version_before = app.version();
    match action {
        // ---- global ---------------------------------------------------------
        Action::Quit => {
            app.quit();
        }
        Action::Esc => {
            // Pre-M136, this was `handle_esc` — overloading it inside
            // apply_action keeps the contract "Esc outside Input closes the
            // top-most overlay / goes back" (M167: no tab-bar focus toggle).
            apply_esc(app, runner)?;
        }
        Action::RefreshLane => {
            // The dispatch table in `modes/normal` decides which keys emit
            // RefreshLane; here we map to the correct loader based on the
            // current lane.
            match app.active_lane {
                Lane::Overview => super::runner_helpers::load_dashboard(runner, app)?,
                Lane::Path => load_path_data(runner, app)?,
                _ => load_data_for_lane(runner, app)?,
            }
        }
        Action::ToggleFilter => {
            app.toggle_filter();
        }
        Action::ToggleHideDone => {
            app.toggle_hide_done();
            let val = if app.hide_done { "true" } else { "false" };
            let _ = runner.run_raw("config", &["set", "ui.hide_done", val]);
        }
        // M182 S4: sort rebind — persistence flows through `mp config
        // set sort.<lane> <sortkey>` on confirm. The open/cycle/cancel
        // arms stay pure-app state (no runner call); confirm + cancel
        // are the only two that touch mp. Errors from the `mp config
        // set` round-trip are surfaced as a footer flash via
        // `App::set_action_error` so the user sees a real failure
        // instead of a silent success.
        Action::OpenSortRebind => app.open_sort_rebind(),
        Action::SortRebindNext => app.cycle_sort_rebind_next(),
        Action::SortRebindPrev => app.cycle_sort_rebind_prev(),
        Action::SortRebindConfirm => {
            persist_sort_rebind_choice(runner, app)?;
        }
        Action::SortRebindCancel => app.cancel_sort_rebind(),

        // M185: lifecycle filter modal + grooming preset.
        Action::OpenLifecycleFilter => {
            if app.active_lane == Lane::Milestones && app.content == ContentState::List {
                app.open_lifecycle_filter();
            } else {
                // Surface the lane gate instead of a silent no-op.
                app.set_flash_message("Lifecycle filter is only available on the Milestones list.");
            }
        }
        Action::LifecycleFilterToggle => app.lifecycle_filter_toggle(),
        Action::LifecycleFilterNext => app.lifecycle_filter_next(),
        Action::LifecycleFilterPrev => app.lifecycle_filter_prev(),
        Action::LifecycleFilterCommit => app.lifecycle_filter_commit(),
        Action::LifecycleFilterCancel => app.lifecycle_filter_cancel(),
        Action::ApplyGroomingPreset => {
            if app.active_lane == Lane::Milestones {
                app.apply_grooming_preset();
            } else {
                app.set_flash_message("Grooming preset is only available on Milestones.");
            }
        }

        // M186: search input + cycle sort.
        Action::OpenSearch => {
            if matches!(
                app.active_lane,
                Lane::Milestones | Lane::Backlog | Lane::Ideas
            ) && app.content == ContentState::List
            {
                app.open_search();
            }
        }
        Action::SearchInputChar(c) => app.search_push_char(c),
        Action::SearchInputBackspace => app.search_backspace(),
        Action::SearchInputCommit => app.search_commit(),
        Action::SearchInputCancel => app.search_cancel(),
        Action::CycleSortNext => {
            if matches!(
                app.active_lane,
                Lane::Milestones | Lane::Backlog | Lane::Ideas
            ) {
                app.cycle_sort_next();
            }
        }

        // ---- tab bar focus --------------------------------------------------
        Action::PreviousLane => {
            app.tab_move_up();
            load_data_for_lane(runner, app)?;
        }
        Action::NextLane => {
            app.tab_move_down();
            load_data_for_lane(runner, app)?;
        }
        Action::JumpLane(idx) => {
            // M198: digit keys 1..=N follow the *visible* lane
            // list (Watch omitted when `ui.show_watch_tab` is
            // `false`) so the on-screen tab number matches the
            // key. A stale idx (e.g. the operator toggled the
            // flag off between digit press and dispatch) is
            // silently ignored — the S4 fallback handles the
            // case where Watch *was* the active lane.
            let lanes = Lane::ordered_visible(app.show_watch_tab);
            if let Some(lane) = lanes.get(idx) {
                app.select_lane(*lane);
                load_data_for_lane(runner, app)?;
            }
        }
        Action::FocusContent => {
            load_data_for_lane(runner, app)?;
        }

        // ---- list / detail content -----------------------------------------
        Action::NextSection | Action::PrevSection | Action::NextItem | Action::PrevItem => {
            // M167: detail-section navigation. The actual row math lives in
            // the milestone-detail renderer (which populates a row map during
            // `compute_view`); the runner consumes the action and asks the
            // app to advance. For now, we record a touch so the version
            // counter bumps and the renderer re-runs; WP3 (full document
            // rendering) installs the row-map data and replaces this with
            // the real jump.
            apply_detail_section_nav(app, action);
        }
        Action::Up => {
            app.move_up();
        }
        Action::Down => {
            app.move_down();
        }
        Action::PageUp => {
            app.move_page_up();
        }
        Action::PageDown => {
            app.move_page_down();
        }
        Action::Enter => {
            if app.active_lane == Lane::Settings {
                apply_settings_enter(app, runner)?;
            } else {
                apply_enter(app, runner)?;
            }
        }

        // ---- help overlay ---------------------------------------------------
        Action::OpenHelp => {
            if !matches!(app.active_mode, Mode::Help) {
                app.active_mode = Mode::Help;
                app.touch();
            }
        }
        Action::CloseHelp => {
            if app.active_mode == Mode::Help {
                app.active_mode = Mode::Normal;
                app.touch();
            }
        }

        // ---- review menu ----------------------------------------------------
        Action::OpenReviewMenu => {
            if app.content == super::app::ContentState::MilestoneDetail {
                if let Some(ms_id) = app.selected_milestone_id.clone() {
                    let gate = load_preflight_gate(runner, &ms_id);
                    if let Some(error) = gate.error.clone() {
                        app.set_action_error(
                            format!("Cannot check approval gate for M{ms_id}. Approve remains disabled."),
                            error,
                        );
                    } else {
                        app.clear_flash_message();
                    }
                    app.preflight_gate = Some(gate);
                }
                app.open_review_menu();
            }
        }
        Action::CloseReviewMenu => {
            if let Mode::ReviewMenu(_) = app.active_mode {
                app.active_mode = Mode::Normal;
                app.touch();
            }
        }
        Action::ExecuteReviewAction => {
            apply_review_menu_enter(app, runner)?;
        }

        // ---- annotation thread + co-approval ------------------------------
        Action::OpenAnnotationThread => {
            if let Some(ref ms_id) = app.selected_milestone_id.clone() {
                app.open_thread();
                load_annotations(runner, app, ms_id)?;
            }
        }
        Action::CloseAnnotationThread => {
            app.go_back();
            // M136 review remediation: `App::open_thread` now also sets
            // `active_mode = Mode::AnnotationThread` (so the dispatcher
            // routes through `modes::annotation_thread::handle_key`),
            // which means closing has to clear it. The `else` arm is
            // defensive: a future caller that bypasses `open_thread`
            // should still leave Normal mode alone.
            if matches!(app.active_mode, Mode::AnnotationThread) {
                app.active_mode = Mode::Normal;
                app.touch();
            }
        }
        Action::ResolveAnnotation => {
            if let Some(annotation) = app.selected_annotation().cloned() {
                if annotation.status == "open" || annotation.status == "addressed" {
                    resolve_annotation(runner, app, &annotation.id)?;
                    if let Some(ref target) = app.selected_milestone_id.clone() {
                        load_annotations(runner, app, target)?;
                    }
                }
            }
        }
        Action::ReopenAnnotation => {
            if let Some(annotation) = app.selected_annotation().cloned() {
                if annotation.status == "resolved" {
                    reopen_annotation(runner, app, &annotation.id)?;
                    if let Some(ref target) = app.selected_milestone_id.clone() {
                        load_annotations(runner, app, target)?;
                    }
                }
            }
        }
        Action::CreateAnnotation => {
            if let Some(ref ms_id) = app.selected_milestone_id.clone() {
                app.start_input(ms_id.to_string(), "review-request".to_string());
            }
        }
        Action::EnterCoApproval => {
            if let Some(ann) = app.selected_annotation().cloned() {
                if ann.kind == "approval-request"
                    && (ann.status == "open" || ann.status == "addressed")
                {
                    let ms_id = ann.target.clone();
                    app.enter_co_approval(ann, ms_id);
                }
            }
        }
        Action::ConfirmCoApproval => {
            let (ann_id, ms_id, choice) = match app.begin_co_approval_execution() {
                Ok(values) => values,
                Err(message) => {
                    app.set_action_error(message, message);
                    return Ok(());
                }
            };

            let operation = (|| -> Result<()> {
                match choice {
                    CoApprovalAction::Approve => {
                        let already_resolved = app
                            .co_approval_annotation
                            .as_ref()
                            .is_some_and(|annotation| annotation.status == "resolved");
                        if !already_resolved {
                            resolve_annotation(runner, app, &ann_id)?;
                            if let Some(annotation) = app.co_approval_annotation.as_mut() {
                                annotation.status = "resolved".to_string();
                            }
                        }
                        co_approval_approve(runner, app, &ms_id)?;
                    }
                    CoApprovalAction::Reject => {
                        // EnterCoApproval only admits open|addressed, but after a
                        // partial Approve (resolve ok, approve fail) the in-memory
                        // annotation may already be resolved — Reject then reopens.
                        // For open|addressed, Reject is a real decline: resolve the
                        // approval-request without calling `milestone approve`.
                        // Never call `annotation reopen` on open/addressed; real mp
                        // rejects both (already open / cannot reopen from addressed).
                        let status = app
                            .co_approval_annotation
                            .as_ref()
                            .map(|annotation| annotation.status.as_str())
                            .unwrap_or("");
                        match status {
                            "resolved" => {
                                reopen_annotation(runner, app, &ann_id)?;
                                if let Some(annotation) = app.co_approval_annotation.as_mut() {
                                    annotation.status = "open".to_string();
                                }
                            }
                            "open" | "addressed" => {
                                resolve_annotation(runner, app, &ann_id)?;
                                if let Some(annotation) = app.co_approval_annotation.as_mut() {
                                    annotation.status = "resolved".to_string();
                                }
                            }
                            other => {
                                anyhow::bail!(
                                    "cannot reject approval-request from status: {other} \
                                     (expected open, addressed, or resolved)"
                                );
                            }
                        }
                    }
                }
                Ok(())
            })();

            // Refresh is deliberately best-effort: a stale view must not hide
            // the command that actually failed, and the modal remains retryable.
            let _ = load_annotations(runner, app, &ms_id);
            let _ = super::runner_helpers::check_approval_status(runner, app, &ms_id);
            match operation {
                Ok(()) => {
                    app.clear_flash_message();
                    app.finish_co_approval();
                }
                Err(error) => app.fail_co_approval(error.to_string()),
            }
        }
        Action::SetCoApprovalAction(choice) => {
            app.set_co_approval_action(choice);
        }

        // ---- milestone-detail approval controls ----------------------------
        Action::ToggleApproval => {
            if let Some(ref ms_id) = app.selected_milestone_id.clone() {
                if app.approval_blocked {
                    if let Some(ref ann_id) = app.approval_annotation_id.clone() {
                        resolve_annotation(runner, app, ann_id)?;
                        load_annotations(runner, app, ms_id)?;
                        super::runner_helpers::check_approval_status(runner, app, ms_id)?;
                    }
                } else {
                    create_approval_annotation(runner, app, ms_id)?;
                    load_annotations(runner, app, ms_id)?;
                    super::runner_helpers::check_approval_status(runner, app, ms_id)?;
                }
            }
        }

        // ---- input overlay --------------------------------------------------
        Action::SubmitInput => {
            if let Some((target, kind, body)) = app.confirm_input() {
                if kind == "set-dependency" {
                    // M172 S6: shell out to `mp milestone update <id>
                    // --json @-` with the new dependency appended. The
                    // user types the dependency milestone ID into
                    // the overlay; the source (target) is the
                    // currently-selected milestone.
                    set_dependency(runner, app, &target, &body)?;
                } else {
                    create_annotation(runner, app, &target, &kind, &body)?;
                    let t = target.clone();
                    load_annotations(runner, app, &t)?;
                }
            }
        }
        Action::CancelInput => {
            app.cancel_input();
        }
        Action::PushInputChar(c) => {
            if let Some(state) = app.settings.as_mut() {
                if let Some(edit) = state.edit.as_mut() {
                    edit.cursor = char_insert(&mut edit.buffer, edit.cursor, c);
                    app.touch();
                    return Ok(());
                }
            }
            app.push_input_char(c);
        }
        Action::PopInputChar => {
            if let Some(state) = app.settings.as_mut() {
                if let Some(edit) = state.edit.as_mut() {
                    edit.cursor = char_backspace(&mut edit.buffer, edit.cursor);
                    app.touch();
                    return Ok(());
                }
            }
            app.pop_input_char();
        }

        // ---- settings lane (M169) -------------------------------------------
        Action::SettingsSave => {
            apply_settings_save(app, runner)?;
        }

        // ---- watch lane --------------------------------------------------
        Action::WatchToggleSelect => {
            let id_opt = app.watch.picker_candidate().map(|c| c.id.clone());
            if let Some(id) = id_opt {
                app.watch.toggle_select(&id);
            }
        }
        Action::WatchPreflight => {
            crate::tui::watch::run_preflight(runner, app)?;
        }
        Action::WatchStart => {
            crate::tui::watch::start_watch(runner, app)?;
        }
        Action::WatchStop => {
            crate::tui::watch::stop_watch(runner, app, 30)?;
        }
        Action::WatchRefresh => {
            // Same `mem::take` pattern as the production on_idle
            // closure — `poll_watch_state` needs `&mut app` and
            // `&mut app.watch_poller`, which overlap.
            let mut poller = std::mem::take(&mut app.watch_poller);
            let result = crate::tui::watch::poll_watch_state(runner, app, &mut poller);
            app.watch_poller = poller;
            result?;
        }
        Action::WatchMovePicker { delta } => {
            app.watch.move_picker(delta);
        }
        Action::WatchMoveQueue { delta } => {
            app.watch.move_queue(delta);
        }
        Action::WatchClearQueue => {
            app.watch.clear_selection();
        }
    }

    if app.watch != watch_before && app.version() == version_before {
        app.touch();
    }
    Ok(())
}

fn apply_settings_enter(app: &mut App, runner: &MpRunner) -> Result<()> {
    use crate::tui::modes::settings::{flat_key, SETTINGS_KEYS};

    let Some(state) = app.settings.as_mut() else {
        return Ok(());
    };

    if matches!(state.focus, SettingsFocus::Editing) {
        return apply_settings_commit_edit(app, runner);
    }

    let selected_idx = state.selected_idx;
    let config = state.config.clone();
    let Some((_section, key)) = flat_key(selected_idx) else {
        return Ok(());
    };
    // **M169-rev (MED fix):** prefer a previously staged value over the
    // on-disk value when re-opening the edit on the same key. The
    // post-staging renderer previews the raw buffer string, so the
    // edit buffer must agree — otherwise the third Enter after a
    // commit silently reverts the user's buffer to the on-disk value.
    let buffer = if let Some(staged) = state.staged_edits.get(key) {
        staged.clone()
    } else {
        match runner.run::<serde_json::Value>("config", &["get", key]) {
            Ok(v) => match v.get("value") {
                Some(serde_json::Value::Null) | None => String::new(),
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Bool(b)) => b.to_string(),
                Some(serde_json::Value::Number(n)) => n.to_string(),
                Some(other) => other.to_string(),
            },
            Err(_) => value_for_key(&config, key),
        }
    };
    let Some(state) = app.settings.as_mut() else {
        return Ok(());
    };
    state.edit = Some(SettingsEdit {
        key: key.to_string(),
        cursor: buffer.chars().count(),
        buffer,
        errors: Vec::new(),
    });
    state.focus = SettingsFocus::Editing;
    state.selected_idx = state
        .selected_idx
        .min(SETTINGS_KEYS.len().saturating_sub(1));
    app.touch();
    Ok(())
}

fn apply_settings_commit_edit(app: &mut App, runner: &MpRunner) -> Result<()> {
    let (key, value) = {
        let Some(state) = app.settings.as_ref() else {
            return Ok(());
        };
        let Some(edit) = state.edit.as_ref() else {
            return Ok(());
        };
        (edit.key.clone(), edit.buffer.clone())
    };

    let dry_stdout = runner.run_raw_allow_failure("config", &["set", &key, &value, "--dry-run"])?;
    let dry: serde_json::Value = serde_json::from_slice(&dry_stdout).unwrap_or_else(|_| {
        serde_json::json!({
            "ok": false,
            "errors": [{ "field": key, "message": "invalid dry-run response" }]
        })
    });
    let ok = dry.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if !ok {
        let errors = settings_dry_run_errors(&dry, &key);
        if let Some(state) = app.settings.as_mut() {
            if let Some(edit) = state.edit.as_mut() {
                edit.errors = errors;
            }
            app.touch();
        }
        return Ok(());
    }

    if let Some(state) = app.settings.as_mut() {
        state.staged_edits.insert(key.clone(), value.clone());
        // **M169-rev (MED fix):** do NOT mutate `state.config` from the
        // staging path. The previous `set_config_value` shadowed mp's
        // type coercion (`parse_bool` accepts `true | 1 | yes`,
        // `parse_icons` accepts `none | ascii | unicode`, etc.) with a
        // narrower Rust-side parser that stored raw strings like "yes"
        // or "01" — the in-memory preview then disagreed with the
        // mp-coerced on-disk value. The renderer already prefers
        // `staged_edits` over `state.config` for staged keys, so leaving
        // `state.config` alone is safe. mp owns the coercion; we only
        // own the staging buffer until `apply_settings_save` flushes
        // through `mp config set`, at which point `reload_settings_after_save`
        // re-pulls the canonical coerced config.
        state.edit = None;
        state.focus = SettingsFocus::Fields;
        app.touch();
    }
    Ok(())
}

fn apply_settings_save(app: &mut App, runner: &MpRunner) -> Result<()> {
    // **M169-rev (LOW fix):** `BTreeMap` iterates key-sorted so the
    // on-disk write order is deterministic across runs.
    let staged: Vec<(String, String)> = app
        .settings
        .as_ref()
        .map(|s| {
            s.staged_edits
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    if staged.is_empty() {
        return Ok(());
    }

    // AC-05: dry-run all staged edits before any commit. Any dry-run
    // failure aborts the whole batch and surfaces the API error in the
    // footer flash — `staged_edits` is preserved so the user can fix
    // the offending key and retry.
    for (key, value) in &staged {
        let dry_stdout =
            runner.run_raw_allow_failure("config", &["set", key, value, "--dry-run"])?;
        let dry: serde_json::Value = serde_json::from_slice(&dry_stdout).unwrap_or_else(|_| {
            serde_json::json!({
                "ok": false,
                "errors": [{ "field": key, "message": "invalid dry-run response" }]
            })
        });
        let ok = dry.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok {
            let errors = settings_dry_run_errors(&dry, key);
            let msg = errors.join("; ");
            app.set_action_error(format!("Settings save failed: {msg}"), msg);
            return Ok(());
        }
    }

    // Commit each staged edit. **M169-rev (MED fix):** track which keys
    // actually landed so a commit-time failure surfaces the partial
    // state. Previously, k1 would silently land on disk while the
    // footer reported the k2 failure and `staged_edits` was left
    // holding both — the user had no signal that k1 succeeded.
    let mut committed: Vec<String> = Vec::new();
    for (key, value) in &staged {
        if let Err(e) = runner.run_raw("config", &["set", key, value]) {
            let msg = e.to_string();
            let suffix = if committed.is_empty() {
                String::new()
            } else {
                format!(
                    " ({} already saved: {})",
                    committed.len(),
                    committed.join(", ")
                )
            };
            app.set_action_error(
                format!("Settings save failed: {msg}{suffix}"),
                format!("Failed to save {key} = {value}: {msg}{suffix}"),
            );
            // Reload (preserving the failed key in `staged_edits` so
            // the user can retry it) and drop committed keys so a
            // follow-up `s` only re-tries the failed one.
            reload_settings_after_save_keeping(app, runner, std::slice::from_ref(key))?;
            return Ok(());
        }
        committed.push(key.clone());
    }

    reload_settings_after_save(app, runner)?;
    app.set_flash_message(format!("Saved {} setting(s)", staged.len()));
    Ok(())
}

fn settings_dry_run_errors(dry: &serde_json::Value, key: &str) -> Vec<String> {
    dry_run_errors_for(dry, key)
}

/// Public re-export of the dry-run error formatter so the M169-rev
/// regression tests can pin the error shape.
pub fn dry_run_errors_for(dry: &serde_json::Value, key: &str) -> Vec<String> {
    dry.get("errors")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .map(|err| {
                    let field = err.get("field").and_then(|f| f.as_str()).unwrap_or("");
                    let message = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("validation failed");
                    if field.is_empty() {
                        message.to_string()
                    } else {
                        format!("{field}: {message}")
                    }
                })
                .collect()
        })
        .unwrap_or_else(|| vec![format!("{key}: validation failed")])
}

/// **M169-rev:** test-only public surface for the partial-reload
/// helper (`apply_reload_keeping`) and the dry-run error formatter
/// (`dry_run_errors_for`). `#[doc(hidden)]` keeps the module out of
/// `cargo doc` so it's not mistaken for an external API — the
/// integration tests under `crates/raul/tests/m169_rev.rs` import
/// from `raul::tui::action::test_helpers` directly.
#[doc(hidden)]
pub mod test_helpers {
    pub use super::dry_run_errors_for;
    pub use super::reload_settings_after_save_keeping as apply_reload_keeping;
}

fn reload_settings_after_save(app: &mut App, runner: &MpRunner) -> Result<()> {
    reload_settings_after_save_keeping(app, runner, &[])
}

/// **M169-rev (MED fix):** reload config after a save, but keep the
/// supplied keys in `staged_edits` (typically the still-failing keys
/// the user needs to retry). All other staged keys — i.e. ones that
/// landed on disk in this save — are dropped.
pub fn reload_settings_after_save_keeping(
    app: &mut App,
    runner: &MpRunner,
    keep_staged: &[String],
) -> Result<()> {
    if let Ok(data) = runner.run::<serde_json::Value>("config", &["show"]) {
        let config = data
            .get("config")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        app.keybinds = crate::tui::keybinds::Keybinds::load_from_config(&data);
        if let Some(state) = app.settings.as_mut() {
            state.config = config;
            state
                .staged_edits
                .retain(|k, _| keep_staged.iter().any(|kept| kept == k));
            state.edit = None;
            state.focus = SettingsFocus::Fields;
        }
        if let Some(hd) = data
            .pointer("/config/ui/hide_done")
            .and_then(|v| v.as_bool())
        {
            app.hide_done = hd;
        }
        if let Some(theme) = data.pointer("/config/ui/theme").and_then(|v| v.as_str()) {
            if let Some(p) = crate::theme::Palette::by_name(theme) {
                app.palette = p;
            }
        }
        if let Some(icons) = data.pointer("/config/ui/icons").and_then(|v| v.as_str()) {
            let mode = match icons {
                "none" => crate::config::IconMode::None,
                "ascii" => crate::config::IconMode::Ascii,
                _ => crate::config::IconMode::Unicode,
            };
            crate::config::set_icons(mode);
        }
        app.touch();
    }
    Ok(())
}

fn apply_settings_cancel_edit(app: &mut App) {
    if let Some(state) = app.settings.as_mut() {
        state.edit = None;
        state.focus = SettingsFocus::Fields;
        app.touch();
    }
}

/// M167: detail-section navigation. The milestone-detail renderer (WP3)
/// populates `app.detail_section_rows` with the absolute row offset of
/// each populated section's first item; this function finds the next /
/// previous / item-relative target and writes the new `detail_scroll`.
/// The keypress is always consumed (recorded via `touch()`) so the help
/// overlay and footers don't claim it.
fn apply_detail_section_nav(app: &mut App, action: Action) {
    let rows = app.detail_section_rows.take();
    if rows.is_empty() {
        // No section map yet (the renderer hasn't produced one for this
        // detail render). Still consume the action so the help / footer
        // doesn't claim it.
        app.touch();
        return;
    }
    let current = app.detail_scroll;
    let new_scroll = match action {
        Action::NextSection => rows
            .iter()
            .find(|&&row| row > current)
            .copied()
            .unwrap_or(current),
        Action::PrevSection => rows
            .iter()
            .rev()
            .find(|&&row| row < current)
            .copied()
            .unwrap_or(current),
        Action::NextItem => rows
            .iter()
            .find(|&&row| row > current.saturating_add(1))
            .copied()
            .unwrap_or(current),
        Action::PrevItem => {
            // Walk backwards one item at a time: pick the row just below
            // the current position. Since `rows` only carries section
            // starts, intra-section stepping falls back to the section
            // boundary one above the cursor — fine for v1; per-item
            // mapping comes when WP3 stores per-item rows.
            rows.iter()
                .rev()
                .find(|&&row| row < current)
                .copied()
                .unwrap_or(current)
        }
        _ => current,
    };
    let max = app.detail_max_scroll.get() as usize;
    app.detail_scroll = new_scroll.min(max as u16);
    app.touch();
}

// M140 ext-review F-08: cursor-aware buffer mutations. `cursor` is a
// char index (not a byte offset) so multi-byte UTF-8 is handled
// correctly. Both helpers clamp the returned index into [0, len].
fn char_insert(buffer: &mut String, cursor: usize, c: char) -> usize {
    let len = buffer.chars().count();
    let idx = cursor.min(len);
    let mut chars: Vec<char> = buffer.chars().collect();
    chars.insert(idx, c);
    *buffer = chars.into_iter().collect();
    idx + 1
}

fn char_backspace(buffer: &mut String, cursor: usize) -> usize {
    let len = buffer.chars().count();
    let idx = cursor.min(len);
    if idx == 0 {
        return 0;
    }
    let mut chars: Vec<char> = buffer.chars().collect();
    chars.remove(idx - 1);
    *buffer = chars.into_iter().collect();
    idx - 1
}

/// Esc outside an Input overlay. The behavior depends on the current mode:
///
///   * `Help` → close help, return to Normal.
///   * `ReviewMenu` → close the menu, return to Normal.
///   * Normal/List content → focus the tab bar (with redraw).
///   * Normal/Detail/Thread content → `go_back()`, then focus the tab bar.
///
/// `go_back()` is the previous-implementation's escape hatch for
/// MilestoneDetail, BacklogDetail, AnnotationThread, and CoApproval. We
/// mirror it exactly so the per-mode Esc behavior is unchanged.
///
/// `pub` so unit tests outside `action.rs` (notably the M134 Esc-on-List
/// dirty-signal contract tests in `runner.rs`) can drive Esc without
/// constructing an `MpRunner` shell-out path.
pub fn apply_esc(app: &mut App, _runner: &MpRunner) -> Result<()> {
    // M169: cancel an active Settings field edit.
    if app.active_lane == Lane::Settings {
        if app
            .settings
            .as_ref()
            .is_some_and(|s| matches!(s.focus, SettingsFocus::Editing))
        {
            apply_settings_cancel_edit(app);
            return Ok(());
        }
        // Esc on the Settings lane (no active edit) is a no-op.
        return Ok(());
    }
    match app.active_mode {
        Mode::Help => {
            app.active_mode = Mode::Normal;
            app.touch();
        }
        Mode::ReviewMenu(_) => {
            app.active_mode = Mode::Normal;
            app.touch();
        }
        Mode::Normal => match app.content {
            // M167: Esc on a top-level List is a no-op (the old behavior
            // focused the tab bar, but the tab bar / content focus split is
            // gone). Esc on a drilled-in detail still pops back via
            // `go_back()`.
            super::app::ContentState::List => {}
            _ => {
                app.go_back();
                app.touch();
            }
        },
        // Input / AnnotationThread — Esc from the dispatcher only
        // emits this when the per-mode handler has not already absorbed it
        // (Mode::Input absorbs Esc into CancelInput; Mode::AnnotationThread
        // absorbs Esc into CloseAnnotationThread). Reaching here means
        // a caller emitted Action::Esc in the wrong mode — a no-op is
        // the safe behavior.
        // (M140's `Mode::Settings` was removed in M169 — the Settings
        // lane is handled by the explicit `if app.active_lane ==
        // Lane::Settings` branch above this match.)
        Mode::Input(_) | Mode::AnnotationThread => {}
        // M185: Esc cancels the filter modal and restores prior state.
        Mode::LifecycleFilter(_) => {
            app.lifecycle_filter_cancel();
        }
        // M186: Esc cancels the search input and restores prior term.
        Mode::SearchInput(_) => {
            app.search_cancel();
        }
    }
    Ok(())
}

/// Enter-key behavior on the Normal mode. Pre-M136 this was a giant
/// `(content, event)` match in `handle_event`; the same logic now lives here
/// as a flat chain of `if`s so the per-mode handler can remain a 1-2 line
/// key → action mapping.
///
/// The branches mirror the pre-M136 code 1:1 except where the data model
/// changed (`AnnotationInfo::target` instead of `selected_milestone_id`).
///
/// Note: Enter on `MilestoneDetail` and Enter on `AnnotationThread` route
/// through their dedicated actions (`OpenAnnotationThread`,
/// `EnterCoApproval`) rather than re-running the same body twice; the
/// dispatcher would otherwise need to special-case content state at every
/// Enter call site.
fn apply_enter(app: &mut App, runner: &MpRunner) -> Result<()> {
    use super::app::ContentState;
    match app.content {
        ContentState::List => match app.active_lane {
            Lane::Milestones => {
                let ms_id = app
                    .visible_milestones()
                    .get(app.selected_index)
                    .map(|m| m.id.clone());
                if let Some(ms_id) = ms_id {
                    app.enter_milestone_detail(None);
                    load_milestone_detail(runner, app, &ms_id)?;
                }
            }
            Lane::Backlog | Lane::Ideas => {
                // M184: Backlog (TW-/BF-/BL-*) and Ideas (ID-*) share
                // the backlog table shape; Enter opens backlog detail.
                if let Some(b) = app.visible_backlog().get(app.selected_index) {
                    let backlog_id = b.id.clone();
                    app.selected_backlog_id = Some(backlog_id.clone());
                    app.detail_scroll = 0;
                    app.content = ContentState::BacklogDetail;
                    super::runner_helpers::load_backlog_detail(runner, app, &backlog_id)?;
                }
            }
            Lane::Path => {
                let next_action = app.dashboard.next_action.clone();
                if !next_action.is_empty() {
                    let ms_id = next_action
                        .trim_start_matches('M')
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    app.select_lane(Lane::Milestones);
                    load_milestones(runner, app)?;
                    let found = app.milestones.iter().any(|m| m.id == ms_id);
                    if found {
                        app.enter_milestone_detail_by_id(&ms_id);
                        load_milestone_detail(runner, app, &ms_id)?;
                    } else {
                        app.set_flash_message(next_action);
                    }
                }
            }
            Lane::Overview => {
                if let Some(item) = app.visible_inbox().get(app.selected_index).cloned() {
                    super::runner_helpers::navigate_from_inbox_item(app, runner, &item)?;
                }
            }
            Lane::Watch => {
                // M179: the Watch lane's Enter semantics are
                // selector-driven (toggle a milestone's selection
                // in the picker) — see `tui::watch` (S3). The
                // dispatch here is a no-op fallback; S3 will
                // re-route via a dedicated `Action::WatchToggleSelect`.
            }
            Lane::Settings => {}
        },
        ContentState::MilestoneDetail => {
            // Pre-M136: Enter on MilestoneDetail opened the annotation
            // thread. The same logic now lives in
            // `Action::OpenAnnotationThread`'s arm — re-dispatch via the
            // action so the open-thread + load-annotations code stays in one
            // place.
            return apply_action(app, runner, Action::OpenAnnotationThread);
        }
        ContentState::BacklogDetail => { /* no-op */ }
        ContentState::AnnotationThread => {
            // Pre-M136: Enter on an approval-request annotation row entered
            // co-approval. Same as `Action::EnterCoApproval`.
            return apply_action(app, runner, Action::EnterCoApproval);
        }
        ContentState::CoApproval => {
            // Delegate to the canonical confirm arm so the confirm flow
            // lives in exactly one place (F-01: previously this inlined a
            // ~17-line copy of `Action::ConfirmCoApproval`, which drifted).
            return apply_action(app, runner, Action::ConfirmCoApproval);
        }
    }
    Ok(())
}

/// Review-menu Enter: snapshot the selected item, close the menu, dispatch
/// the action. Mirrors the pre-M136 `_ => {}` arm of `handle_event` for the
/// `Event::Enter` branch under `show_review_menu`.
fn apply_review_menu_enter(app: &mut App, runner: &MpRunner) -> Result<()> {
    let (action_label, ms_id) = match &app.active_mode {
        Mode::ReviewMenu(menu) => (
            menu.items.get(menu.selected).cloned(),
            app.selected_milestone_id.clone(),
        ),
        _ => (None, None),
    };
    // Close the menu regardless — pre-M136 behavior was to always close on
    // Enter.
    if let Mode::ReviewMenu(_) = app.active_mode {
        app.active_mode = Mode::Normal;
        app.touch();
    }
    if let (Some(action), Some(ref ms_id)) = (action_label, ms_id) {
        match execute_review_action(runner, app, ms_id, &action) {
            ReviewActionOutcome::Ok => {
                app.clear_flash_message();
                if let Err(e) = load_milestone_detail(runner, app, ms_id) {
                    let msg = e.to_string();
                    app.set_action_error(msg.clone(), msg);
                }
            }
            ReviewActionOutcome::M121GateError {
                ac_count,
                ms_id,
                full,
            } => {
                let focused = super::runner_helpers::format_m121_flash_message(&ms_id, ac_count);
                let details = if full.is_empty() {
                    focused.clone()
                } else {
                    full
                };
                app.set_action_error(focused, details);
            }
            ReviewActionOutcome::OtherError(msg) => {
                app.set_action_error(msg.clone(), msg);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the Action enum is `Eq`-comparable — important for the
    /// integration tests in `tui_modes.rs`, which compare returned Vecs
    /// against expected literals.
    #[test]
    fn actions_are_eq_comparable() {
        let q1 = Action::Quit;
        let q2 = Action::Quit;
        assert_eq!(q1, q2);
        assert_ne!(Action::Quit, Action::Up);
    }

    // M140 ext-review F-08: cursor-aware buffer mutations.

    #[test]
    fn char_insert_appends_at_end() {
        let mut buf = String::new();
        let cur = char_insert(&mut buf, 0, 'a');
        assert_eq!(buf, "a");
        assert_eq!(cur, 1);
    }

    #[test]
    fn char_insert_inserts_in_middle() {
        let mut buf = "ad".to_string();
        let cur = char_insert(&mut buf, 1, 'b');
        assert_eq!(buf, "abd");
        assert_eq!(cur, 2);
    }

    #[test]
    fn char_insert_clamps_past_end() {
        let mut buf = "ab".to_string();
        let cur = char_insert(&mut buf, 99, 'c');
        assert_eq!(buf, "abc");
        assert_eq!(cur, 3);
    }

    #[test]
    fn char_backspace_removes_before_cursor() {
        let mut buf = "abc".to_string();
        let cur = char_backspace(&mut buf, 2);
        assert_eq!(buf, "ac");
        assert_eq!(cur, 1);
    }

    #[test]
    fn char_backspace_at_zero_is_noop() {
        let mut buf = "abc".to_string();
        let cur = char_backspace(&mut buf, 0);
        assert_eq!(buf, "abc");
        assert_eq!(cur, 0);
    }

    #[test]
    fn char_backspace_handles_multibyte_at_boundary() {
        let mut buf = "a→b".to_string();
        // → is 3 bytes (UTF-8), 1 char. cursor=2 sits between → and b.
        let cur = char_backspace(&mut buf, 2);
        assert_eq!(buf, "ab");
        assert_eq!(cur, 1);
    }
}
