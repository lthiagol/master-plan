//! M136: Mode enum for raul's TUI dispatch.
//!
//! Prior to M136 the runner held three independent inline mode flags on `App`
//! (`show_help: bool`, `input_mode: Option<InputMode>`, `show_review_menu: bool`)
//! and a fourth (`input_buffer: String`) that was only meaningful while
//! `input_mode` was set. The dispatch chain in `runner.rs` mixed global keys,
//! input-mode keys, tab-bar focus, lane-specific path special cases,
//! and a content-keyed match in `handle_event` — a 450-line block that was
//! hard to test and easy to forget a path on.
//!
//! M136 collapses those four pieces into a single `Mode` enum on `App` so the
//! dispatcher becomes a one-line match: `match (app.active_mode, key) -> …`.
//! Each variant carries the mode-local state (input buffer, review-menu
//! selection) so closing a mode drops its state by construction and there is
//! no way for a stale flag to leak into a different mode's reads.
//!
//! ## Variant overview
//!
//! | Variant | Replaces | Meaning |
//! |---------|----------|---------|
//! | `Normal` | (always-on "no mode" state) | The default mode: list / detail / co-approval content screens. Lane navigation, list interaction, detail scrolling, and milestone-detail commands all live here. |
//! | `Input` | `input_mode: Option<InputMode>` + `input_buffer: String` | A text-input overlay is open (creating an annotation). The text buffer lives inside the variant so cancelling the input drops it by construction. |
//! | `Help` | `show_help: bool` | The help overlay is open and exclusive (render() returns early while the help screen is up). |
//! | `AnnotationThread` | (was a `ContentState` value) | The annotation thread view inside a milestone. M136 promotes it to a mode because, like `Input` and `ReviewMenu`, its keys have nothing in common with the default content keys. |
//! | `ReviewMenu` | `show_review_menu: bool` | The review-menu overlay is open and exclusive. Selection state lives inside the variant. |
//!
//! M140's `Settings` variant was removed in M169 — Settings is now
//! `Lane::Settings` carrying `App.settings: Option<SettingsState>`
//! instead of a mode variant. The dispatcher no longer routes a
//! `Mode::Settings` arm.
//!
//! ## Mode-local state
//!
//! Variants that need per-mode data carry it as a struct in the variant so
//! the mode's *full* state disappears together when transitioning away:
//!
//!   * `Input(InputState)` — `target`, `kind`, `buffer`
//!   * `ReviewMenu(ReviewMenuState)` — `items`, `selected`
//!
//! `Normal`, `Help`, and `AnnotationThread` carry no payload —
//! any data the renderer/dispatcher needs in those modes (the current
//! `ContentState`, the currently-selected milestone, the footer's `open_only`
//! filter state, …) already lives on `App` and is shared across modes.
//! `Settings` is no longer a mode; its state lives at `App.settings`.
//!
//! ## Dispatch contract
//!
//! The top-level dispatch is `dispatch_key(app, key) -> Vec<Action>` (see
//! [`mod.rs`](crate)). Per-mode handlers live in `tui/modes/` and never
//! mutate `App` or shell out to `mp` — side effects are confined to
//! `apply_action(app, action)` in `tui/action.rs`.

/// State carried inside `Mode::Input`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputState {
    pub target: String,
    pub kind: String,
    pub buffer: String,
}

/// State carried inside `Mode::ReviewMenu`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewMenuState {
    pub items: Vec<String>,
    pub selected: usize,
}

/// M168 BF-03: the Settings modal is a flat list — there is no
/// Sections-vs-Fields focus toggle anymore. The state holds a single
/// `selected_idx` over the flat list, and the field-edit popup
/// (when open) carries its own editing state inside
/// `SettingsState::edit`.
///
/// `SettingsFocus::Editing` remains as a marker used by the dispatcher
/// to route keys to the input overlay vs. the flat list while the edit
/// popup is up. The popup is either up (`Editing` + `Some(edit)`) or
/// down (`Fields` + `None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsFocus {
    /// Flat list has focus; Up/Down/Enter navigate.
    #[default]
    Fields,
    /// Field-edit popup is up; Esc cancels, Enter saves, all chars route
    /// to the input buffer.
    Editing,
}

/// In-progress field edit inside Settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEdit {
    pub key: String,
    pub buffer: String,
    /// M140 ext-review F-08: byte/cell index into `buffer` where the
    /// caret sits. Maintained on every `PushInputChar` (advance by
    /// the new char's length) and `PopInputChar` (clamp to current
    /// cell length). Renderers use this to position a block cursor.
    pub cursor: usize,
    pub errors: Vec<String>,
}

/// M169: lane-scoped Settings state on `App.settings` while
/// `active_lane == Lane::Settings`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
    /// Full `config` object from `mp config show`.
    pub config: serde_json::Value,
    /// Index into the flat key list (see `modes::settings::SETTINGS_KEYS`).
    /// Clamped to `0..SETTINGS_KEYS.len()` on every Up/Down.
    pub selected_idx: usize,
    pub focus: SettingsFocus,
    pub edit: Option<SettingsEdit>,
    /// Pending writes flushed by `Action::SettingsSave` (`s` on the lane).
    ///
    /// **M169-rev (LOW fix):** `BTreeMap` so iteration in
    /// `apply_settings_save` is deterministic across
    /// runs (key-sorted) — `HashMap` would make the on-disk write order
    /// non-deterministic and the JSON diff noisy for users staging
    /// multiple keys at once.
    pub staged_edits: std::collections::BTreeMap<String, String>,
}

impl SettingsState {
    pub fn new(config: serde_json::Value) -> Self {
        Self {
            config,
            selected_idx: 0,
            focus: SettingsFocus::Fields,
            edit: None,
            staged_edits: std::collections::BTreeMap::new(),
        }
    }

    pub fn has_staged_edits(&self) -> bool {
        !self.staged_edits.is_empty()
    }
}

impl ReviewMenuState {
    /// The canonical review-menu items. Always the same four labels, in
    /// this order; the consumer (`App::open_review_menu`) clones this
    /// into the `Mode::ReviewMenu(_)` variant. A future optimization
    /// (M140+) could switch to a `&'static [&'static str]` constant and
    /// reference it from the variant, but for M136 the owned `Vec<String>`
    /// keeps `Mode::ReviewMenu` `Eq`-comparable for the integration tests.
    ///
    /// M172 S6: the menu grew a "Set dependency" item. The item is
    /// always available (the dispatcher doesn't gate it on in-progress
    /// siblings — see the M172 spec for the rationale: the user can
    /// also add a dependency to a milestone that has zero in-progress
    /// siblings).
    pub fn canonical() -> Vec<String> {
        vec![
            "Approve milestone".to_string(),
            "Block milestone".to_string(),
            "Unblock milestone".to_string(),
            "Request grooming".to_string(),
            "Set dependency".to_string(),
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Mode {
    /// Default state — list / detail / co-approval screens. Lane navigation,
    /// list interaction, and milestone-detail commands live here.
    #[default]
    Normal,
    /// A text-input overlay is open. The buffer is mode-local so cancelling
    /// the input drops it by construction.
    Input(InputState),
    /// The help overlay is open (render() returns early while this is up).
    Help,
    /// The annotation thread view inside a milestone.
    AnnotationThread,
    /// The review-menu overlay is open. Items + selected index live inside
    /// the variant; closing the menu drops both by construction.
    ReviewMenu(ReviewMenuState),
    /// M185: multi-select lifecycle filter modal (Milestones lane).
    LifecycleFilter(LifecycleFilterState),
    /// M186: live substring search input (Milestones/Backlog/Ideas).
    SearchInput(SearchInputState),
}

/// M185: state for the lifecycle filter modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleFilterState {
    /// Cursor index into [`crate::tui::progress::LIFECYCLE_FILTER_OPTIONS`].
    pub selected: usize,
    /// Working set while the modal is open.
    pub draft: std::collections::BTreeSet<String>,
    /// Snapshot of `app.milestone_filter` at open time (Esc restores).
    pub prior: std::collections::BTreeSet<String>,
}

/// M186: state for the live search input. The per-lane committed term
/// lives separately on `App::lane_search`; this struct holds only the
/// in-flight draft plus a snapshot of the prior term (Esc restores).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchInputState {
    pub buffer: String,
    pub prior: String,
}
