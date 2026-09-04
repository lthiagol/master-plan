//! M136/M138 + M167: `Mode::Normal` handler.
//!
//! M167 removes the `tab_bar_focused` toggle state — the tab bar is
//! always visual chrome and arrows always move the content. Lane
//! navigation (`previous_lane` / `next_lane`, including Tab / Shift+Tab)
//! is therefore checked alongside the other global keys rather than in
//! a focus-specific branch.
//!
//! M138 still routes contextual keys (e.g. Overview's `r` vs the
//! Milestones lane's `r`) through their lane-specific handlers, so
//! per-mode and per-lane overrides resolve to the same `Action`s they
//! did pre-M167.
//!
//! ## Inside Normal
//!
//! `app.active_mode == Mode::Normal` covers three sub-states:
//!
//! * `content == List` + Overview lane → overview binds (refresh, watch)
//! * `content == List` + Path lane     → path-list binds (refresh)
//! * any other Normal state            → the (content × action) table
//!
//! ## Side effects
//!
//! None. The returned `Vec<Action>` is what
//! [`crate::tui::action::apply_action`] turns into `App` mutations and `mp`
//! subprocess calls.
//!
//! [`crate::tui::action::apply_action`]: crate::tui::action::apply_action

use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::action::Action;
use crate::tui::app::{App, ContentState, Lane};
use crate::tui::keybinds::{any_matches, Keybinds};

/// Handle a keypress in `Mode::Normal`. The returned `Vec<Action>` is
/// applied by the dispatcher in order; an empty vector means the key was
/// either unbound or bound but contextually irrelevant.
pub fn handle_key(key: KeyEvent, app: &App) -> Vec<Action> {
    // M169: Settings lane keys are handled first (including save `s`).
    if app.active_lane == Lane::Settings {
        let actions = crate::tui::modes::settings::handle_key(key, app);
        if !actions.is_empty() {
            return actions;
        }
    }

    // M215 / F-01: Autopilot lane keys dispatch BEFORE the global
    // Esc handler so the panel / replay close path wins over the
    // generic `Action::Esc`. Other Autopilot keys (Space, j/k, o,
    // s, P) also fire before the content-canonical lookup.
    if app.active_lane == Lane::Autopilot {
        if let Some(actions) = handle_autopilot_lane_key(key, app) {
            return actions;
        }
    }

    // Modal menu keys must be handled before content navigation.
    if app.sort_rebind_open() {
        let kb = &app.keybinds;
        if any_matches(&kb.sort_rebind_next, &key) {
            return vec![Action::SortRebindNext];
        }
        if any_matches(&kb.sort_rebind_prev, &key) {
            return vec![Action::SortRebindPrev];
        }
        if any_matches(&kb.sort_rebind_confirm, &key) {
            return vec![Action::SortRebindConfirm];
        }
        if any_matches(&kb.sort_rebind_cancel, &key) {
            return vec![Action::SortRebindCancel];
        }
        // Other keys are no-ops while the menu is open (the user
        // is interacting with the menu, not the list).
        return Vec::new();
    }

    let kb = &app.keybinds;

    // Global keys (always reachable in Normal, never in Input / Help /
    // ReviewMenu / AnnotationThread / Settings — those modes handle their
    // own keys locally). These win over the content-specific bindings
    // below, matching pre-M167 order.
    if any_matches(&kb.quit, &key) {
        return vec![Action::Quit];
    }
    if any_matches(&kb.escape, &key) {
        return vec![Action::Esc];
    }
    if any_matches(&kb.open_settings, &key) {
        if app.active_lane == Lane::Settings {
            return Vec::new();
        }
        // M198 / M214: the Settings lane is always visible
        // (it's a control surface, not a domain lane) so its
        // position in the visible list is deterministic. Use
        // the visible list so the digit-jump dispatch below
        // agrees.
        let idx = Lane::ordered_visible(app.show_autopilot_tab)
            .iter()
            .position(|l| *l == Lane::Settings)
            .expect("Settings lane must exist");
        return vec![Action::JumpLane(idx)];
    }
    // Lane navigation (Tab / Shift+Tab / ← / → / h / l) is reachable from
    // any content state in Normal — M167 dropped the "tab-bar focused"
    // gate so users can move lanes whenever they're not in an input.
    // M169-rev2 user report: 'h' is bound to both `previous_lane`
    // (vim-style alias) and `keybinds.hide_done`. On List view the
    // contextual binding wins — the user pressing 'h' on Milestones
    // toggles hide_done, not PreviousLane (which moves them to
    // Overview). Detail view doesn't have a hide_done semantic so
    // 'h' stays a PreviousLane alias there.
    if any_matches(&kb.previous_lane, &key) {
        if app.content == ContentState::List && any_matches(&kb.hide_done, &key) {
            return vec![Action::ToggleHideDone];
        }
        return vec![Action::PreviousLane];
    }
    if any_matches(&kb.next_lane, &key) {
        return vec![Action::NextLane];
    }
    // Digit lane-jumps are positional, not per-action bindings (indexed
    // bindings are out of scope for the v1/v2 keybind rework).
    if matches!(key.code, KeyCode::Char(c) if c.is_ascii_digit()) {
        // M198 / M214: digits index into the *visible* lane list so
        // the on-screen tab number matches the key. When the
        // Autopilot tab is hidden, the operator sees 6 tabs and
        // digits 1..=6 are valid; the seventh digit is a no-op.
        let max_idx = Lane::ordered_visible(app.show_autopilot_tab).len();
        if max_idx <= 9 {
            if let KeyCode::Char(c) = key.code {
                if let Some(d) = c.to_digit(10) {
                    let idx = (d as usize).saturating_sub(1);
                    if idx < max_idx {
                        return vec![Action::JumpLane(idx)];
                    }
                }
            }
        }
    }
    // M167: detail-section navigation (only consumed on MilestoneDetail).
    if app.content == ContentState::MilestoneDetail {
        if any_matches(&kb.next_section, &key) {
            return vec![Action::NextSection];
        }
        if any_matches(&kb.prev_section, &key) {
            return vec![Action::PrevSection];
        }
        if any_matches(&kb.next_item, &key) {
            return vec![Action::NextItem];
        }
        if any_matches(&kb.prev_item, &key) {
            return vec![Action::PrevItem];
        }
    }

    if app.content == ContentState::List && app.active_lane == Lane::Overview {
        return handle_overview_lane_key(key, app, kb);
    }

    if app.content == ContentState::List && app.active_lane == Lane::Path {
        return handle_path_list_key(key, app, kb);
    }

    handle_event_in_normal(key, app)
}

/// Overview lane special keys (when the list is showing).
///
/// `refresh` and `reopen` (which share the `r`/`R` shape) both trigger
/// a refresh here, preserving the pre-M167 `'r' | 'R'` behavior on the
/// dashboard.
fn handle_overview_lane_key(key: KeyEvent, app: &App, kb: &Keybinds) -> Vec<Action> {
    if any_matches(&kb.refresh, &key) || any_matches(&kb.reopen, &key) {
        return vec![Action::RefreshLane];
    }
    if any_matches(&kb.help, &key) {
        return vec![Action::OpenHelp];
    }
    handle_event_in_normal(key, app)
}

/// Path lane + List content. `refresh` reloads the path data; everything else
/// falls through to the content dispatch.
fn handle_path_list_key(key: KeyEvent, app: &App, kb: &Keybinds) -> Vec<Action> {
    if any_matches(&kb.refresh, &key) {
        return vec![Action::RefreshLane];
    }
    handle_event_in_normal(key, app)
}

/// M215 / F-01: Autopilot lane key dispatch. The handler returns
/// `Some(actions)` when it consumed a keypress; `None` when the
/// keypress should fall through to the content-canonical handler
/// (so `Up` / `Down` still work as escape hatches for legacy
/// callers).
///
/// M222: every match below now consults `app.keybinds.lane_autopilot`
/// instead of the pre-M222 hardcoded `KeyCode` literals. The
/// defaults preserve the same shapes (Space / j / k / o / s / P /
/// Esc) so a user without `~/.config/raul/keybinds.toml` sees the
/// pre-M222 behavior verbatim.
///
/// Default keymap (no overrides — see `Keybinds::default()`):
/// - Space → toggle picker selection
/// - j / k → move picker cursor
/// - `<o>` → toggle override panel
/// - `<s>` → start (validate + shell out to `mp autopilot start`)
/// - capital P → open past-session replay
/// - Esc → close the panel / replay shell if one is open
fn handle_autopilot_lane_key(key: KeyEvent, app: &App) -> Option<Vec<Action>> {
    use crate::tui::keybinds::any_matches;
    let ap = &app.keybinds.lane_autopilot;

    // Esc (or any binding in `ap.close`) closes the panel or replay
    // shell first; otherwise falls through to the global Esc handler.
    if any_matches(&ap.close, &key) {
        if app.autopilot.panel_open {
            return Some(vec![Action::AutopilotTogglePanel]);
        }
        if app.autopilot.replay_open {
            return Some(vec![Action::AutopilotCloseReplay]);
        }
    }

    // Toggle picker selection. The picker is only reachable when
    // no panel / replay is open.
    if any_matches(&ap.select, &key) && !app.autopilot.panel_open && !app.autopilot.replay_open {
        return Some(vec![Action::AutopilotToggleSelect]);
    }

    // Move picker cursor (vim-style aliases). Same gating as
    // `select`.
    if !app.autopilot.panel_open && !app.autopilot.replay_open {
        if any_matches(&ap.move_picker_down, &key) {
            return Some(vec![Action::AutopilotMovePicker { delta: 1 }]);
        }
        if any_matches(&ap.move_picker_up, &key) {
            return Some(vec![Action::AutopilotMovePicker { delta: -1 }]);
        }
    }

    // `<o>` (or whatever is bound to `ap.toggle_panel`) toggles
    // the override panel. On any lane the global
    // `keybinds.cycle_sort` (default plain `o`) is bound to
    // `Action::CycleSortNext`; the Autopilot lane's panel key
    // wins because it dispatches first.
    if any_matches(&ap.toggle_panel, &key) {
        return Some(vec![Action::AutopilotTogglePanel]);
    }

    // `<s>` (or whatever is bound to `ap.start`) starts the run.
    // Validation lives in `apply_action` (`Action::AutopilotStart`);
    // an empty selection or open panel / replay is a no-op there.
    if any_matches(&ap.start, &key) {
        return Some(vec![Action::AutopilotStart]);
    }

    // `ap.replay` (default capital P) opens the past-session
    // replay.
    if any_matches(&ap.replay, &key) {
        return Some(vec![Action::AutopilotOpenReplay]);
    }

    // M216 AC-03: `ap.refresh` (default `r`) triggers the manual
    // refresh. The dispatcher shells out to `mp autopilot session
    // show <id>` + `mp autopilot status` and repopulates the
    // lane's status_graph / queue_view / violations / detail_panel
    // / ac_detail / telemetry fields.
    if any_matches(&ap.refresh, &key) {
        return Some(vec![Action::AutopilotRefresh]);
    }

    // M216 AC-06: pause / resume / cancel / restart.
    if any_matches(&ap.pause, &key) {
        return Some(vec![Action::AutopilotPause]);
    }
    if any_matches(&ap.resume, &key) {
        return Some(vec![Action::AutopilotResume]);
    }
    if any_matches(&ap.cancel, &key) {
        return Some(vec![Action::AutopilotCancel]);
    }
    if any_matches(&ap.restart, &key) {
        return Some(vec![Action::AutopilotRestart]);
    }

    // M216 AC-06: steer — send a one-off message through
    // `mp autopilot control steer`. The handler reads the
    // buffer's last line as the message body; the buffer is the
    // picker input (no lane-level input field yet — pre-M217
    // surface). The keybind is the contract.
    if any_matches(&ap.steer, &key) {
        return Some(vec![Action::AutopilotSteer {
            message: String::new(),
        }]);
    }

    // M216 AC-05: open / close the detail pane. `<d>` enters
    // detail for the picker cursor's milestone (or the active
    // queue item when a session is loaded); capital `D` closes.
    if any_matches(&ap.open_detail, &key) {
        let milestone_id = app
            .autopilot
            .picker
            .cursor_candidate()
            .map(|c| c.id.clone())
            .or_else(|| {
                app.autopilot
                    .queue_view
                    .as_ref()
                    .and_then(|qv| qv.active_milestone_id().map(|s| s.to_string()))
            })
            .unwrap_or_default();
        if milestone_id.is_empty() {
            return None;
        }
        return Some(vec![Action::AutopilotOpenDetail { milestone_id }]);
    }
    if any_matches(&ap.close_detail, &key) {
        return Some(vec![Action::AutopilotCloseDetail]);
    }

    None
}

/// The (content × action) dispatch. `Keybinds::resolve` gives the
/// content-canonical action for the key; each content state then maps that
/// action to the concrete `Action`(s) it triggers. An unbound key or a
/// content/action pair with no behavior yields an empty `Vec`.
fn handle_event_in_normal(key: KeyEvent, app: &App) -> Vec<Action> {
    let resolved = match app.keybinds.resolve(&key) {
        Some(a) => a,
        None => return Vec::new(),
    };

    match app.content {
        ContentState::List => match resolved {
            Action::Quit => vec![Action::Quit],
            Action::OpenHelp => vec![Action::OpenHelp],
            Action::ToggleFilter => vec![Action::ToggleFilter],
            Action::ToggleHideDone => vec![Action::ToggleHideDone],
            Action::OpenLifecycleFilter => vec![Action::OpenLifecycleFilter],
            Action::ApplyGroomingPreset => vec![Action::ApplyGroomingPreset],
            Action::OpenSearch => vec![Action::OpenSearch],
            Action::CycleSortNext => vec![Action::CycleSortNext],
            // S menu was rendering correctly but the keypress never reached
            // open_sort_rebind() — handle_event_in_normal was missing
            // this arm, so kb.resolve() returned the action and the
            // dispatcher dropped it on the floor.
            Action::OpenSortRebind => vec![Action::OpenSortRebind],
            Action::Up => vec![Action::Up],
            Action::Down => vec![Action::Down],
            Action::PageUp => vec![Action::PageUp],
            Action::PageDown => vec![Action::PageDown],
            Action::Enter => vec![Action::Enter],
            _ => Vec::new(),
        },
        ContentState::MilestoneDetail => match resolved {
            Action::Up => vec![Action::Up],
            Action::Down => vec![Action::Down],
            Action::PageUp => vec![Action::Up],
            Action::PageDown => vec![Action::Down],
            Action::Enter => vec![Action::OpenAnnotationThread],
            Action::OpenReviewMenu => vec![Action::OpenReviewMenu],
            Action::ToggleApproval => vec![Action::ToggleApproval],
            _ => Vec::new(),
        },
        ContentState::BacklogDetail => match resolved {
            Action::Up => vec![Action::Up],
            Action::Down => vec![Action::Down],
            Action::PageUp => vec![Action::Up],
            Action::PageDown => vec![Action::Down],
            _ => Vec::new(),
        },
        ContentState::CoApproval => match resolved {
            Action::Enter => vec![Action::ConfirmCoApproval],
            Action::ToggleApproval => {
                vec![Action::SetCoApprovalAction(
                    crate::tui::app::CoApprovalAction::Approve,
                )]
            }
            Action::ReopenAnnotation => {
                vec![Action::SetCoApprovalAction(
                    crate::tui::app::CoApprovalAction::Reject,
                )]
            }
            Action::Up => vec![Action::Up],
            Action::Down => vec![Action::Down],
            _ => Vec::new(),
        },
        ContentState::AnnotationThread => Vec::new(),
    }
}
