//! Control-first setup form, split geometry, and sidebar state for
//! the Autopilot lane.
//!
//! The lane is two views over one state block:
//!
//! * **Split** — a setup form on the left (six stacked sections plus
//!   the control row) and a tabbed sidebar on the right. The split
//!   width, the active tab, and the sidebar's visibility are *UI
//!   preferences* and live in mp config under `ui.autopilot.*`.
//! * **Takeover** — a full-screen dashboard that replaces the split
//!   while `mp autopilot status` reports a live run.
//!
//! Everything in this module is pure data plus pure functions. The
//! renderers read it, `apply_action` mutates it, and the persistence
//! path turns a changed field into an `mp config set` argv (see
//! [`config_set_argv`]) so raul never owns a state file of its own.
//!
//! The `ui.autopilot.*` constants are duplicated from mp on purpose:
//! raul is a separate crate and cannot link `mp::config`. The mp-side
//! validation (the 25..=75 clamp, the three-tab enum) is the
//! authority; the bounds here exist so the clamp happens in the
//! renderer even if a config write is rejected.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use ratatui::layout::Rect;

/// Clamp window for the left column, as a percentage of the lane
/// width. Mirrors `mp`'s `UI_AUTOPILOT_SPLIT_PCT_MIN` / `_MAX`.
pub const SPLIT_PCT_MIN: u32 = 25;
pub const SPLIT_PCT_MAX: u32 = 75;
/// Default left-column width — the 40/60 geometry.
pub const SPLIT_PCT_DEFAULT: u32 = 40;

/// Tab order. `v` walks forward, `Shift+V` walks backward.
pub const TABS: [&str; 3] = ["progress", "activity", "state"];

/// The default tab (also the fallback for an unparseable stored
/// value).
pub const TAB_DEFAULT: SidebarTab = SidebarTab::Progress;

/// Which pane of the sidebar is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarTab {
    /// The default tab. `#[default]` here is what makes
    /// `SidebarTab::default()` the Progress tab; `TAB_DEFAULT` names
    /// the same value for callers that want a constant.
    #[default]
    Progress,
    Activity,
    State,
}

impl SidebarTab {
    /// The stored config spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Activity => "activity",
            Self::State => "state",
        }
    }

    /// Parse a stored config value, falling back to the default for
    /// anything unrecognized. Falling back rather than erroring keeps
    /// a hand-edited `config.json` from bricking the lane.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "activity" => Self::Activity,
            "state" => Self::State,
            _ => Self::Progress,
        }
    }

    /// Label rendered on the tab header.
    pub fn label(self) -> &'static str {
        match self {
            Self::Progress => "Progress",
            Self::Activity => "Activity",
            Self::State => "State",
        }
    }
}

impl SidebarTab {
    /// The default tab, as a constant. The enum's `Default` derive
    /// agrees; this exists so call sites read as a named preference
    /// rather than a bare variant.
    pub const DEFAULT: Self = Self::Progress;
}

/// Whether Start runs in the foreground or detaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    #[default]
    Normal,
    Detached,
}

impl RunMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Detached => "detached",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Detached => "detached",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "detached" => Self::Detached,
            _ => Self::Normal,
        }
    }
}

/// The role slugs a setup form shows, in render order. The topology
/// chip decides how many of these are live: `one-agent` drives
/// `orchestrator` only, `two-agent` adds `runner`, `three-agent` adds
/// `reviewer`.
pub const ALL_ROLES: [&str; 3] = ["orchestrator", "runner", "reviewer"];

/// The roles a topology drives, in render order.
pub fn roles_for_topology(topology: &str) -> &'static [&'static str] {
    match topology {
        "one-agent" => &ALL_ROLES[0..1],
        "two-agent" => &ALL_ROLES[0..2],
        _ => &ALL_ROLES,
    }
}

/// Harness choices offered per role. Mirrors
/// [`super::ALLOWED_HARNESSES`]; the first entry is the implicit
/// "inherit the global default" chip.
pub const HARNESS_CHOICES: [&str; 4] = ["inherit", "opencode", "cursor", "pi"];

/// One chip in the setup form. `Option<String>` is `Some` when the
/// chip is the selected one; the renderer dims the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub label: String,
    pub value: Option<String>,
}

/// The six-section setup form.
///
/// Persistence split: topology, per-role harness, and both commit
/// toggles round-trip through mp. Milestone selection and run mode are
/// **per-run** — they are deliberately not persisted, because a stale
/// selection from last week is worse than no selection at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupForm {
    pub topology: String,
    /// `role -> harness`. A missing role means "inherit".
    pub harness: BTreeMap<String, String>,
    /// When true, the next harness change applies to every role at
    /// once instead of one row at a time.
    pub harness_uniform: bool,
    /// Per-run milestone selection. Never persisted.
    pub selected: Vec<String>,
    /// Per-run filter: show only milestones in a drivable lifecycle.
    pub only_ready: bool,
    pub commit_after_execute: bool,
    pub push_after_review: bool,
    /// Per-run mode. Never persisted.
    pub run_mode: RunMode,
}

impl Default for SetupForm {
    fn default() -> Self {
        Self {
            topology: super::DEFAULT_TOPOLOGY.to_string(),
            harness: BTreeMap::new(),
            harness_uniform: true,
            selected: Vec::new(),
            only_ready: false,
            commit_after_execute: false,
            push_after_review: false,
            run_mode: RunMode::Normal,
        }
    }
}

impl SetupForm {
    pub fn new() -> Self {
        Self::default()
    }

    /// The harness chosen for `role`, or the string the form writes
    /// back to mp. An empty or inherited value writes `inherit`.
    pub fn harness_for(&self, role: &str) -> &str {
        self.harness
            .get(role)
            .map(String::as_str)
            .unwrap_or("inherit")
    }

    /// Pick a topology. Widening the topology adds rows (their harness
    /// is unset / inherited); narrowing keeps the values the removed
    /// rows had, so widening back restores them.
    pub fn set_topology(&mut self, topology: &str) {
        if super::ALLOWED_TOPOLOGIES.contains(&topology) {
            self.topology = topology.to_string();
        }
    }

    /// Set one role's harness, or every live role when the uniform
    /// toggle is on.
    pub fn set_harness(&mut self, role: &str, harness: &str) {
        let roles: Vec<&str> = if self.harness_uniform {
            roles_for_topology(&self.topology).to_vec()
        } else {
            vec![role]
        };
        for r in roles {
            if harness == "inherit" || harness.is_empty() {
                self.harness.remove(r);
            } else {
                self.harness.insert(r.to_string(), harness.to_string());
            }
        }
    }

    /// Toggle a milestone's per-run selection.
    pub fn toggle_milestone(&mut self, id: &str) {
        match self.selected.iter().position(|s| s == id) {
            Some(index) => {
                self.selected.remove(index);
            }
            None => self.selected.push(id.to_string()),
        }
    }

    /// The one-line summary under Start: `<n> milestones · <topology>
    /// · <run mode>`. No time estimate — the milestone explicitly
    /// rules one out.
    pub fn summary(&self) -> String {
        let n = self.selected.len();
        format!(
            "{} milestone{} · {} · {}",
            n,
            if n == 1 { "" } else { "s" },
            self.topology,
            self.run_mode.label()
        )
    }
}

/// The outcome the operator picked in the detached-mode popover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetachedChoice {
    /// Keep the run detached and hand off to the control row.
    Confirm,
    /// Jump to the override panel to set model / skill / extras.
    ConfigureExtras,
    /// Revert to normal mode and leave the popover.
    Back,
}

impl DetachedChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::Confirm => "Confirm",
            Self::ConfigureExtras => "Configure extras",
            Self::Back => "Back",
        }
    }
}

/// The detached-chip popover. `None` when the popover is closed; a
/// detached run is only ever armed through this struct, so there is
/// no way to reach a detached Start without passing the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedConfirm {
    /// The run mode the popover is confirming. Always `Detached`
    /// today, but kept as a field so the popover renders the value
    /// it is actually asking about.
    pub mode: RunMode,
    /// Index of the highlighted option.
    pub cursor: usize,
}

/// The three popover options in render order.
pub const DETACHED_CHOICES: [DetachedChoice; 3] = [
    DetachedChoice::Confirm,
    DetachedChoice::ConfigureExtras,
    DetachedChoice::Back,
];

impl DetachedConfirm {
    pub fn new() -> Self {
        Self {
            mode: RunMode::Detached,
            cursor: 0,
        }
    }

    /// The option the operator currently highlights.
    pub fn current(&self) -> DetachedChoice {
        DETACHED_CHOICES[self.cursor.min(DETACHED_CHOICES.len() - 1)]
    }

    pub fn move_cursor(&mut self, delta: i64) {
        let len = DETACHED_CHOICES.len() as i64;
        let next = self.cursor as i64 + delta;
        self.cursor = next.rem_euclid(len) as usize;
    }
}

impl Default for DetachedConfirm {
    fn default() -> Self {
        Self::new()
    }
}

/// One row of the read-only per-milestone peek.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeekAc {
    pub id: String,
    pub status: String,
}

/// The read-only peek opened from a takeover row. Sourced from
/// `mp show milestone <id>`; raul renders it and never writes to it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MilestonePeek {
    pub milestone_id: String,
    pub title: String,
    pub outcome: String,
    pub acs: Vec<PeekAc>,
}

impl MilestonePeek {
    /// Build a peek from a `mp show milestone <id>` payload. Returns
    /// `None` when the payload has no `intent.outcome` to show, so a
    /// truncated response renders no modal rather than an empty one.
    pub fn from_show_json(id: &str, payload: &serde_json::Value) -> Option<Self> {
        let outcome = payload.get("intent")?.get("outcome")?.as_str()?.to_string();
        let title = payload
            .get("milestone")
            .and_then(|m| m.get("title"))
            .and_then(|t| t.as_str())
            .unwrap_or(id)
            .to_string();
        let acs = payload
            .get("acceptance_criteria")
            .and_then(|a| a.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        Some(PeekAc {
                            id: row.get("id")?.as_str()?.to_string(),
                            status: row
                                .get("status")
                                .and_then(|s| s.as_str())
                                .unwrap_or("pending")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            milestone_id: id.to_string(),
            title,
            outcome,
            acs,
        })
    }
}

/// Split geometry + sidebar preferences. The three persisted fields
/// are the ones in `ui.autopilot.*`; `state_tab_hidden` is derived
/// session state (Back from the takeover while a run is live), never
/// persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotLayout {
    pub split_pct: u32,
    pub sidebar_tab: SidebarTab,
    pub sidebar_visible: bool,
    /// True while a run is live and the operator has pressed Back
    /// from the takeover. While set, the State tab is not offered.
    pub state_tab_hidden: bool,
}

impl Default for AutopilotLayout {
    fn default() -> Self {
        Self {
            split_pct: SPLIT_PCT_DEFAULT,
            sidebar_tab: TAB_DEFAULT,
            sidebar_visible: true,
            state_tab_hidden: false,
        }
    }
}

impl AutopilotLayout {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clamp a raw split percentage into the legal window. Applied on
    /// every read *and* every drag so no code path can produce an
    /// out-of-range column.
    pub fn clamp_split_pct(pct: i64) -> u32 {
        pct.clamp(SPLIT_PCT_MIN as i64, SPLIT_PCT_MAX as i64) as u32
    }

    /// Set the split width, clamped.
    pub fn set_split_pct(&mut self, pct: i64) {
        self.split_pct = Self::clamp_split_pct(pct);
    }

    /// The left column's width in cells for a lane `width` columns
    /// wide, leaving at least one cell for the sidebar border.
    pub fn left_width(&self, width: u16) -> u16 {
        if !self.sidebar_visible || width < 4 {
            return width;
        }
        let pct = self.split_pct.min(SPLIT_PCT_MAX) as f64 / 100.0;
        let left = ((width as f64) * pct).round() as u16;
        left.clamp(1, width - 1)
    }

    /// The tabs the operator can currently reach. The State tab is
    /// withheld while a run is live and the operator has returned to
    /// the split via Back — the takeover owns the screen in that
    /// mode, so a State tab would show a stale snapshot.
    pub fn selectable_tabs(&self) -> Vec<SidebarTab> {
        let mut tabs = vec![SidebarTab::Progress, SidebarTab::Activity];
        if !self.state_tab_hidden {
            tabs.push(SidebarTab::State);
        }
        tabs
    }

    /// Step to the next (or previous) selectable tab, wrapping. If the
    /// current tab is not selectable — e.g. the operator was on State
    /// and then went Back while the run continued — the first
    /// selectable tab is chosen instead.
    pub fn cycle_tab(&mut self, delta: i64) {
        let tabs = self.selectable_tabs();
        let current = tabs
            .iter()
            .position(|t| *t == self.sidebar_tab)
            .map(|i| i as i64)
            .unwrap_or(-1);
        let len = tabs.len() as i64;
        let next = (current + delta).rem_euclid(len) as usize;
        self.sidebar_tab = tabs[next];
    }

    /// Flip sidebar visibility. The new value is the caller's cue to
    /// write `ui.autopilot.sidebar_visible`.
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }
}

/// Derive the read-only topology id: `<topology>-<NNN>` where `NNN`
/// is `1 + session_count`, zero-padded to three digits.
///
/// Derived on render from the live session list; nothing is
/// persisted, so two raul windows showing the same plan agree only
/// until the next run starts — which is the point of a *display*
/// id rather than a stored one.
pub fn topology_id(topology: &str, session_count: usize) -> String {
    format!("{topology}-{:03}", session_count.saturating_add(1))
}

/// The exact `mp` argv for persisting one Autopilot preference.
///
/// Returned as data so the persistence contract is testable without
/// spawning a process, and so the fake-runner assertions pin the
/// literal call rather than a side effect.
pub fn config_set_argv(key: &str, value: &str) -> Vec<String> {
    vec![
        "config".to_string(),
        "set".to_string(),
        key.to_string(),
        value.to_string(),
    ]
}

/// The `mp autopilot config set` argv for a topology / per-role
/// harness choice. Those live in a different config section from the
/// `ui.autopilot.*` preferences, so they take a different mp command.
pub fn autopilot_config_set_argv(key: &str, value: &str) -> Vec<String> {
    vec![
        "autopilot".to_string(),
        "config".to_string(),
        "set".to_string(),
        key.to_string(),
        value.to_string(),
    ]
}

/// The `mp config set` argv for a commit-behavior toggle.
pub fn commit_toggle_argv(field: &str, enabled: bool) -> Vec<String> {
    config_set_argv(
        &format!("agent.automation.{field}"),
        if enabled { "true" } else { "false" },
    )
}

/// The `mp config set` argv for one of the three `ui.autopilot.*`
/// UI preferences.
pub fn ui_pref_argv(field: &str, value: &str) -> Vec<String> {
    config_set_argv(&format!("ui.autopilot.{field}"), value)
}

/// Parse the `ui.autopilot.*` triple out of a `mp config get` shaped
/// payload. Unknown / missing values fall back to the documented
/// defaults rather than erroring — a preferences read must never be
/// able to fail the lane load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiPrefs {
    pub split_pct: u32,
    pub sidebar_tab: SidebarTab,
    pub sidebar_visible: bool,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            split_pct: SPLIT_PCT_DEFAULT,
            sidebar_tab: TAB_DEFAULT,
            sidebar_visible: true,
        }
    }
}

impl UiPrefs {
    /// Build prefs from an iterator of `(key, value)` pairs, e.g. the
    /// rows of a `mp config get ui.autopilot.split_pct` batch.
    pub fn from_pairs<'a, I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut prefs = Self::default();
        for (key, value) in pairs {
            match key {
                "ui.autopilot.split_pct" => {
                    prefs.split_pct = value
                        .parse::<i64>()
                        .map(AutopilotLayout::clamp_split_pct)
                        .unwrap_or(SPLIT_PCT_DEFAULT);
                }
                "ui.autopilot.sidebar_tab" => prefs.sidebar_tab = SidebarTab::parse(value),
                "ui.autopilot.sidebar_visible" => {
                    prefs.sidebar_visible = matches!(value, "true" | "1" | "yes");
                }
                _ => {}
            }
        }
        prefs
    }

    /// Apply these prefs onto a layout.
    pub fn apply_to(&self, layout: &mut AutopilotLayout) {
        layout.set_split_pct(self.split_pct as i64);
        layout.sidebar_tab = self.sidebar_tab;
        layout.sidebar_visible = self.sidebar_visible;
    }
}

/// The picker's drivable-milestone ids, in picker order. The
/// Milestones section's chips and the takeover's fallback rows both read
/// this, so "what is queued" has one answer.
pub fn picker_candidates(app: &crate::tui::app::App) -> Vec<String> {
    app.autopilot
        .picker
        .candidates
        .iter()
        .map(|c| c.id.clone())
        .collect()
}

/// Move the picker cursor to `id` and toggle its selection. One gesture,
/// because a chip is both the row you point at and the row you want:
/// landing the cursor and flipping the flag in one click matches what
/// the keyboard's Space does, and makes "click the milestone I mean"
/// a single action.
pub fn click_candidate(app: &mut crate::tui::app::App, id: &str) -> bool {
    let Some(index) = app
        .autopilot
        .picker
        .candidates
        .iter()
        .position(|c| c.id == id)
    else {
        return false;
    };
    app.autopilot.picker.cursor = index;
    app.autopilot.setup.toggle_milestone(id);
    app.touch();
    true
}

/// One queued-milestone row's worth of data, as the takeover shows it.
/// A projection of [`super::QueueRow`] with the fields the takeover
/// renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakeoverRow {
    pub id: String,
    pub lifecycle: String,
    pub cycle: u64,
    /// True for the row the drive is currently working on, so a glance
    /// identifies the active step.
    pub active: bool,
}

/// The takeover's rows. Prefers the live queue view (populated by the
/// poller from `mp autopilot session show`); falls back to the setup
/// form's per-run selection so a run that started before the first
/// refresh still shows what is queued.
pub fn queued_milestones(app: &crate::tui::app::App) -> Vec<TakeoverRow> {
    if let Some(view) = &app.autopilot.queue_view {
        if !view.rows.is_empty() {
            return view
                .rows
                .iter()
                .map(|row| TakeoverRow {
                    id: row.milestone_id.clone(),
                    lifecycle: row.lifecycle.clone(),
                    // The drive's own count, from the matching
                    // `session.queue[i].cycle` (the typed source).
                    // A milestone whose queue item has no `cycle`
                    // has not been attempted, so it is on cycle 1 —
                    // which `QueueRow::cycle` already encodes.
                    cycle: row.cycle,
                    active: row.active,
                })
                .collect();
        }
    }
    app.autopilot
        .setup
        .selected
        .iter()
        .map(|id| TakeoverRow {
            id: id.clone(),
            lifecycle: "in-progress".to_string(),
            // No queue view: the run has not been refreshed yet, so
            // there is no typed cycle to read. Cycle 1 is the honest
            // reading here — the milestone has not been attempted.
            cycle: super::queue_row_default_cycle(),
            active: false,
        })
        .collect()
}

// ======================================================================
// Geometry — one derivation shared by the renderer and the hit test
// ======================================================================//
// The renderer draws from these rects and the mouse handler hit-tests
// against the same values, so a chip can never be drawn in one place
// and clickable in another. Kept free of any ratatui widget types so
// the hit-test side can use it without pulling in the render tree.

/// The seven stacked bands of the setup region, in render order. The
/// first six are the spec's "six stacked sections"; `ControlRow` is the
/// always-present action strip under Start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupSection {
    Topology,
    Harness,
    Milestones,
    Commit,
    RunMode,
    Start,
    ControlRow,
}

/// The setup sections in render order.
pub const SETUP_SECTIONS: [SetupSection; 7] = [
    SetupSection::Topology,
    SetupSection::Harness,
    SetupSection::Milestones,
    SetupSection::Commit,
    SetupSection::RunMode,
    SetupSection::Start,
    SetupSection::ControlRow,
];

impl SetupSection {
    /// The section's heading, as rendered.
    pub fn title(self) -> &'static str {
        match self {
            Self::Topology => "Topology",
            Self::Harness => "Harness",
            Self::Milestones => "Milestones",
            Self::Commit => "Commit",
            Self::RunMode => "Run mode",
            Self::Start => "Start",
            Self::ControlRow => "Controls",
        }
    }

    /// The one-indexed position used in the golden's section order.
    pub fn ordinal(self) -> usize {
        SETUP_SECTIONS
            .iter()
            .position(|s| *s == self)
            .map(|i| i + 1)
            .unwrap_or(0)
    }
}

/// One interactive chip. `rect` is what the mouse handler hit-tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipArea {
    pub section: SetupSection,
    /// Stable identity for the chip within its section, e.g.
    /// `topology:two-agent` or `harness:runner:cursor`.
    pub id: String,
    pub label: String,
    pub selected: bool,
    pub rect: Rect,
}

/// One sidebar tab header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabArea {
    pub tab: SidebarTab,
    pub label: String,
    pub active: bool,
    pub rect: Rect,
}

/// One control-row button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlAction {
    Pause,
    Stop,
    Resume,
    Back,
}

impl ControlAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pause => "Pause",
            Self::Stop => "Stop",
            Self::Resume => "Resume",
            Self::Back => "Back",
        }
    }

    pub fn all() -> [Self; 4] {
        [Self::Pause, Self::Stop, Self::Resume, Self::Back]
    }
}

/// The control row is always drawn; every button is dim and inert when
/// no run is live, so the renderer's only job is to reflect
/// `run_live`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlRowAreas {
    pub buttons: Vec<(ControlAction, Rect)>,
    pub rect: Rect,
}

/// Everything the Autopilot lane draws, derived from the lane state and
/// the full-frame rect. Both `render_autopilot_lane` and
/// `compute_view`'s hit-test path call [`regions`] so the two can never
/// disagree.
#[derive(Debug, Clone, PartialEq)]
pub struct AutopilotRegions {
    /// The lane's full width. The split percentage is relative to
    /// this, not to the setup column: dividing by the *current* setup
    /// width would make the drag accelerate as the column narrows, so
    /// a pointer held still would keep pushing the border left.
    pub lane_width: u16,
    /// The setup form's column (left). Equals `area` when the sidebar
    /// is collapsed.
    pub setup: Rect,
    /// The sidebar's column (right). `None` when collapsed.
    pub sidebar: Option<Rect>,
    /// The draggable split border column, as an absolute `x`. `None`
    /// when there is nothing to drag — collapsed sidebar, a live run,
    /// or a lane too narrow to split.
    pub split_x: Option<u16>,
    /// The seven setup bands, keyed by section. Always all seven so a
    /// caller can index without a fallback.
    pub sections: Vec<(SetupSection, Rect)>,
    /// Every chip, in render order.
    pub chips: Vec<ChipArea>,
    /// The visible sidebar tabs, in render order.
    pub tabs: Vec<TabArea>,
    /// The sidebar body (below the tab headers).
    pub sidebar_body: Rect,
    /// The control row under Start.
    pub control_row: ControlRowAreas,
    /// One rect per takeover milestone row. Empty unless the takeover
    /// is on screen — the rows and the setup chips never coexist.
    pub takeover_rows: Vec<TakeoverRowArea>,
    /// The detached-mode popover's rect. `None` unless the popover is
    /// open. Lives here (rather than being recomputed by the mouse
    /// handler) so the box the operator clicks is the box the renderer
    /// drew.
    pub detached_popover: Option<Rect>,
}

/// One queued-milestone row on the takeover. Clicking it (or pressing
/// Enter on it) opens the read-only peek.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakeoverRowArea {
    pub milestone_id: String,
    pub rect: Rect,
}

impl AutopilotRegions {
    /// The chips belonging to one section, in render order.
    pub fn chips_in(&self, section: SetupSection) -> Vec<&ChipArea> {
        self.chips.iter().filter(|c| c.section == section).collect()
    }

    /// The rect for a chip, if it is on screen.
    pub fn chip_rect(&self, id: &str) -> Option<Rect> {
        self.chips.iter().find(|c| c.id == id).map(|c| c.rect)
    }

    /// The rect for a sidebar tab, if it is currently offered.
    pub fn tab_rect(&self, tab: SidebarTab) -> Option<Rect> {
        self.tabs.iter().find(|t| t.tab == tab).map(|t| t.rect)
    }
}

/// Rows the Milestones band keeps for itself (one content row plus its
/// two borders). Whatever is left over after the fixed bands go also
/// lands here — it is the only elastic section.
const MILESTONES_MIN_HEIGHT: u16 = 3;

/// The fixed height of each band, in rows, including both borders.
/// Derived from what the section has to show rather than fixed at
/// authoring time, so widening the topology (which adds a harness row)
/// grows the band instead of silently clipping it.
fn fixed_section_heights(setup: &SetupForm) -> [(SetupSection, u16); 7] {
    let roles = roles_for_topology(&setup.topology).len() as u16;
    [
        (SetupSection::Topology, 3),
        // One row per live role, plus borders.
        (SetupSection::Harness, roles.saturating_add(2)),
        (SetupSection::Milestones, MILESTONES_MIN_HEIGHT),
        (SetupSection::Commit, 3),
        (SetupSection::RunMode, 3),
        // The Start button and the one-line summary are two rows.
        (SetupSection::Start, 4),
        (SetupSection::ControlRow, 3),
    ]
}

/// Derive every rect the lane needs from its state and the full lane
/// area.
///
/// `takeover_active` and `takeover_rows` are separate because they are
/// separate facts: the takeover is on screen whenever a run is live and
/// has not been dismissed, and its rows band is empty when nothing is
/// queued. Collapsing them would make a live-but-empty takeover render
/// as the split.
///
/// A live takeover also gates the split border — there is no split to
/// drag while the takeover owns the screen.
///
/// `candidates` is the picker's drivable-milestone list. The
/// Milestones section renders every candidate as a chip (selected ones
/// marked) rather than only the selection, because the operator has to
/// be able to see and click what they have not picked yet.
pub fn regions(
    area: Rect,
    layout: &AutopilotLayout,
    setup: &SetupForm,
    candidates: &[String],
    takeover_active: bool,
    takeover_rows: &[TakeoverRow],
    popover_open: bool,
) -> AutopilotRegions {
    let sidebar_visible = layout.sidebar_visible && area.width >= 8;
    let left_w = if sidebar_visible {
        layout.left_width(area.width)
    } else {
        area.width
    };
    let setup_rect = Rect {
        x: area.x,
        y: area.y,
        width: left_w,
        height: area.height,
    };
    let sidebar = sidebar_visible.then(|| Rect {
        x: area.x + left_w,
        y: area.y,
        // The border column belongs to the sidebar so the setup region
        // never draws under the `│` glyph.
        width: area.width.saturating_sub(left_w),
        height: area.height,
    });
    // One column is the border itself. No hit target while the
    // takeover owns the screen — there is no split to drag.
    let split_x = sidebar.filter(|_| !takeover_active).map(|s| s.x);

    // --- setup bands -------------------------------------------------
    // Fixed bands first, then whatever is left goes to Milestones so
    // the selection list gets the terminal's spare rows instead of
    // the column ending in dead space.
    let mut heights: Vec<(SetupSection, u16)> = fixed_section_heights(setup)
        .into_iter()
        .filter(|(s, _)| *s != SetupSection::Milestones)
        .collect();
    let fixed_total: u16 = heights.iter().map(|(_, h)| *h).sum();
    let milestones_height = setup_rect
        .height
        .saturating_sub(fixed_total)
        .max(MILESTONES_MIN_HEIGHT.min(setup_rect.height));
    heights.push((SetupSection::Milestones, milestones_height));
    // Re-sort into render order (the push above appended Milestones last).
    heights.sort_by_key(|(s, _)| s.ordinal());

    let mut y = setup_rect.y;
    let mut remaining = setup_rect.height;
    let mut sections: Vec<(SetupSection, Rect)> = Vec::with_capacity(7);
    for (section, rows) in heights {
        if remaining == 0 {
            // Squeeze mode: every band still gets a rect so callers
            // never have to handle a missing section, but later bands
            // are zero-height and render nothing.
            sections.push((section, Rect::new(setup_rect.x, y, setup_rect.width, 0)));
            continue;
        }
        // Never let a fixed band eat the whole column on a short
        // terminal: leave at least one row for each later band.
        let later = SETUP_SECTIONS.len() as u16 - section.ordinal() as u16;
        let ceiling = remaining.saturating_sub(later);
        let h = rows.min(ceiling);
        sections.push((section, Rect::new(setup_rect.x, y, setup_rect.width, h)));
        y += h;
        remaining = remaining.saturating_sub(h);
    }

    // --- chips -------------------------------------------------------
    let mut chips: Vec<ChipArea> = Vec::new();
    let mut push_row = |section: SetupSection, entries: Vec<(String, String, bool)>| {
        let rect = sections
            .iter()
            .find(|(s, _)| *s == section)
            .map(|(_, r)| *r)
            .unwrap_or_default();
        // The header line is the block border; chips live inside.
        if rect.height < 3 {
            return;
        }
        let inner_y = rect.y + 1;
        let mut x = rect.x + 1;
        for (id, label, selected) in entries {
            let w = label.chars().count() as u16 + 2;
            if x + w > rect.x + rect.width {
                break; // row full
            }
            chips.push(ChipArea {
                section,
                id,
                label,
                selected,
                rect: Rect::new(x, inner_y, w, 1),
            });
            x += w + 1;
        }
    };

    push_row(
        SetupSection::Topology,
        super::ALLOWED_TOPOLOGIES
            .iter()
            .map(|t| {
                (
                    format!("topology:{t}"),
                    (*t).to_string(),
                    setup.topology == *t,
                )
            })
            .collect(),
    );
    push_row(
        SetupSection::Harness,
        vec![(
            "harness:uniform".to_string(),
            "uniform".to_string(),
            setup.harness_uniform,
        )],
    );
    for role in roles_for_topology(&setup.topology) {
        let current = setup.harness_for(role);
        push_row(
            SetupSection::Harness,
            HARNESS_CHOICES
                .iter()
                .map(|h| {
                    (
                        format!("harness:{role}:{h}"),
                        (*h).to_string(),
                        current == *h,
                    )
                })
                .collect(),
        );
    }
    push_row(
        SetupSection::Milestones,
        candidates
            .iter()
            .map(|id| {
                (
                    format!("milestone:{id}"),
                    id.clone(),
                    setup.selected.iter().any(|s| s == id),
                )
            })
            .collect(),
    );
    push_row(
        SetupSection::Commit,
        vec![
            (
                "commit:commit_after_execute".to_string(),
                "commit".to_string(),
                setup.commit_after_execute,
            ),
            (
                "commit:push_after_review".to_string(),
                "push".to_string(),
                setup.push_after_review,
            ),
        ],
    );
    push_row(
        SetupSection::RunMode,
        vec![
            (
                "run_mode:normal".to_string(),
                "normal".to_string(),
                setup.run_mode == RunMode::Normal,
            ),
            (
                "run_mode:detached".to_string(),
                "detached".to_string(),
                setup.run_mode == RunMode::Detached,
            ),
        ],
    );
    let start_rect = sections
        .iter()
        .find(|(s, _)| *s == SetupSection::Start)
        .map(|(_, r)| *r)
        .unwrap_or_default();
    if start_rect.height >= 3 {
        chips.push(ChipArea {
            section: SetupSection::Start,
            id: "start".to_string(),
            label: "Start".to_string(),
            selected: true,
            rect: Rect::new(start_rect.x + 1, start_rect.y + 1, 6, 1),
        });
    }

    // --- control row -------------------------------------------------
    let control_rect = sections
        .iter()
        .find(|(s, _)| *s == SetupSection::ControlRow)
        .map(|(_, r)| *r)
        .unwrap_or_default();
    let mut buttons = Vec::new();
    if control_rect.height >= 3 {
        let mut x = control_rect.x + 1;
        for action in ControlAction::all() {
            let w = action.label().len() as u16 + 2;
            if x + w > control_rect.x + control_rect.width {
                break;
            }
            buttons.push((action, Rect::new(x, control_rect.y + 1, w, 1)));
            x += w + 1;
        }
    }
    let control_row = ControlRowAreas {
        buttons,
        rect: control_rect,
    };

    // --- sidebar tabs ------------------------------------------------
    let mut tabs = Vec::new();
    let mut sidebar_body = Rect::default();
    if let Some(sb) = sidebar {
        let mut tx = sb.x + 1;
        for tab in layout.selectable_tabs() {
            let label = tab.label();
            let w = label.chars().count() as u16 + 2;
            if tx + w > sb.x + sb.width {
                break;
            }
            tabs.push(TabArea {
                tab,
                label: label.to_string(),
                active: layout.sidebar_tab == tab,
                rect: Rect::new(tx, sb.y, w, 1),
            });
            tx += w;
        }
        sidebar_body = Rect::new(sb.x, sb.y + 1, sb.width, sb.height.saturating_sub(1));
    }

    // --- detached popover ----------------------------------------------
    // Centred over the setup region, and only present when the popover
    // is open. The renderer draws it and the mouse handler hit-tests
    // the same rect.
    let detached_popover = popover_open.then(|| detached_popover_rect(setup_rect));

    // --- takeover rows ------------------------------------------------
    // Five stacked bands: topology strip, queued rows, activity tail,
    // telemetry strip, control row. The rows band is elastic and the
    // rest are fixed.
    let mut takeover_areas: Vec<TakeoverRowArea> = Vec::new();
    if takeover_active {
        const TOPOLOGY_H: u16 = 3;
        const TELEMETRY_H: u16 = 1;
        const CONTROL_H: u16 = 3;
        let fixed = TOPOLOGY_H + TELEMETRY_H + CONTROL_H;
        let rows_height = area.height.saturating_sub(fixed);
        let rows_top = area.y + TOPOLOGY_H + TELEMETRY_H;
        // One row inside the band's borders.
        for (i, row) in takeover_rows.iter().enumerate() {
            if i as u16 + 2 > rows_height {
                break; // band full — the rest stay unclickable rather
                       // than being mapped onto rows that are not drawn
            }
            takeover_areas.push(TakeoverRowArea {
                milestone_id: row.id.clone(),
                rect: Rect::new(
                    area.x + 1,
                    rows_top + 1 + i as u16,
                    area.width.saturating_sub(2),
                    1,
                ),
            });
        }
    }

    AutopilotRegions {
        lane_width: area.width,
        setup: setup_rect,
        sidebar,
        split_x,
        sections,
        chips,
        tabs,
        sidebar_body,
        control_row,
        takeover_rows: takeover_areas,
        detached_popover: detached_popover
            .is_some()
            .then(|| detached_popover.unwrap()),
    }
}

/// The detached popover's rect, centred over the setup region. One row
/// per choice plus the two borders.
pub fn detached_popover_rect(area: Rect) -> Rect {
    let width = 40.min(area.width);
    let height = (DETACHED_CHOICES.len() as u16 + 2).min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// The takeover's five band rects, in render order. Shared by the
/// renderer and the hit test for the same reason `regions` is.
pub fn takeover_bands(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    const TOPOLOGY_H: u16 = 3;
    const TELEMETRY_H: u16 = 1;
    const CONTROL_H: u16 = 3;

    // Every band is clamped to the space left below it, so a terminal
    // too short to hold the takeover gets truncated bands rather than
    // rects that run past the bottom of the lane.
    let bottom = area.y.saturating_add(area.height);
    let clamp = |y: u16, want: u16| -> Rect {
        let y = y.min(bottom);
        Rect::new(area.x, y, area.width, want.min(bottom.saturating_sub(y)))
    };

    let topology = clamp(area.y, TOPOLOGY_H);
    let telemetry = clamp(topology.y.saturating_add(topology.height), TELEMETRY_H);
    // The control row is pinned to the bottom so pause / stop stay
    // reachable however short the terminal is.
    let control = clamp(bottom.saturating_sub(CONTROL_H.min(area.height)), CONTROL_H);
    // The rows band takes whatever is between telemetry and control.
    let rows_y = telemetry.y.saturating_add(telemetry.height);
    let rows = Rect::new(area.x, rows_y, area.width, control.y.saturating_sub(rows_y));
    // The activity tail takes the last row of the rows band, so it
    // fills whatever the queue did not.
    let activity = Rect::new(
        area.x,
        rows.y.saturating_add(rows.height).saturating_sub(1),
        area.width,
        if rows.height == 0 { 0 } else { 1 },
    );
    (topology, telemetry, rows, activity, control)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_form_summary_singular_and_plural() {
        let mut form = SetupForm::new();
        form.selected = vec!["M1".into()];
        assert!(form.summary().starts_with("1 milestone · "));
        form.selected.push("M2".into());
        assert!(form.summary().starts_with("2 milestones · "));
    }

    #[test]
    fn split_pct_clamps_at_both_ends() {
        assert_eq!(AutopilotLayout::clamp_split_pct(0), SPLIT_PCT_MIN);
        assert_eq!(AutopilotLayout::clamp_split_pct(24), SPLIT_PCT_MIN);
        assert_eq!(AutopilotLayout::clamp_split_pct(25), 25);
        assert_eq!(AutopilotLayout::clamp_split_pct(40), 40);
        assert_eq!(AutopilotLayout::clamp_split_pct(75), 75);
        assert_eq!(AutopilotLayout::clamp_split_pct(76), SPLIT_PCT_MAX);
        assert_eq!(AutopilotLayout::clamp_split_pct(1000), SPLIT_PCT_MAX);
    }

    #[test]
    fn left_width_never_starves_the_sidebar() {
        let mut layout = AutopilotLayout::new();
        layout.set_split_pct(75);
        assert_eq!(layout.left_width(100), 75);
        layout.set_split_pct(25);
        assert_eq!(layout.left_width(100), 25);
        // Tiny terminals: too narrow to split, so the setup region
        // keeps the whole lane (see `regions`, which also drops the
        // sidebar below 8 columns).
        assert_eq!(layout.left_width(2), 2);
        assert_eq!(layout.left_width(1), 1);
        // Collapsed sidebar: the setup region takes the whole lane.
        layout.toggle_sidebar();
        assert_eq!(layout.left_width(100), 100);
    }

    #[test]
    fn cycle_tab_wraps_and_skips_hidden_state() {
        let mut layout = AutopilotLayout::new();
        layout.cycle_tab(1);
        assert_eq!(layout.sidebar_tab, SidebarTab::Activity);
        layout.cycle_tab(1);
        assert_eq!(layout.sidebar_tab, SidebarTab::State);
        layout.cycle_tab(1);
        assert_eq!(layout.sidebar_tab, SidebarTab::Progress);

        // Back from the takeover withholds State: the cycle now has
        // two stops and wraps between them.
        layout.state_tab_hidden = true;
        layout.sidebar_tab = SidebarTab::Activity;
        layout.cycle_tab(1);
        assert_eq!(layout.sidebar_tab, SidebarTab::Progress);
        layout.cycle_tab(1);
        assert_eq!(layout.sidebar_tab, SidebarTab::Activity);
    }

    #[test]
    fn cycle_tab_recovers_from_a_hidden_current_tab() {
        let mut layout = AutopilotLayout::new();
        layout.sidebar_tab = SidebarTab::State;
        layout.state_tab_hidden = true;
        layout.cycle_tab(1);
        // State is unreachable, so we land on the first live tab
        // rather than being stuck.
        assert_ne!(layout.sidebar_tab, SidebarTab::State);
    }

    #[test]
    fn topology_id_is_zero_padded_and_offset_by_one() {
        assert_eq!(topology_id("three-agent", 0), "three-agent-001");
        assert_eq!(topology_id("three-agent", 3), "three-agent-004");
        assert_eq!(topology_id("one-agent", 41), "one-agent-042");
        assert_eq!(topology_id("two-agent", 999), "two-agent-1000");
    }

    #[test]
    fn harness_uniform_applies_to_every_live_role() {
        let mut form = SetupForm::new();
        form.set_topology("two-agent");
        form.harness_uniform = true;
        form.set_harness("runner", "cursor");
        assert_eq!(form.harness_for("orchestrator"), "cursor");
        assert_eq!(form.harness_for("runner"), "cursor");

        // Narrowing does not drop the stored value, so widening back
        // restores it.
        form.set_topology("one-agent");
        assert_eq!(form.harness_for("orchestrator"), "cursor");
        form.set_topology("two-agent");
        assert_eq!(form.harness_for("runner"), "cursor");
    }

    #[test]
    fn harness_per_role_targets_one_row() {
        let mut form = SetupForm::new();
        form.harness_uniform = false;
        form.set_harness("reviewer", "pi");
        assert_eq!(form.harness_for("reviewer"), "pi");
        assert_eq!(form.harness_for("runner"), "inherit");
    }

    #[test]
    fn detached_popover_cycles_all_three_choices() {
        let mut pop = DetachedConfirm::new();
        assert_eq!(pop.current(), DetachedChoice::Confirm);
        pop.move_cursor(1);
        assert_eq!(pop.current(), DetachedChoice::ConfigureExtras);
        pop.move_cursor(1);
        assert_eq!(pop.current(), DetachedChoice::Back);
        pop.move_cursor(1);
        assert_eq!(pop.current(), DetachedChoice::Confirm);
        pop.move_cursor(-1);
        assert_eq!(pop.current(), DetachedChoice::Back);
    }

    #[test]
    fn ui_prefs_fall_back_on_garbage() {
        let prefs = UiPrefs::from_pairs([
            ("ui.autopilot.split_pct", "not-a-number"),
            ("ui.autopilot.sidebar_tab", "nonsense"),
            ("ui.autopilot.sidebar_visible", "banana"),
        ]);
        assert_eq!(prefs.split_pct, SPLIT_PCT_DEFAULT);
        assert_eq!(prefs.sidebar_tab, SidebarTab::Progress);
        // "banana" is not a true word, so visibility reads false rather
        // than defaulting back to true — a bad value must not widen the
        // layout behind the operator's back.
        assert!(!prefs.sidebar_visible);
    }

    #[test]
    fn ui_prefs_reads_real_values() {
        let prefs = UiPrefs::from_pairs([
            ("ui.autopilot.split_pct", "62"),
            ("ui.autopilot.sidebar_tab", "activity"),
            ("ui.autopilot.sidebar_visible", "false"),
        ]);
        assert_eq!(prefs.split_pct, 62);
        assert_eq!(prefs.sidebar_tab, SidebarTab::Activity);
        assert!(!prefs.sidebar_visible);
    }
}
