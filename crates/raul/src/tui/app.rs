use serde_json::Value;
use std::cell::Cell;

use ratatui::text::Line;

use super::keybinds::Keybinds;
use super::lane_cache::LaneCache;
use super::markdown::{self, MarkdownStyles};
use super::mode::{InputState, Mode, ReviewMenuState, SettingsFocus, SettingsState};
use crate::theme::Palette;

/// M163 AC-03: result of a `mp plan verify-ac <id>` pre-flight call made
/// when the review menu opens. Parsed from the JSON output; stored on `App`
/// so the renderer can grey out the "Approve milestone" item when the gate
/// is closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightGate {
    /// `true` when all AC verification statuses pass the approval gate.
    pub open: bool,
    /// Number of ACs with unresolvable verification.
    pub unresolvable_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Lane {
    Overview,
    Milestones,
    Path,
    Backlog,
    /// M184: Ideas = exploratory backlog rows (`ID-*` only).
    /// Actionable backlog (`TW-*` / `BF-*` / `BL-*`) lives on
    /// [`Lane::Backlog`]; the former Tweaks lane was folded in.
    Ideas,
    /// M179: dedicated `mp watch` workflow — milestone picker,
    /// preflight, start, lifecycle graph, queue, log + agent
    /// output, attach/stop/detach. Pinned immediately before
    /// `Settings` so it stays adjacent to the most workflow-y
    /// control surface.
    Watch,
    /// M164: Settings lane (M169 real lane, not modal).
    Settings,
}

/// M172 S5: sort key for the per-lane sort rebind menu.
///
/// `Id` is the legacy alphabetical order; `Lifecycle` groups
/// milestones by their canonical `lifecycle` value (in-progress →
/// approved → done, etc.); `Priority` orders by backlog priority
/// (or "—" when the row has no priority); `Updated` orders by the
/// `lifecycle_at` timestamp (most-recent first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Id,
    Lifecycle,
    Priority,
    Updated,
    /// Backlog-shaped equivalent of `Lifecycle` — sorts by the `status`
    /// field (`open` > `pending` > `active` > `resolved` > `done` >
    /// `archived` …). Only valid on `Lane::Backlog` / `Ideas`.
    Status,
    /// Alphabetical sort by title. Cross-lane (works on both milestone
    /// and backlog rows). Cycle-sort sequence visits this after Id to
    /// match the visible column order.
    Title,
}

impl SortKey {
    pub fn label(&self) -> &'static str {
        match self {
            SortKey::Id => "id",
            SortKey::Lifecycle => "lifecycle",
            SortKey::Priority => "priority",
            SortKey::Updated => "updated",
            SortKey::Status => "status",
            SortKey::Title => "title",
        }
    }
}

/// Which sort keys apply to a given lane, in visible column order.
/// `o` cycle-sort walks this list top-to-bottom so the user can predict
/// the next stop from the column layout.
///
/// - `Milestones` — `Id → Title → Priority → Lifecycle → Updated → Id`
///   (5 stops; Gauge and Since are visualizations of Lifecycle /
///   Updated respectively, so cycle skips them as redundant stops).
/// - `Backlog` / `Ideas` — `Id → Title → Priority → Status → Id`
///   (4 stops; matches the visible column order).
/// - `Path` / `Overview` / `Settings` / `Watch` — no sort menu (no
///   list to sort, or the sort surface is custom — Watch has its own
///   selection ordering by milestone id).
pub fn sort_keys_for(lane: Lane) -> Vec<SortKey> {
    match lane {
        Lane::Milestones => vec![
            SortKey::Id,
            SortKey::Title,
            SortKey::Priority,
            SortKey::Lifecycle,
            SortKey::Updated,
        ],
        Lane::Backlog | Lane::Ideas => vec![
            SortKey::Id,
            SortKey::Title,
            SortKey::Priority,
            SortKey::Status,
        ],
        // Path / Overview / Settings / Watch: no sort menu.
        Lane::Path | Lane::Overview | Lane::Settings | Lane::Watch => Vec::new(),
    }
}

/// M182 S3: priority rank for `SortKey::Priority`. Duplicates the
/// `path_prefs::priority_rank` table so raul doesn't need a cross-
/// crate dep for a 5-line lookup. Higher rank sorts first under
/// ascending order; unknown values default to "normal" rank (2) so
/// a malformed priority doesn't sink the row to the bottom.
fn priority_rank(priority: &str) -> u8 {
    match priority {
        "urgent" => 4,
        "high" => 3,
        "normal" => 2,
        "low" => 1,
        _ => 2,
    }
}

/// M182 S3: lifecycle rank for `SortKey::Lifecycle`. Higher rank
/// sorts first. Documented order (F-07 / F-12):
/// in-progress > approved > groomed > done > self-reviewed >
/// reviewed > complete > cancelled > remediation > draft.
///
/// `complete` and `cancelled` are distinct ranks (complete above
/// cancelled). `remediation` sits below both terminal states so the
/// sort matches the comment on `visible_milestones`. Unknown values
/// and `draft` default to 0 (lowest).
fn lifecycle_rank(lifecycle: &str) -> u8 {
    match lifecycle {
        "in-progress" => 9,
        "approved" => 8,
        "groomed" => 7,
        "done" => 6,
        "self-reviewed" => 5,
        "reviewed" => 4,
        "complete" => 3,
        "cancelled" => 2,
        "remediation" => 1,
        _ => 0,
    }
}

/// Status rank for `SortKey::Status` (Backlog/Ideas). Higher rank
/// sorts first. Open / pending / active items surface above terminal
/// (resolved / done / archived / dismissed / closed / cancelled) ones;
/// unknown values default to 0 so malformed statuses sink rather than
/// shadow a real row. Mirrors `lifecycle_rank`'s shape.
fn status_rank(status: &str) -> u8 {
    match status {
        "open" => 9,
        "pending" => 8,
        "active" => 7,
        "in-progress" => 6,
        "resolved" => 3,
        "done" => 2,
        "archived" | "dismissed" | "closed" | "cancelled" => 1,
        _ => 0,
    }
}

/// M182 S3: numeric comparator for milestone IDs. Strips any
/// leading non-digit prefix (typically "M") so "M01" parses to 1, then
/// splits each id on `.` and parses each segment as u32. Falls back
/// to lexicographic compare on segments that don't parse. This mirrors
/// `mp::paths::compare_milestone_ids` but is duplicated here so
/// raul doesn't need a cross-crate dep for a comparator that's
/// load-bearing under every sort key's tie-breaker.
fn compare_milestone_ids(a: &str, b: &str) -> std::cmp::Ordering {
    let strip_prefix = |s: &str| -> String {
        let trimmed: String = s
            .trim_start_matches(|c: char| !c.is_ascii_digit() && c != '.')
            .to_string();
        if trimmed.is_empty() {
            s.to_string()
        } else {
            trimmed
        }
    };
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|seg| seg.parse::<u64>().unwrap_or(u64::MAX))
            .collect()
    };
    let va = parse(&strip_prefix(a));
    let vb = parse(&strip_prefix(b));
    for (xa, xb) in va.iter().zip(vb.iter()) {
        match xa.cmp(xb) {
            std::cmp::Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    va.len().cmp(&vb.len())
}

impl Lane {
    /// M184: exactly 7 lanes — Tweaks folded into Backlog; Grooming
    /// tab removed from the ordered surface.
    pub fn ordered() -> Vec<Lane> {
        vec![
            Lane::Overview,
            Lane::Milestones,
            Lane::Path,
            Lane::Backlog,
            Lane::Ideas,
            Lane::Watch,
            Lane::Settings,
        ]
    }

    pub fn label(&self) -> &'static str {
        crate::lanes::lane_label(self)
    }

    pub fn compact_label(&self) -> &'static str {
        match self {
            Lane::Overview => "Ov",
            Lane::Milestones => "Ml",
            Lane::Path => "Ph",
            Lane::Backlog => "Bl",
            Lane::Ideas => "Id",
            Lane::Watch => "Wt",
            Lane::Settings => "Set",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DetailMarkdownCache {
    pub milestone_id: String,
    pub intent: Vec<Line<'static>>,
    pub problem: Vec<Line<'static>>,
}

/// Milestone-detail measurement cache keyed by content hash and panel width.
/// Interior mutability is limited to derived render state; content or width
/// changes naturally invalidate the cached measurement.
#[derive(Debug, Clone, Copy)]
pub struct DetailMeasurementCache {
    pub content_hash: u64,
    pub area_width: u16,
    pub max_scroll: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContentState {
    List,
    MilestoneDetail,
    BacklogDetail,
    AnnotationThread,
    CoApproval,
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionCounts {
    pub total: u64,
    pub done: u64,
    pub planned: u64,
    pub in_progress: u64,
    pub blocked: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SpecCounts {
    pub ready: u64,
    pub review: u64,
    pub verified: u64,
}

/// M146: canonical per-lifecycle-stage counts. The Plan overview
/// block renders the buckets in lifecycle order rather than the
/// pre-M146 legacy spec/exec split so the dashboard reads in the same
/// vocabulary as the Milestone list (`lifecycle` column) and detail
/// (single `lifecycle` badge). `SpecCounts` + `ExecutionCounts`
/// remain around for backcompat; new code should read `LifecycleCounts`.
#[derive(Debug, Clone, Default)]
pub struct LifecycleCounts {
    pub total: u64,
    pub draft: u64,
    pub groomed: u64,
    pub approved: u64,
    pub in_progress: u64,
    pub done: u64,
    pub self_reviewed: u64,
    pub reviewed: u64,
    pub complete: u64,
    pub remediation: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DashboardSnapshot {
    pub planning_status: String,
    pub execution_mode: String,
    pub inbox_count: u64,
    pub pending_review_count: u64,
    pub track_pending: u64,
    pub annotations_open: u64,
    pub next_action: String,
    pub path_preview: Vec<String>,
    pub execution_counts: ExecutionCounts,
    pub spec_counts: SpecCounts,
    /// M146: canonical lifecycle bucket counts from `mp status
    /// by_lifecycle`. Drives the Plan-overview line shown to humans.
    pub lifecycle_counts: LifecycleCounts,
    pub blockers: Vec<BlockerLine>,
    pub inbox_items: Vec<InboxLine>,
}

#[derive(Debug, Clone)]
pub struct BlockerLine {
    pub milestone: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct InboxLine {
    pub id: String,
    pub kind: String,
    pub display: String,
    pub reason: String,
    pub action: String,
}

#[derive(Debug, Clone)]
pub struct BacklogLine {
    pub id: String,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub resolution: String,
}

#[derive(Debug, Clone, Default)]
pub struct MilestoneSummary {
    pub id: String,
    pub title: String,
    /// M144: canonical M100 lifecycle value (`draft`, `groomed`, `approved`,
    /// `in-progress`, `done`, `self-reviewed`, `reviewed`, `complete`,
    /// `remediation`). Replaces the legacy `spec_status` + `execution_status`
    /// pair as the source of truth for the TUI Milestones lane.
    pub lifecycle: String,
    /// M144: RFC3339 timestamp of the last lifecycle transition. The TUI
    /// renders a relative-time "since" string (`3d ago`) next to the
    /// lifecycle column when this is `Some`; falls back to a generic
    /// "since updated" placeholder when `None`.
    pub lifecycle_at: Option<String>,
    /// M172 S2: `depends_on` carries the primary-dependency edges
    /// parsed from `mp list milestones` output (the same shape that
    /// `parse_milestone_summaries` builds). Empty for milestones with
    /// no edges. Used by `milestone_tree::build_tree` to construct
    /// the hierarchical tree view in the Milestones lane.
    pub depends_on: Vec<String>,
    /// M182 S2: priority from `mp list milestones` (`urgent` /
    /// `high` / `normal` / `low`). Used by the sort-rebind menu's
    /// "priority" sort option. Defaults to `"normal"` for
    /// pre-M122 milestones (the field was optional until M122).
    pub priority: String,
    /// M182 S2: last-touch date (YYYY-MM-DD) from `mp list
    /// milestones`. Used by the sort-rebind menu's "updated" sort
    /// option. Empty string means "unknown" — sinks to the bottom
    /// under ascending order.
    pub updated: String,
}

impl MilestoneSummary {
    /// Convenience constructor for tests + sample fixtures.
    /// `depends_on` defaults to an empty Vec so test code doesn't
    /// have to thread an empty array through every literal.
    /// `priority` defaults to "normal" and `updated` to "" —
    /// the sort-rebind menu's defaults (pre-sort, pre-touch).
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        lifecycle: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            lifecycle: lifecycle.into(),
            lifecycle_at: None,
            depends_on: Vec::new(),
            priority: "normal".to_string(),
            updated: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnnotationInfo {
    pub id: String,
    pub target: String,
    pub kind: String,
    pub status: String,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub resolved_at: String,
}

#[derive(Debug)]
pub struct App {
    pub active_lane: Lane,
    pub content: ContentState,
    pub dashboard: DashboardSnapshot,
    /// M181 S2: typed view of `mp overview` (the M180 consolidated
    /// snapshot). Replaces the legacy `dashboard` for new renderers;
    /// `dashboard` remains populated for backcompat until S3 retires
    /// it.
    pub overview: crate::overview_snapshot::OverviewSnapshot,
    pub backlog: Vec<BacklogLine>,
    pub milestones: Vec<MilestoneSummary>,
    pub selected_index: usize,
    pub selected_milestone_id: Option<String>,
    pub milestone_detail: Option<Value>,
    pub detail_markdown_cache: Option<DetailMarkdownCache>,
    /// M169-rev scrollbar (L3a): see [`DetailMeasurementCache`]. The
    /// measurement pass in `render_milestone_detail` skips the
    /// 8×-panel Buffer allocation + Paragraph::render when this
    /// cache hits (same content hash + same panel width). Wrapped
    /// in `Cell` so the renderer can update it through `&App` (the
    /// `render` and `render_milestone_detail` signatures stay
    /// immutable-borrow; the cache is logically a memoized
    /// computation, not user-visible state).
    pub detail_measurement_cache: Cell<Option<DetailMeasurementCache>>,
    pub annotations: Vec<AnnotationInfo>,
    pub selected_annotation_index: usize,
    pub open_only: bool,
    pub hide_done: bool,
    /// M185: multi-select lifecycle filter for the Milestones lane.
    /// Empty = show all (subject to `hide_done`). Survives lane switches.
    pub milestone_filter: std::collections::BTreeSet<String>,
    /// M186: per-lane substring search against id+title (empty = all).
    /// Survives lane switches; cleared on raul restart.
    pub lane_search: std::collections::HashMap<Lane, String>,
    /// M154: `[review].hunk` flag read from mp config at startup.
    /// When true, the milestone detail view shows the "hunk export:
    /// on (N anchored)" indicator. Loaded once via `UiConfig::load`;
    /// restart raul to pick up mid-session flag changes.
    pub review_hunk_enabled: bool,
    pub quitting: bool,
    /// M172 S5: sort rebind inline menu. `None` = menu closed;
    /// `Some(keys)` = menu open with the listed available keys.
    /// `sort_rebind_index` tracks the highlighted key (cycles
    /// with ArrowUp/Down).
    pub sort_rebind_menu: Option<Vec<SortKey>>,
    pub sort_rebind_index: usize,
    /// Per-lane sort preference. Keys are `Lane::*` variants that
    /// show a list (Milestones / Backlog / side lanes). The map
    /// carries the user's bound `SortKey` per lane; the default
    /// is `SortKey::Id` (legacy alphabetical order). Persistence
    /// to `~/.agents/master-plan/config.json` flows through the
    /// `mp config set sort.<lane> <sortkey>` write wired in M182 S4
    /// (M172 S5 shipped only the in-memory menu; persistence was the
    /// follow-up).
    pub lane_sort_key: std::collections::HashMap<Lane, SortKey>,
    /// M136: the single source of truth for "which UI mode are we in".
    /// Replaces pre-M136's `show_help: bool`, `input_mode: Option<InputMode>`,
    /// `input_buffer: String`, and `show_review_menu: bool`. The
    /// per-mode-state data (input target/kind/buffer, review-menu items +
    /// selected) lives *inside* the matching variant so closing a mode
    /// drops its state by construction.
    pub active_mode: Mode,
    pub approval_blocked: bool,
    pub approval_annotation_id: Option<String>,
    pub co_approval_annotation: Option<AnnotationInfo>,
    pub co_approval_milestone_id: Option<String>,
    pub co_approval_action: Option<CoApprovalAction>,
    pub co_approval_state: CoApprovalState,
    pub path_data: Option<Value>,
    /// Vertical scroll offset for the Path tree (line units). Mirrors
    /// `detail_scroll` for the Path tab so a tall tree is navigable
    /// via j/k, page keys, wheel, and scrollbar track clicks (M157
    /// extra-review AC-05 gap).
    pub path_scroll: u16,
    /// Max `path_scroll` for the last render (lines − viewport height).
    /// Written by the Path renderer via interior mutability so
    /// `render` stays `&App`.
    pub path_max_scroll: Cell<u16>,
    pub detail_scroll: u16,
    pub detail_max_scroll: Cell<u16>,
    /// M167: row offsets (relative to the detail Paragraph's first
    /// rendered row) of each populated section's first item, populated
    /// by `render_milestone_detail` on every render. The detail-nav
    /// actions (`]`, `[`, `n`, `p`) read this snapshot to jump between
    /// sections / items without rescanning the document. Wrapped in
    /// `RefCell` so the renderer can update it via `&App`.
    pub detail_section_rows: std::cell::RefCell<Vec<u16>>,
    pub selected_backlog_id: Option<String>,
    pub backlog_detail: Option<Value>,
    /// Transient user-visible error (e.g. failed review action); shown in footer.
    pub flash_message: Option<String>,
    /// M163: full, untruncated text behind the most recent `flash_message`.
    /// Preserved separately so the `?` help overlay can render a "Details"
    /// section with the original message even when the footer line was
    /// truncated to fit the terminal width. Cleared whenever `flash_message`
    /// is cleared (success path) so the overlay never surfaces stale data.
    pub last_action_error: Option<String>,
    /// M163 AC-03: pre-flight gate result from `mp plan verify-ac`. Set when
    /// the review menu opens; cleared when it closes. Used by the review-menu
    /// renderer to grey out the "Approve milestone" item when the milestone
    /// fails AC verification.
    pub preflight_gate: Option<PreflightGate>,
    /// Active color palette from theme config (ui.theme). Default: MOCHA.
    pub palette: &'static Palette,
    /// Per-lane TTL cache for `mp` subprocess payloads. Reads consult it
    /// before shelling out; successful loads populate it, and mutations
    /// invalidate affected lanes so re-entry observes fresh data.
    pub lane_cache: LaneCache,
    /// M134: monotonically-incrementing state-change counter. Bumped only on
    /// actual mutations (a no-op `move_down` at the end of a list leaves it
    /// alone). The runner loop samples it before/around every dispatched
    /// event and uses the diff to decide whether to set the dirty signal —
    /// keeping the dirty-signal logic purely value-driven so a future
    /// mutator can't forget to flag the change.
    pub version: u64,
    /// M138: the active keybindings. Defaults to raul's built-in bindings
    /// (reproducing the pre-M138 hardcoded set) and is replaced at TUI start
    /// with `Keybinds::load_from_config` so user `[keybinds]` overrides take
    /// effect. The per-mode handlers in `tui::modes` and the help/footer
    /// renderers read from this single source of truth.
    pub keybinds: Keybinds,
    /// M169: populated while `active_lane == Lane::Settings`; cleared on leave.
    pub settings: Option<SettingsState>,
    /// M179: Watch client model — picker + selection + preflight +
    /// M178 status/output snapshots. Single source of truth for
    /// the Watch lane; other modules read but do not write.
    pub watch: crate::tui::watch::Watch,
    /// M179 S7: 2-5s poller state for the Watch lane. The
    /// `run_loop`'s `on_idle` closure calls `poll_watch_state`
    /// whenever the Watch lane is active; the Poller rate-limits
    /// the underlying `mp watch-control status` / `output`
    /// shell-outs.
    pub watch_poller: crate::tui::watch::Poller,
    /// Plan directory used by the Watch poller to refresh its bounded log cache.
    /// Defaults to `.` for tests and callers that do not override it.
    pub plan_dir: std::path::PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoApprovalAction {
    Approve,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoApprovalState {
    Choosing,
    Executing,
    Confirmed,
}

/// Compatibility shape for callers that still construct legacy input state.
/// New code should use `Mode::Input(InputState)`.
#[derive(Debug, Clone)]
pub struct InputMode {
    pub target: String,
    pub kind: String,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            active_lane: Lane::Overview,
            content: ContentState::List,
            dashboard: DashboardSnapshot::default(),
            overview: crate::overview_snapshot::OverviewSnapshot::default(),
            backlog: Vec::new(),
            milestones: Vec::new(),
            selected_index: 0,
            selected_milestone_id: None,
            milestone_detail: None,
            detail_markdown_cache: None,
            detail_measurement_cache: Cell::new(None),
            annotations: Vec::new(),
            selected_annotation_index: 0,
            open_only: false,
            hide_done: false,
            milestone_filter: std::collections::BTreeSet::new(),
            lane_search: std::collections::HashMap::new(),
            review_hunk_enabled: false,
            quitting: false,
            active_mode: Mode::Normal,
            approval_blocked: false,
            approval_annotation_id: None,
            co_approval_annotation: None,
            co_approval_milestone_id: None,
            co_approval_action: None,
            co_approval_state: CoApprovalState::Choosing,
            // M172 S5: sort rebind state. The inline menu's selected
            // sort key (None = menu closed). When `Some`, the menu
            // is open and ArrowUp/Down cycle the index. Enter
            // binds and clears the menu.
            sort_rebind_menu: None,
            sort_rebind_index: 0,
            // Per-lane sort key. The map keys are Lane values; the
            // default is `SortKey::Id` (the legacy order) so
            // behavior is unchanged for callers that never touch
            // the menu.
            lane_sort_key: std::collections::HashMap::new(),
            path_data: None,
            path_scroll: 0,
            path_max_scroll: Cell::new(0),
            detail_scroll: 0,
            detail_max_scroll: Cell::new(0),
            detail_section_rows: std::cell::RefCell::new(Vec::new()),
            selected_backlog_id: None,
            backlog_detail: None,
            flash_message: None,
            last_action_error: None,
            preflight_gate: None,
            palette: Palette::default_palette(),
            lane_cache: LaneCache::with_default_ttl(0),
            version: 0,
            keybinds: Keybinds::default(),
            settings: None,
            watch: crate::tui::watch::Watch::empty(),
            watch_poller: crate::tui::watch::Poller::new(),
            plan_dir: std::path::PathBuf::from("."),
        }
    }

    pub fn set_flash_message(&mut self, message: impl Into<String>) {
        self.flash_message = Some(message.into());
        self.last_action_error = None;
    }

    pub fn set_action_error(&mut self, message: impl Into<String>, details: impl Into<String>) {
        self.flash_message = Some(message.into());
        self.last_action_error = Some(details.into());
    }

    pub fn clear_flash_message(&mut self) {
        self.flash_message = None;
        self.last_action_error = None;
    }

    /// M134: bump the version counter. Called from mutator methods *after* a
    /// state change is committed, so a no-op branch (e.g. `move_down` past
    /// the end of a list) leaves the counter untouched. The runner loop
    /// reads `version()` before/around every event to drive the dirty
    /// signal — a method that mutates without calling this method will
    /// silently fail to redraw.
    ///
    /// `pub(crate)` so direct field-write helpers in `runner.rs`
    /// (`handle_esc`, etc.) can bump the counter when they patch state
    /// outside the typed mutator API. Outside the crate, the canonical
    /// way to mutate `App` is via the typed methods that already call
    /// `touch()` internally.
    pub(crate) fn touch(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// M134: read-only accessor used by the runner loop to capture a snapshot
    /// before dispatching an event. Kept as a method (rather than a public
    /// field read) so future caching or hashing can be added in one place.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Palette used for rendering — monochrome when color output is disabled.
    pub fn effective_palette(&self) -> &'static Palette {
        if crate::config::color_enabled() {
            self.palette
        } else {
            &crate::theme::MONOCHROME
        }
    }

    pub fn load_dashboard(&mut self, snapshot: DashboardSnapshot) {
        self.dashboard = snapshot;
        self.touch();
    }

    /// M181 S2: install the typed `mp overview` snapshot. The legacy
    /// `dashboard` field is also populated from the same payload so
    /// the existing renderer keeps working until S3 retires it.
    pub fn load_overview_snapshot(&mut self, snapshot: crate::overview_snapshot::OverviewSnapshot) {
        self.overview = snapshot.clone();
        self.dashboard = legacy_dashboard_from_overview(&snapshot);
        // M181 S3: clamp selection to the (≤ 5) inbox rows. Activity
        // and counter sections never participate in focus order.
        let n = self.overview.inbox.len();
        if self.selected_index >= n && n > 0 {
            self.selected_index = n - 1;
        }
        self.touch();
    }

    pub fn load_backlog(&mut self, backlog: Vec<BacklogLine>) {
        self.backlog = backlog;
        let n = self.visible_backlog().len();
        if self.selected_index >= n && n > 0 {
            self.selected_index = n - 1;
        }
        self.touch();
    }

    pub fn select_lane(&mut self, lane: Lane) {
        if self.active_lane == Lane::Settings && lane != Lane::Settings {
            self.settings = None;
        }
        // M186 F-02: switching lanes while Mode::SearchInput is active
        // cancels the input (Esc-like). The per-lane buffer is already
        // mirrored into `lane_search` on each keystroke (live-filter),
        // so reopening search on the original lane restores it. The
        // prior-term snapshot lives inside SearchInputState and is
        // dropped here — that's fine because the in-flight draft already
        // committed to lane_search (the only state read by visible_*).
        if matches!(self.active_mode, Mode::SearchInput(_)) {
            self.active_mode = Mode::Normal;
        }
        self.active_lane = lane;
        self.content = ContentState::List;
        self.selected_index = 0;
        self.touch();
    }

    pub fn load_milestones(&mut self, milestones: Vec<MilestoneSummary>) {
        self.milestones = milestones;
        if self.selected_index >= self.milestones.len() && !self.milestones.is_empty() {
            self.selected_index = self.milestones.len() - 1;
        }
        self.touch();
    }

    pub fn load_milestone_detail(&mut self, detail: Value) {
        let ms_id = detail["milestone"]["id"].as_str().unwrap_or("").to_string();
        let styles = MarkdownStyles {
            palette: self.effective_palette(),
        };
        let width = 72;
        let intent = detail["intent"]["outcome"].as_str().unwrap_or("");
        let problem = detail["problem"]["description"].as_str().unwrap_or("");
        self.detail_markdown_cache = Some(DetailMarkdownCache {
            milestone_id: ms_id,
            intent: markdown::parse_markdown(intent, &styles, width),
            problem: markdown::parse_markdown(problem, &styles, width),
        });
        self.milestone_detail = Some(detail);
        self.touch();
    }

    pub fn load_path_data(&mut self, data: Value) {
        self.path_data = Some(data);
        self.path_scroll = 0;
        self.path_max_scroll.set(0);
        self.touch();
    }

    pub fn load_annotations(&mut self, annotations: Vec<AnnotationInfo>) {
        let selected_id = self
            .selected_annotation()
            .map(|annotation| annotation.id.clone());
        self.annotations = annotations;
        self.reanchor_annotation_selection(selected_id.as_deref());
        self.touch();
    }

    pub fn move_up(&mut self) {
        // M136: review-menu selection lives inside `Mode::ReviewMenu(_)`;
        // mutate the variant in place rather than a separate field.
        if let Mode::ReviewMenu(menu) = &mut self.active_mode {
            if menu.selected > 0 {
                menu.selected -= 1;
                self.touch();
            }
            return;
        }
        // M169: settings flat-list navigation on the Settings lane.
        if self.active_lane == Lane::Settings {
            if let Some(state) = self.settings.as_mut() {
                if matches!(state.focus, SettingsFocus::Editing) {
                    return;
                }
                if state.selected_idx > 0 {
                    state.selected_idx -= 1;
                    self.touch();
                }
            }
            return;
        }

        match self.content {
            ContentState::List if self.active_lane == Lane::Path => {
                if self.path_scroll > 0 {
                    self.path_scroll -= 1;
                    self.touch();
                }
            }
            ContentState::List => {
                let count = self.current_list_count();
                if count > 0 && self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.touch();
                }
            }
            ContentState::AnnotationThread => {
                if !self.visible_annotations().is_empty() && self.selected_annotation_index > 0 {
                    self.selected_annotation_index -= 1;
                    self.touch();
                }
            }
            ContentState::MilestoneDetail | ContentState::BacklogDetail
                if self.detail_scroll > 0 =>
            {
                self.detail_scroll -= 1;
                self.touch();
            }
            _ => {}
        }
    }

    /// Default page-jump size for list scrolling. Picked as a sane middle of
    /// typical terminal viewport heights (24-40 rows visible content area).
    pub const PAGE_SIZE: usize = 10;

    /// M91 S7: page-up on a list — moves selected_index back by PAGE_SIZE,
    /// clamping at 0. No-op on non-List content (where Up/Down already do
    /// the right thing for annotations and detail-scroll).
    pub fn move_page_up(&mut self) {
        if let Mode::ReviewMenu(menu) = &mut self.active_mode {
            let step = Self::PAGE_SIZE.min(menu.selected);
            if step > 0 {
                menu.selected -= step;
                self.touch();
            }
            return;
        }
        if self.active_lane == Lane::Settings {
            if let Some(state) = self.settings.as_mut() {
                if matches!(state.focus, SettingsFocus::Editing) {
                    return;
                }
                let before = state.selected_idx;
                let after = before.saturating_sub(Self::PAGE_SIZE);
                if after != before {
                    state.selected_idx = after;
                    self.touch();
                }
            }
            return;
        }
        if self.content != ContentState::List {
            self.move_up();
            return;
        }
        if self.active_lane == Lane::Path {
            let before = self.path_scroll;
            self.path_scroll = self.path_scroll.saturating_sub(Self::PAGE_SIZE as u16);
            if self.path_scroll != before {
                self.touch();
            }
            return;
        }
        let before = self.selected_index;
        self.selected_index = self.selected_index.saturating_sub(Self::PAGE_SIZE);
        if self.selected_index != before {
            self.touch();
        }
    }

    /// M91 S7: page-down on a list — moves selected_index forward by
    /// PAGE_SIZE, clamping at the last index of the current list. No-op on
    /// non-List content.
    pub fn move_page_down(&mut self) {
        if let Mode::ReviewMenu(menu) = &mut self.active_mode {
            let max = menu.items.len().saturating_sub(1);
            let step = Self::PAGE_SIZE.min(max.saturating_sub(menu.selected));
            if step > 0 {
                menu.selected += step;
                self.touch();
            }
            return;
        }
        if self.active_lane == Lane::Settings {
            if let Some(state) = self.settings.as_mut() {
                if matches!(state.focus, SettingsFocus::Editing) {
                    return;
                }
                let max = super::modes::settings::SETTINGS_KEYS
                    .len()
                    .saturating_sub(1);
                let before = state.selected_idx;
                let after = std::cmp::min(before + Self::PAGE_SIZE, max);
                if after != before {
                    state.selected_idx = after;
                    self.touch();
                }
            }
            return;
        }
        if self.content != ContentState::List {
            self.move_down();
            return;
        }
        if self.active_lane == Lane::Path {
            let max = self.path_max_scroll.get();
            let before = self.path_scroll;
            self.path_scroll = self
                .path_scroll
                .saturating_add(Self::PAGE_SIZE as u16)
                .min(max);
            if self.path_scroll != before {
                self.touch();
            }
            return;
        }
        let count = self.current_list_count();
        if count == 0 {
            return;
        }
        let max = count - 1;
        let before = self.selected_index;
        self.selected_index = std::cmp::min(self.selected_index + Self::PAGE_SIZE, max);
        if self.selected_index != before {
            self.touch();
        }
    }

    pub fn move_down(&mut self) {
        // M136: review-menu selection lives inside `Mode::ReviewMenu(_)`;
        // mutate the variant in place rather than a separate field.
        if let Mode::ReviewMenu(menu) = &mut self.active_mode {
            let max = menu.items.len().saturating_sub(1);
            if menu.selected < max {
                menu.selected += 1;
                self.touch();
            }
            return;
        }
        // M169: settings flat-list navigation on the Settings lane.
        if self.active_lane == Lane::Settings {
            if let Some(state) = self.settings.as_mut() {
                if matches!(state.focus, SettingsFocus::Editing) {
                    return;
                }
                let max = super::modes::settings::SETTINGS_KEYS
                    .len()
                    .saturating_sub(1);
                if state.selected_idx < max {
                    state.selected_idx += 1;
                    self.touch();
                }
            }
            return;
        }

        match self.content {
            ContentState::List if self.active_lane == Lane::Path => {
                let max = self.path_max_scroll.get();
                if self.path_scroll < max {
                    self.path_scroll += 1;
                    self.touch();
                }
            }
            ContentState::List => {
                let count = self.current_list_count();
                let max = count.saturating_sub(1);
                if self.selected_index < max {
                    self.selected_index += 1;
                    self.touch();
                }
            }
            ContentState::AnnotationThread => {
                let max = self.visible_annotations().len().saturating_sub(1);
                if self.selected_annotation_index < max {
                    self.selected_annotation_index += 1;
                    self.touch();
                }
            }
            ContentState::MilestoneDetail | ContentState::BacklogDetail
                if self.detail_scroll < self.detail_max_scroll.get() =>
            {
                self.detail_scroll += 1;
                self.touch();
            }
            _ => {}
        }
    }

    fn current_list_count(&self) -> usize {
        if matches!(self.active_mode, Mode::ReviewMenu(_)) {
            return 0;
        }
        match self.active_lane {
            Lane::Milestones => self.visible_milestones().len(),
            Lane::Backlog | Lane::Ideas => self.visible_backlog().len(),
            // Path uses `path_scroll` / `path_max_scroll`, not selected_index.
            Lane::Path => 0,
            Lane::Overview => self.visible_inbox().len(),
            // Watch and Settings own independent selection models.
            Lane::Watch => 0,
            Lane::Settings => 0,
        }
    }

    pub fn enter_milestone_detail(&mut self, index: Option<usize>) {
        let idx = index.unwrap_or(self.selected_index);
        let id = {
            let visible = self.visible_milestones();
            if idx >= visible.len() {
                return;
            }
            visible[idx].id.clone()
        };
        self.selected_milestone_id = Some(id);
        self.selected_index = idx;
        self.content = ContentState::MilestoneDetail;
        self.touch();
    }

    /// Enter MilestoneDetail for a milestone by id, independent of list position.
    /// Use this from Path/Inbox drill-in where the caller already knows
    /// the id (a card, a next-action, an inbox item) — resolving through
    /// `visible_milestones()` would pick the wrong row when `hide_done` is set
    /// and the target is a done milestone (M87 AC-01).
    pub fn enter_milestone_detail_by_id(&mut self, id: &str) {
        self.selected_milestone_id = Some(id.to_string());
        // Restore the visible-list selection so go_back / keyboard nav land
        // on this row. When `hide_done` hides the target, there is no valid
        // visible index for it — leave the existing selection rather than
        // storing a full-list index that would be out of bounds for the
        // visible (filtered) list.
        if let Some(visible_pos) = self.visible_milestones().iter().position(|m| m.id == id) {
            self.selected_index = visible_pos;
        }
        self.content = ContentState::MilestoneDetail;
        self.touch();
    }

    pub fn open_thread(&mut self) {
        if self.selected_milestone_id.is_none() {
            return;
        }
        self.content = ContentState::AnnotationThread;
        self.selected_annotation_index = 0;
        // M136 review remediation: also flip `active_mode` so the
        // dispatcher routes annotation-thread keys through the dedicated
        // `modes::annotation_thread::handle_key` handler. Pre-fix this
        // method only set the content state; the dispatcher then ran the
        // legacy `(ContentState::AnnotationThread, Event)` arms in
        // `modes::normal::handle_event_in_normal`, leaving
        // `modes::annotation_thread::handle_key` and the
        // `Mode::AnnotationThread` variant unreachable from production.
        // `apply_action::CloseAnnotationThread` is the symmetric exit
        // site — it clears `active_mode` back to `Mode::Normal`.
        self.active_mode = Mode::AnnotationThread;
        self.touch();
    }

    pub fn go_back(&mut self) {
        match self.content {
            ContentState::CoApproval => {
                if self.co_approval_state == CoApprovalState::Confirmed {
                    self.co_approval_state = CoApprovalState::Choosing;
                    self.co_approval_action = None;
                    self.co_approval_annotation = None;
                }
                self.content = ContentState::AnnotationThread;
                self.touch();
            }
            ContentState::AnnotationThread => {
                self.content = ContentState::MilestoneDetail;
                self.touch();
            }
            ContentState::MilestoneDetail | ContentState::BacklogDetail => {
                self.detail_scroll = 0;
                self.content = ContentState::List;
                self.touch();
            }
            ContentState::List => {
                if self.active_lane != Lane::Overview {
                    self.active_lane = Lane::Overview;
                    self.selected_index = 0;
                    self.touch();
                }
            }
        }
    }

    pub fn toggle_filter(&mut self) {
        let selected_id = self
            .selected_annotation()
            .map(|annotation| annotation.id.clone());
        self.open_only = !self.open_only;
        self.reanchor_annotation_selection(selected_id.as_deref());
        self.touch();
    }

    /// M172 S5: open the sort-rebind inline menu with the available
    /// sort keys for the active lane. Idempotent — calling twice
    /// without an intermediate close is a no-op (the menu stays open
    /// at its current index). Lanes without a sort menu (Path /
    /// Overview / Settings) leave the menu closed — `open_sort_rebind`
    /// is a no-op there.
    pub fn open_sort_rebind(&mut self) {
        let keys = sort_keys_for(self.active_lane.clone());
        if keys.is_empty() {
            // Lanes that don't surface a sort menu: don't even open
            // an empty menu. `sort_rebind_open()` stays `false`.
            return;
        }
        self.sort_rebind_menu = Some(keys);
        // Default the highlight to the lane's current sort key, so
        // the user sees their existing choice highlighted when they
        // open the menu.
        let current = self.lane_sort_key(self.active_lane.clone());
        self.sort_rebind_index = self
            .sort_rebind_menu
            .as_ref()
            .and_then(|keys| keys.iter().position(|k| *k == current))
            .unwrap_or(0);
        self.touch();
    }

    /// M172 S5: cycle to the next sort key in the open menu.
    pub fn cycle_sort_rebind_next(&mut self) {
        if let Some(keys) = self.sort_rebind_menu.as_ref() {
            if !keys.is_empty() {
                self.sort_rebind_index = (self.sort_rebind_index + 1) % keys.len();
                self.touch();
            }
        }
    }

    /// M172 S5: cycle to the previous sort key.
    pub fn cycle_sort_rebind_prev(&mut self) {
        if let Some(keys) = self.sort_rebind_menu.as_ref() {
            if !keys.is_empty() {
                let n = keys.len();
                self.sort_rebind_index = (self.sort_rebind_index + n - 1) % n;
                self.touch();
            }
        }
    }

    /// M172 S5: bind the highlighted sort key to the active lane
    /// and close the menu.
    pub fn confirm_sort_rebind(&mut self) {
        if let Some(keys) = self.sort_rebind_menu.as_ref() {
            if let Some(k) = keys.get(self.sort_rebind_index) {
                self.lane_sort_key.insert(self.active_lane.clone(), *k);
            }
        }
        // M182 S5 (external review F-06): after a sort rebind, the
        // selected_index no longer points at the same milestone — the
        // re-ordering moved it. Find the previously-selected milestone
        // by id and re-anchor the selection so the highlight doesn't
        // jump to a random row.
        if let Some(prev_id) = self.selected_milestone_id.clone() {
            let visible_ids: Vec<String> = self
                .visible_milestones()
                .iter()
                .map(|m| m.id.clone())
                .collect();
            if let Some(pos) = visible_ids.iter().position(|id| id == &prev_id) {
                self.selected_index = pos;
            }
        }
        self.sort_rebind_menu = None;
        self.touch();
    }

    /// M172 S5: close the menu without changing the sort key.
    pub fn cancel_sort_rebind(&mut self) {
        self.sort_rebind_menu = None;
        self.touch();
    }

    /// M172 S5: read the active lane's sort key (defaults to `Id`).
    pub fn lane_sort_key(&self, lane: Lane) -> SortKey {
        self.lane_sort_key
            .get(&lane)
            .copied()
            .unwrap_or(SortKey::Id)
    }

    /// M172 S5: is the sort-rebind menu currently open?
    pub fn sort_rebind_open(&self) -> bool {
        self.sort_rebind_menu.is_some()
    }

    /// M172 S5: which key is highlighted in the open menu? Returns
    /// `None` when the menu is closed.
    pub fn sort_rebind_highlight(&self) -> Option<SortKey> {
        self.sort_rebind_menu
            .as_ref()
            .and_then(|keys| keys.get(self.sort_rebind_index).copied())
    }

    pub fn toggle_hide_done(&mut self) {
        self.hide_done = !self.hide_done;
        self.selected_index = 0;
        self.touch();
    }

    /// M185: open the lifecycle filter modal (snapshots current filter).
    pub fn open_lifecycle_filter(&mut self) {
        use crate::tui::mode::LifecycleFilterState;
        self.active_mode = Mode::LifecycleFilter(LifecycleFilterState {
            selected: 0,
            draft: self.milestone_filter.clone(),
            prior: self.milestone_filter.clone(),
        });
        self.touch();
    }

    pub fn lifecycle_filter_toggle(&mut self) {
        use crate::tui::mode::Mode as M;
        use crate::tui::progress::LIFECYCLE_FILTER_OPTIONS;
        if let M::LifecycleFilter(ref mut st) = self.active_mode {
            if let Some(lc) = LIFECYCLE_FILTER_OPTIONS.get(st.selected) {
                let key = (*lc).to_string();
                if !st.draft.remove(&key) {
                    st.draft.insert(key);
                }
                self.touch();
            }
        }
    }

    pub fn lifecycle_filter_next(&mut self) {
        use crate::tui::mode::Mode as M;
        use crate::tui::progress::LIFECYCLE_FILTER_OPTIONS;
        if let M::LifecycleFilter(ref mut st) = self.active_mode {
            let n = LIFECYCLE_FILTER_OPTIONS.len();
            if n > 0 {
                st.selected = (st.selected + 1) % n;
                self.touch();
            }
        }
    }

    pub fn lifecycle_filter_prev(&mut self) {
        use crate::tui::mode::Mode as M;
        use crate::tui::progress::LIFECYCLE_FILTER_OPTIONS;
        if let M::LifecycleFilter(ref mut st) = self.active_mode {
            let n = LIFECYCLE_FILTER_OPTIONS.len();
            if n > 0 {
                st.selected = (st.selected + n - 1) % n;
                self.touch();
            }
        }
    }

    pub fn lifecycle_filter_commit(&mut self) {
        use crate::tui::mode::Mode as M;
        if let M::LifecycleFilter(st) = &self.active_mode {
            self.milestone_filter = st.draft.clone();
            self.selected_index = 0;
            self.active_mode = Mode::Normal;
            self.touch();
        }
    }

    pub fn lifecycle_filter_cancel(&mut self) {
        use crate::tui::mode::Mode as M;
        if let M::LifecycleFilter(st) = &self.active_mode {
            self.milestone_filter = st.prior.clone();
            self.active_mode = Mode::Normal;
            self.touch();
        }
    }

    /// M185: Grooming preset — approved + in-progress + groomed.
    pub fn apply_grooming_preset(&mut self) {
        self.milestone_filter = crate::tui::progress::GROOMING_PRESET
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        self.selected_index = 0;
        self.touch();
    }

    /// M186: the active lane's committed search term (empty = no filter).
    pub fn lane_search_term(&self) -> &str {
        self.lane_search
            .get(&self.active_lane)
            .map(String::as_str)
            .unwrap_or("")
    }

    /// M186: open the search input, snapshotting the prior term.
    pub fn open_search(&mut self) {
        use crate::tui::mode::SearchInputState;
        let prior = self.lane_search_term().to_string();
        self.active_mode = Mode::SearchInput(SearchInputState {
            buffer: String::new(),
            prior,
        });
        self.touch();
    }

    pub fn search_push_char(&mut self, c: char) {
        if let Mode::SearchInput(state) = &mut self.active_mode {
            state.buffer.push(c);
            // Live-filter: mirror the draft into the lane's committed
            // term so visible_milestones/visible_backlog narrow as the
            // user types. Commit freezes this; Cancel restores `prior`.
            self.lane_search
                .insert(self.active_lane.clone(), state.buffer.clone());
            self.selected_index = 0;
            self.touch();
        }
    }

    pub fn search_backspace(&mut self) {
        if let Mode::SearchInput(state) = &mut self.active_mode {
            if state.buffer.pop().is_some() {
                self.lane_search
                    .insert(self.active_lane.clone(), state.buffer.clone());
                self.selected_index = 0;
                self.touch();
            }
        }
    }

    pub fn search_commit(&mut self) {
        if let Mode::SearchInput(state) = std::mem::replace(&mut self.active_mode, Mode::Normal) {
            self.lane_search
                .insert(self.active_lane.clone(), state.buffer);
            self.selected_index = 0;
            self.touch();
        }
    }

    pub fn search_cancel(&mut self) {
        if let Mode::SearchInput(state) = std::mem::replace(&mut self.active_mode, Mode::Normal) {
            // Restore prior term (could be empty).
            if state.prior.is_empty() {
                self.lane_search.remove(&self.active_lane);
            } else {
                self.lane_search
                    .insert(self.active_lane.clone(), state.prior);
            }
            self.touch();
        }
    }

    /// M186: cycle the active lane's sort key. Cycles within the
    /// per-lane key set returned by [`sort_keys_for`] — milestone
    /// lanes go Id→Lifecycle→Priority→Updated→Id; backlog/ideas go
    /// Id→Status→Priority→Id. Lanes without a sort menu are a no-op
    /// (the action layer also gates this).
    pub fn cycle_sort_next(&mut self) {
        let lane = self.active_lane.clone();
        let keys = sort_keys_for(lane.clone());
        if keys.is_empty() {
            return;
        }
        let cur = self.lane_sort_key(lane.clone());
        let next = keys
            .iter()
            .cycle()
            .skip_while(|&k| *k != cur)
            .nth(1)
            .copied()
            .unwrap_or(keys[0]);
        self.lane_sort_key.insert(lane, next);
        self.selected_index = 0;
        self.touch();
    }

    /// Milestones currently shown: all, or non-done when `hide_done` is set.
    /// M144: filters on the canonical `lifecycle` field (terminal values
    /// `complete` and `cancelled` map to "done" UX; `done` and `remediation`
    /// stay visible because they represent in-flight review work).
    pub fn visible_milestones(&self) -> Vec<&MilestoneSummary> {
        let mut filtered: Vec<&MilestoneSummary> = if self.hide_done {
            self.milestones
                .iter()
                .filter(|m| m.lifecycle != "complete" && m.lifecycle != "cancelled")
                .collect()
        } else {
            self.milestones.iter().collect()
        };
        // M185: multi-select lifecycle filter (empty = all).
        if !self.milestone_filter.is_empty() {
            filtered.retain(|m| self.milestone_filter.contains(&m.lifecycle));
        }
        // M186: per-lane substring search against id+title.
        let term = self.lane_search_term();
        if !term.is_empty() {
            let needle = term.to_ascii_lowercase();
            filtered.retain(|m| {
                m.id.to_ascii_lowercase().contains(&needle)
                    || m.title.to_ascii_lowercase().contains(&needle)
            });
        }
        // M182 S3: apply the active lane's sort key. The sort runs
        // client-side on the cached full list (no extra mp round-
        // trip). Ties fall back to numeric-id compare so the order is
        // stable across binds.
        let key = self.lane_sort_key(self.active_lane.clone());
        match key {
            SortKey::Id => {
                filtered.sort_by(|a, b| compare_milestone_ids(&a.id, &b.id));
            }
            SortKey::Lifecycle => {
                // Lifecycle order: in-progress > approved > groomed >
                // done > self-reviewed > reviewed > complete >
                // cancelled > remediation > draft. Ties → numeric id.
                filtered.sort_by(|a, b| {
                    let ord = lifecycle_rank(&b.lifecycle).cmp(&lifecycle_rank(&a.lifecycle));
                    ord.then_with(|| compare_milestone_ids(&a.id, &b.id))
                });
            }
            SortKey::Priority => {
                // Higher priority first (urgent > high > normal >
                // low). Ties → numeric id. The rank helper mirrors
                // `mp::path_prefs::priority_rank` — duplicated here
                // to avoid a cross-crate dep for a one-line rank
                // table.
                filtered.sort_by(|a, b| {
                    let ord = priority_rank(&b.priority).cmp(&priority_rank(&a.priority));
                    ord.then_with(|| compare_milestone_ids(&a.id, &b.id))
                });
            }
            SortKey::Updated => {
                // Most recent first. Empty `updated` sinks to the
                // bottom under ascending order.
                filtered.sort_by(|a, b| {
                    let ord = b.updated.cmp(&a.updated);
                    ord.then_with(|| compare_milestone_ids(&a.id, &b.id))
                });
            }
            // Status is backlog-only; on Milestones it's a stale bind
            // (defensive — should not happen via the S menu, which
            // surfaces the per-lane key set). Fall back to id ordering.
            SortKey::Status => {
                filtered.sort_by(|a, b| compare_milestone_ids(&a.id, &b.id));
            }
            // Alphabetical by title, case-insensitive. Ties fall back to
            // numeric-id compare so the order is stable.
            SortKey::Title => {
                filtered.sort_by(|a, b| {
                    let ord = a
                        .title
                        .to_ascii_lowercase()
                        .cmp(&b.title.to_ascii_lowercase());
                    ord.then_with(|| compare_milestone_ids(&a.id, &b.id))
                });
            }
        }
        filtered
    }

    /// Backlog rows currently shown. The `active_lane` narrows the
    /// list (M184 / F-01):
    /// - [`Lane::Backlog`] — actionable work: `B-*` (canonical
    ///   `mp backlog add` prefix), plus `TW-*` / `BF-*` / `BL-*`
    /// - [`Lane::Ideas`] — `ID-*` only (exploratory)
    /// - other lanes — empty (caller should not render a backlog table)
    ///
    /// `hide_done` further filters terminal items.
    pub fn visible_backlog(&self) -> Vec<&BacklogLine> {
        let in_lane = |b: &&BacklogLine| match self.active_lane {
            Lane::Backlog => is_actionable_backlog_id(&b.id),
            Lane::Ideas => b.id.starts_with("ID-"),
            _ => false,
        };
        let term = self.lane_search_term();
        let matches_search = |b: &&BacklogLine| {
            if term.is_empty() {
                return true;
            }
            let needle = term.to_ascii_lowercase();
            b.id.to_ascii_lowercase().contains(&needle)
                || b.title.to_ascii_lowercase().contains(&needle)
        };
        let mut filtered: Vec<&BacklogLine> = if self.hide_done {
            self.backlog
                .iter()
                .filter(|b| in_lane(b) && !backlog_is_terminal(b) && matches_search(b))
                .collect()
        } else {
            self.backlog
                .iter()
                .filter(|b| in_lane(b) && matches_search(b))
                .collect()
        };
        // Apply the active lane's sort key. Mirrors `visible_milestones()` —
        // backlog rows sort by Id / Status / Priority (the per-lane key
        // set from `sort_keys_for`). Updated is not available (BacklogLine
        // has no timestamp).
        let key = self.lane_sort_key(self.active_lane.clone());
        match key {
            SortKey::Id => {
                filtered.sort_by(|a, b| compare_milestone_ids(&a.id, &b.id));
            }
            SortKey::Status => {
                filtered.sort_by(|a, b| {
                    let ord = status_rank(&b.status).cmp(&status_rank(&a.status));
                    ord.then_with(|| compare_milestone_ids(&a.id, &b.id))
                });
            }
            SortKey::Priority => {
                filtered.sort_by(|a, b| {
                    let ord = priority_rank(&b.priority).cmp(&priority_rank(&a.priority));
                    ord.then_with(|| compare_milestone_ids(&a.id, &b.id))
                });
            }
            // Lifecycle/Updated don't apply to backlog rows — fall back
            // to id ordering if a stale bind somehow lands here.
            SortKey::Lifecycle | SortKey::Updated => {
                filtered.sort_by(|a, b| compare_milestone_ids(&a.id, &b.id));
            }
            // Alphabetical by title, case-insensitive.
            SortKey::Title => {
                filtered.sort_by(|a, b| {
                    let ord = a
                        .title
                        .to_ascii_lowercase()
                        .cmp(&b.title.to_ascii_lowercase());
                    ord.then_with(|| compare_milestone_ids(&a.id, &b.id))
                });
            }
        }
        filtered
    }

    pub fn toggle_help(&mut self) {
        self.active_mode = match self.active_mode {
            Mode::Help => Mode::Normal,
            _ => Mode::Help,
        };
        self.touch();
    }

    pub fn quit(&mut self) {
        self.quitting = true;
        self.touch();
    }

    pub fn start_input(&mut self, target: String, kind: String) {
        self.active_mode = Mode::Input(InputState {
            target,
            kind,
            buffer: String::new(),
        });
        self.touch();
    }

    pub fn push_input_char(&mut self, c: char) {
        if let Mode::Input(state) = &mut self.active_mode {
            state.buffer.push(c);
            self.touch();
        }
    }

    pub fn pop_input_char(&mut self) {
        if let Mode::Input(state) = &mut self.active_mode {
            if state.buffer.pop().is_some() {
                self.touch();
            }
        }
    }

    pub fn confirm_input(&mut self) -> Option<(String, String, String)> {
        if let Mode::Input(state) = std::mem::replace(&mut self.active_mode, Mode::Normal) {
            let body = state.buffer;
            if !body.is_empty() {
                self.touch();
                return Some((state.target, state.kind, body));
            }
        }
        None
    }

    pub fn cancel_input(&mut self) {
        if matches!(self.active_mode, Mode::Input(_)) {
            self.active_mode = Mode::Normal;
            self.touch();
        }
    }

    /// True when the input-mode overlay is up. Pre-M136 the same name
    /// queried `input_mode: Option<InputMode>`; M136 routes it through
    /// `active_mode`.
    pub fn is_input_active(&self) -> bool {
        matches!(self.active_mode, Mode::Input(_))
    }

    pub fn open_review_menu(&mut self) {
        self.active_mode = Mode::ReviewMenu(ReviewMenuState {
            items: ReviewMenuState::canonical(),
            selected: 0,
        });
        self.touch();
    }

    pub fn close_review_menu(&mut self) {
        if matches!(self.active_mode, Mode::ReviewMenu(_)) {
            self.active_mode = Mode::Normal;
            self.preflight_gate = None;
            self.touch();
        }
    }

    pub fn selected_review_action(&self) -> Option<&str> {
        if let Mode::ReviewMenu(menu) = &self.active_mode {
            return menu.items.get(menu.selected).map(|s| s.as_str());
        }
        None
    }

    pub fn selected_milestone(&self) -> Option<&MilestoneSummary> {
        self.visible_milestones().get(self.selected_index).copied()
    }

    pub fn selected_annotation(&self) -> Option<&AnnotationInfo> {
        self.visible_annotations()
            .get(self.selected_annotation_index)
            .copied()
    }

    /// Canonical interactive inbox projection used by rendering and actions.
    pub fn visible_inbox(&self) -> &[InboxLine] {
        &self.dashboard.inbox_items
    }

    pub fn visible_annotations(&self) -> Vec<&AnnotationInfo> {
        if self.open_only {
            self.annotations
                .iter()
                .filter(|a| a.status == "open")
                .collect()
        } else {
            self.annotations.iter().collect()
        }
    }

    pub fn enter_co_approval(&mut self, annotation: AnnotationInfo, milestone_id: String) {
        self.co_approval_annotation = Some(annotation);
        self.co_approval_milestone_id = Some(milestone_id);
        self.co_approval_action = None;
        self.co_approval_state = CoApprovalState::Choosing;
        self.content = ContentState::CoApproval;
        self.touch();
    }

    pub fn set_co_approval_action(&mut self, action: CoApprovalAction) {
        self.co_approval_action = Some(action);
        self.touch();
    }

    pub fn begin_co_approval_execution(
        &mut self,
    ) -> Result<(String, String, CoApprovalAction), &'static str> {
        if self.co_approval_state != CoApprovalState::Choosing {
            return Err("Co-approval is already executing or confirmed.");
        }
        let ann = self
            .co_approval_annotation
            .as_ref()
            .filter(|annotation| !annotation.id.is_empty())
            .ok_or("No approval annotation is selected.")?;
        let milestone_id = self
            .co_approval_milestone_id
            .as_ref()
            .filter(|id| !id.is_empty())
            .ok_or("No milestone is selected for co-approval.")?;
        let action = self
            .co_approval_action
            .clone()
            .ok_or("Choose Approve or Reject before confirming.")?;
        let result = (ann.id.clone(), milestone_id.clone(), action);
        self.co_approval_state = CoApprovalState::Executing;
        self.touch();
        Ok(result)
    }

    pub fn finish_co_approval(&mut self) {
        self.co_approval_state = CoApprovalState::Confirmed;
        self.touch();
    }

    pub fn fail_co_approval(&mut self, error: impl Into<String>) {
        self.co_approval_state = CoApprovalState::Choosing;
        let error = error.into();
        self.set_action_error(format!("Co-approval failed: {error}"), error);
    }

    fn reanchor_annotation_selection(&mut self, selected_id: Option<&str>) {
        let visible_ids: Vec<&str> = self
            .visible_annotations()
            .iter()
            .map(|annotation| annotation.id.as_str())
            .collect();
        if let Some(position) =
            selected_id.and_then(|id| visible_ids.iter().position(|visible_id| *visible_id == id))
        {
            self.selected_annotation_index = position;
        } else {
            self.selected_annotation_index = self
                .selected_annotation_index
                .min(visible_ids.len().saturating_sub(1));
        }
    }

    /// Tab / Shift+Tab previous lane — wraps from the first lane
    /// (Overview) to the last (Settings).
    ///
    /// **M169-rev (LOW fix):** wrap at start so AC-01 ("Pressing Tab
    /// while the active lane is Settings, Milestones, Backlog,
    /// Overview, or Path cycles `app.active_lane` along
    /// `Lane::ordered()` (wrapping at end). Pressing Shift+Tab cycles
    /// the same set in reverse") holds. Pre-M169 this clamped at
    /// `pos == 0`; the M167/M140 era tests pinned the clamp behavior
    /// and the AC text was aspirational until now.
    pub fn tab_move_up(&mut self) {
        let lanes = Lane::ordered();
        if let Some(pos) = lanes.iter().position(|l| *l == self.active_lane) {
            let next_pos = if pos == 0 { lanes.len() - 1 } else { pos - 1 };
            self.select_lane(lanes[next_pos].clone());
        }
    }

    /// Tab / Shift+Tab next lane — wraps from the last lane (Settings)
    /// to the first (Overview).
    pub fn tab_move_down(&mut self) {
        let lanes = Lane::ordered();
        if let Some(pos) = lanes.iter().position(|l| *l == self.active_lane) {
            let next_pos = (pos + 1) % lanes.len();
            self.select_lane(lanes[next_pos].clone());
        }
    }
}

/// Terminal backlog statuses hidden by `hide_done` (`h` on the Backlog tab).
pub fn backlog_is_terminal(b: &BacklogLine) -> bool {
    matches!(
        b.status.to_ascii_lowercase().as_str(),
        "resolved" | "done" | "archived" | "dismissed" | "closed" | "cancelled"
    )
}

/// M184 F-01: id prefixes that belong on [`Lane::Backlog`].
///
/// - `B-` — canonical formal backlog (`mp` `next_backlog_id`)
/// - `BL-` — alias retained for any BL-shaped rows / early M184 draft
/// - `TW-` / `BF-` — folded Tweaks / Bugfixes tracks
///
/// Order is intentional: check `BF-` before relying on a bare `B-`
/// would be wrong only if we used a single `B` prefix; each check is
/// a full `"X-"` prefix so they are mutually exclusive.
pub fn is_actionable_backlog_id(id: &str) -> bool {
    id.starts_with("B-") || id.starts_with("BL-") || id.starts_with("TW-") || id.starts_with("BF-")
}

/// M181 S2: build the legacy `DashboardSnapshot` from the typed
/// `OverviewSnapshot` so the existing renderer keeps working until
/// S3 retires the legacy state. The mapping is purely additive —
/// nothing is fabricated, every field comes from the consolidated
/// payload mp exposes.
fn legacy_dashboard_from_overview(
    snap: &crate::overview_snapshot::OverviewSnapshot,
) -> DashboardSnapshot {
    use crate::overview_snapshot::{InboxItem as TInbox, PathItem as TPath};
    let next_action = snap
        .path
        .first()
        .map(|p: &TPath| p.display.clone())
        .unwrap_or_default();
    let path_preview: Vec<String> = snap.path.iter().map(|p| p.display.clone()).collect();
    let inbox_items: Vec<InboxLine> = snap
        .inbox
        .iter()
        .map(|i: &TInbox| InboxLine {
            id: i.id.clone(),
            kind: i.kind.clone(),
            display: i.display.clone(),
            reason: i.reason.clone(),
            action: i.action.clone(),
        })
        .collect();
    DashboardSnapshot {
        planning_status: snap.health.planning_state.clone(),
        execution_mode: snap.health.execution_mode.clone(),
        inbox_count: snap.queues.inbox,
        pending_review_count: snap.queues.pending_reviews,
        track_pending: snap.queues.backlog,
        annotations_open: snap.queues.open_annotations,
        next_action,
        path_preview,
        execution_counts: ExecutionCounts {
            total: snap.totals.milestones,
            done: snap.lifecycle.complete,
            planned: snap.lifecycle.approved,
            in_progress: snap.lifecycle.in_progress,
            blocked: snap.queues.blocked_milestones,
        },
        spec_counts: SpecCounts::default(),
        lifecycle_counts: LifecycleCounts {
            total: snap.totals.milestones,
            draft: snap.lifecycle.draft,
            groomed: snap.lifecycle.groomed,
            approved: snap.lifecycle.approved,
            in_progress: snap.lifecycle.in_progress,
            done: snap.lifecycle.done,
            self_reviewed: snap.lifecycle.self_reviewed,
            reviewed: snap.lifecycle.reviewed,
            complete: snap.lifecycle.complete,
            remediation: snap.lifecycle.remediation,
        },
        blockers: Vec::new(),
        inbox_items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review_menu_selected(app: &App) -> usize {
        match &app.active_mode {
            Mode::ReviewMenu(menu) => menu.selected,
            _ => panic!("expected Mode::ReviewMenu, got {:?}", app.active_mode),
        }
    }

    fn sample_milestones() -> Vec<MilestoneSummary> {
        vec![
            MilestoneSummary {
                id: "01".to_string(),
                title: "Setup".to_string(),
                lifecycle: "complete".to_string(),
                lifecycle_at: Some("2026-07-04T00:00:00Z".to_string()),
                depends_on: vec![],
                priority: "normal".to_string(),
                updated: String::new(),
            },
            MilestoneSummary {
                id: "02".to_string(),
                title: "Core".to_string(),
                lifecycle: "in-progress".to_string(),
                lifecycle_at: Some("2026-07-08T00:00:00Z".to_string()),
                depends_on: vec![],
                priority: "normal".to_string(),
                updated: String::new(),
            },
            MilestoneSummary {
                id: "03".to_string(),
                title: "Polish".to_string(),
                lifecycle: "draft".to_string(),
                lifecycle_at: None,
                depends_on: vec![],
                priority: "normal".to_string(),
                updated: String::new(),
            },
        ]
    }

    #[test]
    fn initial_state_is_status_lane() {
        let app = App::new();
        assert_eq!(app.active_lane, Lane::Overview);
        assert_eq!(app.content, ContentState::List);
        assert_eq!(app.selected_index, 0);
        assert!(!app.quitting);
        assert!(!matches!(app.active_mode, Mode::Help));
    }

    #[test]
    fn select_lane_resets_content() {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        assert_eq!(app.active_lane, Lane::Milestones);
        assert_eq!(app.content, ContentState::List);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn move_up_down_clamps_on_milestones() {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        app.load_milestones(sample_milestones());

        assert_eq!(app.selected_index, 0);
        app.move_up();
        assert_eq!(app.selected_index, 0);

        app.move_down();
        assert_eq!(app.selected_index, 1);
        app.move_down();
        assert_eq!(app.selected_index, 2);
        app.move_down();
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn drill_detail_and_back() {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        app.load_milestones(sample_milestones());
        app.move_down();
        assert_eq!(app.selected_index, 1);

        app.enter_milestone_detail(None);
        assert_eq!(app.content, ContentState::MilestoneDetail);
        assert_eq!(app.selected_milestone_id, Some("02".to_string()));

        app.go_back();
        assert_eq!(app.content, ContentState::List);
        assert_eq!(app.active_lane, Lane::Milestones);
    }

    #[test]
    fn drill_open_thread_back_produces_correct_state() {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        app.load_milestones(sample_milestones());
        app.enter_milestone_detail(Some(0));
        assert_eq!(app.content, ContentState::MilestoneDetail);
        assert_eq!(app.selected_milestone_id, Some("01".to_string()));

        app.open_thread();
        assert_eq!(app.content, ContentState::AnnotationThread);

        app.go_back();
        assert_eq!(app.content, ContentState::MilestoneDetail);

        app.go_back();
        assert_eq!(app.content, ContentState::List);
    }

    #[test]
    fn go_back_from_status_list_stays() {
        let mut app = App::new();
        assert_eq!(app.active_lane, Lane::Overview);
        assert!(!app.quitting);
        app.go_back();
        assert!(!app.quitting);
        assert_eq!(app.active_lane, Lane::Overview);
    }

    #[test]
    fn filter_toggle() {
        let mut app = App::new();
        assert!(!app.open_only);
        app.toggle_filter();
        assert!(app.open_only);
        app.toggle_filter();
        assert!(!app.open_only);
    }

    #[test]
    fn help_toggle() {
        let mut app = App::new();
        assert!(!matches!(app.active_mode, Mode::Help));
        app.toggle_help();
        assert!(matches!(app.active_mode, Mode::Help));
        app.toggle_help();
        assert!(!matches!(app.active_mode, Mode::Help));
    }

    #[test]
    fn selected_milestone_returns_correct() {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        app.load_milestones(sample_milestones());
        app.move_down();
        let m = app.selected_milestone().unwrap();
        assert_eq!(m.id, "02");
        assert_eq!(m.title, "Core");
    }

    #[test]
    fn visible_annotations_filters_open_only() {
        let mut app = App::new();
        app.load_annotations(vec![
            AnnotationInfo {
                id: "AN-01".into(),
                target: "01".into(),
                kind: "review".into(),
                status: "open".into(),
                author: "alice".into(),
                body: "Looks good".into(),
                created_at: "".into(),
                resolved_at: "".into(),
            },
            AnnotationInfo {
                id: "AN-02".into(),
                target: "01".into(),
                kind: "review".into(),
                status: "resolved".into(),
                author: "bob".into(),
                body: "Fixed".into(),
                created_at: "".into(),
                resolved_at: "".into(),
            },
        ]);
        assert_eq!(app.visible_annotations().len(), 2);
        app.toggle_filter();
        assert_eq!(app.visible_annotations().len(), 1);
    }

    #[test]
    fn quit_sets_flag() {
        let mut app = App::new();
        assert!(!app.quitting);
        app.quit();
        assert!(app.quitting);
    }

    #[test]
    fn review_menu_move_up_down() {
        let mut app = App::new();
        app.open_review_menu();
        assert_eq!(review_menu_selected(&app), 0);
        app.move_down();
        assert_eq!(review_menu_selected(&app), 1);
        app.move_down();
        assert_eq!(review_menu_selected(&app), 2);
        app.move_up();
        assert_eq!(review_menu_selected(&app), 1);
        app.close_review_menu();
        assert!(!matches!(app.active_mode, Mode::ReviewMenu(_)));
    }

    #[test]
    fn enter_co_approval_sets_content() {
        let ann = AnnotationInfo {
            id: "AN-01".into(),
            target: "01".into(),
            kind: "approval-request".into(),
            status: "open".into(),
            author: "alice".into(),
            body: "Please approve".into(),
            created_at: "".into(),
            resolved_at: "".into(),
        };
        let mut app = App::new();
        app.enter_co_approval(ann, "01".to_string());
        assert_eq!(app.content, ContentState::CoApproval);
        assert!(app.co_approval_annotation.is_some());
        assert_eq!(app.co_approval_milestone_id, Some("01".to_string()));

        app.go_back();
        assert_eq!(app.content, ContentState::AnnotationThread);
    }

    #[test]
    fn lane_switching_preserves_state_fields() {
        let mut app = App::new();
        app.toggle_filter();
        assert!(app.open_only);

        app.select_lane(Lane::Backlog);
        assert_eq!(app.active_lane, Lane::Backlog);
        assert_eq!(app.content, ContentState::List);
        assert!(app.open_only); // other fields preserved
    }
}
