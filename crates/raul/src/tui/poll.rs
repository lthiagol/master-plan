//! M217: Autopilot lane auto-refresh policy + idle poller.
//!
//! M216 shipped the in-session surfaces behind a *manual* `r`
//! refresh. This module is what M216 deferred: a coalescing
//! single-flight poller that refreshes the focused Autopilot
//! lane's display from the `mp autopilot` read APIs.
//!
//! Three invariants shape the design:
//!
//! 1. **Observer-only.** The poller reads `mp autopilot session
//!    show` + `mp autopilot status`. It never writes session
//!    state, never issues a control verb, and never shells out to
//!    `herdr`. Orchestration liveness, heartbeats, dispatch, and
//!    stale-state escalation stay in mp (M213). If raul is closed,
//!    the drive is unaffected.
//! 2. **Single-flight + coalescing.** At most one refresh request
//!    is in flight. A tick that arrives while a slow call is
//!    outstanding is coalesced (counted, not queued), so an
//!    overrunning `mp` call can never accumulate overlapping
//!    subprocesses.
//! 3. **Display-only cadence.** Polling is gated on the lane
//!    being focused and the poll toggle being on. Pausing display
//!    polling has no effect on the headless engine; resuming does
//!    not replay a catch-up burst.
//!
//! The clock is injected: every decision entry point takes a
//! monotonic `now_ms`. Production passes an `Instant`-derived
//! millisecond count; tests pass a fake clock, so the interval
//! and overrun behaviour is deterministic and does not sleep.

use serde_json::Value;

use crate::mp_runner::MpRunner;
use crate::tui::app::App;

/// Built-in refresh cadence, in seconds. The bottom of the
/// milestone's 2-5s window: fresh enough to feel live, slow
/// enough that a 2s `mp` round-trip never queues.
pub const DEFAULT_REFRESH_SECS: u64 = 2;

/// Lower bound for a resolved cadence. A `refresh_secs: 0`
/// override would turn the idle hook into a busy loop, so the
/// resolver clamps to 1s.
pub const MIN_REFRESH_SECS: u64 = 1;

/// Upper bound for a resolved cadence. Beyond a minute the lane
/// is effectively manual-refresh-only; the clamp keeps a typo
/// (`refresh_secs: 86400`) from looking like a broken poller.
pub const MAX_REFRESH_SECS: u64 = 60;

// ─── S04: refresh_secs resolution chain ──────────────────────────

/// Which link in the resolution chain supplied the cadence.
/// Surfaced so the footer (and the AC-04 test) can prove the
/// precedence rather than inferring it from the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshSource {
    /// `session.json` → `config_overrides.refresh_secs`.
    SessionOverride,
    /// `config.json` → `autopilot.refresh_secs`.
    ProjectConfig,
    /// The built-in [`DEFAULT_REFRESH_SECS`].
    BuiltinDefault,
}

impl RefreshSource {
    pub fn label(&self) -> &'static str {
        match self {
            RefreshSource::SessionOverride => "session",
            RefreshSource::ProjectConfig => "config",
            RefreshSource::BuiltinDefault => "default",
        }
    }
}

/// A resolved cadence plus the link that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshPolicy {
    pub secs: u64,
    pub source: RefreshSource,
}

impl Default for RefreshPolicy {
    fn default() -> Self {
        Self {
            secs: DEFAULT_REFRESH_SECS,
            source: RefreshSource::BuiltinDefault,
        }
    }
}

impl RefreshPolicy {
    pub fn interval_ms(&self) -> u64 {
        self.secs.saturating_mul(1_000)
    }
}

/// Read a positive integer `refresh_secs` from a JSON node,
/// accepting both a number and a numeric string (the override
/// panel stores free-text values before mp normalizes them).
fn refresh_secs_at(node: Option<&Value>) -> Option<u64> {
    let v = node?;
    let raw = match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }?;
    if raw == 0 {
        return Some(MIN_REFRESH_SECS);
    }
    Some(raw.clamp(MIN_REFRESH_SECS, MAX_REFRESH_SECS))
}

/// M217 AC-04 / D2: resolve the display cadence.
///
/// Precedence, highest first:
///
/// 1. `session_show.session.config_overrides.refresh_secs`
///    (also accepted at the payload root's `config_overrides`,
///    which is the shape `mp autopilot session show` emits when
///    the session envelope is unwrapped).
/// 2. `config.autopilot.refresh_secs`.
/// 3. [`DEFAULT_REFRESH_SECS`].
///
/// An explicit user override always beats the project default,
/// and the project default always beats the built-in — the
/// chain never falls *back up*.
pub fn resolve_refresh_policy(session_show: &Value, config: &Value) -> RefreshPolicy {
    let session_node = session_show
        .get("session")
        .and_then(|s| s.get("config_overrides"))
        .or_else(|| session_show.get("config_overrides"));
    if let Some(secs) = refresh_secs_at(session_node.and_then(|n| n.get("refresh_secs"))) {
        return RefreshPolicy {
            secs,
            source: RefreshSource::SessionOverride,
        };
    }
    let config_node = config
        .get("autopilot")
        .and_then(|a| a.get("refresh_secs"))
        .or_else(|| config.get("refresh_secs"));
    if let Some(secs) = refresh_secs_at(config_node) {
        return RefreshPolicy {
            secs,
            source: RefreshSource::ProjectConfig,
        };
    }
    RefreshPolicy::default()
}

/// Convenience wrapper for callers that only want the number.
pub fn resolve_refresh_secs(session_show: &Value, config: &Value) -> u64 {
    resolve_refresh_policy(session_show, config).secs
}

// ─── S01 / S02 / S03: the poller ─────────────────────────────────

/// What the poller decided on a given tick. Every non-`Fire`
/// variant names *why* the tick did not shell out, so the
/// focus-gating and manual-control tests assert on the reason
/// rather than on an opaque bool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollDecision {
    /// Shell out now (the caller must call [`AutopilotPoller::begin`]).
    Fire,
    /// The interval has not elapsed yet.
    NotDue,
    /// A previous request is still outstanding — the tick is
    /// coalesced, never queued.
    Coalesced,
    /// The operator turned auto-refresh off.
    Disabled,
    /// The Autopilot lane is not the focused lane.
    Unfocused,
}

impl PollDecision {
    pub fn should_fire(&self) -> bool {
        matches!(self, PollDecision::Fire)
    }
}

/// The Autopilot lane's display poller.
///
/// Owns the cadence, the single-flight flag, the focus gate, and
/// the operator's on/off toggle. Deliberately holds **no**
/// session state: the payloads it fetches are handed to
/// [`crate::tui::autopilot::refresh::refresh_from_json`], which
/// is the same adapter the manual `r` path uses. The poller's
/// own memory is a [`Snapshot`] used purely to decide whether a
/// re-render is warranted (AC-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutopilotPoller {
    policy: RefreshPolicy,
    /// Monotonic ms at which the last request *started*. `None`
    /// means "fire at the first opportunity" (lane entry, or a
    /// refocus).
    last_started_ms: Option<u64>,
    /// True while a request is outstanding.
    in_flight: bool,
    /// The operator's `toggle_poll` state. `true` = auto-refresh on.
    enabled: bool,
    /// True while the Autopilot lane is the focused lane.
    focused: bool,
    /// Requests actually issued.
    fired: u64,
    /// Ticks dropped because a request was still outstanding.
    coalesced: u64,
    /// Ticks dropped because the lane was unfocused.
    skipped_unfocused: u64,
    /// The last snapshot rendered, for the AC-05 diff.
    last_snapshot: Option<Snapshot>,
}

impl Default for AutopilotPoller {
    fn default() -> Self {
        Self {
            policy: RefreshPolicy::default(),
            last_started_ms: None,
            in_flight: false,
            enabled: true,
            focused: false,
            fired: 0,
            coalesced: 0,
            skipped_unfocused: 0,
            last_snapshot: None,
        }
    }
}

impl AutopilotPoller {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct with an explicit cadence (test helper / explicit
    /// policy application).
    pub fn with_refresh_secs(secs: u64) -> Self {
        Self {
            policy: RefreshPolicy {
                secs: secs.clamp(MIN_REFRESH_SECS, MAX_REFRESH_SECS),
                source: RefreshSource::SessionOverride,
            },
            ..Self::default()
        }
    }

    pub fn policy(&self) -> RefreshPolicy {
        self.policy
    }

    pub fn refresh_secs(&self) -> u64 {
        self.policy.secs
    }

    pub fn interval_ms(&self) -> u64 {
        self.policy.interval_ms()
    }

    /// AC-04: adopt the resolved cadence. Changing the cadence
    /// does not reset the timer — a session override that lands
    /// mid-drive takes effect on the next due check rather than
    /// forcing an immediate extra request.
    pub fn apply_policy(&mut self, policy: RefreshPolicy) {
        self.policy = policy;
    }

    /// AC-04: resolve + adopt in one call from the two payloads.
    pub fn apply_policy_from_payloads(&mut self, session_show: &Value, config: &Value) {
        self.apply_policy(resolve_refresh_policy(session_show, config));
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn is_in_flight(&self) -> bool {
        self.in_flight
    }

    pub fn fired_count(&self) -> u64 {
        self.fired
    }

    pub fn coalesced_count(&self) -> u64 {
        self.coalesced
    }

    pub fn skipped_unfocused_count(&self) -> u64 {
        self.skipped_unfocused
    }

    /// AC-03: flip auto-refresh on/off. Returns the new state.
    /// Turning it back on arms an immediate poll (one request,
    /// not a burst) so the operator sees fresh data right away.
    pub fn toggle_enabled(&mut self) -> bool {
        self.enabled = !self.enabled;
        if self.enabled {
            self.last_started_ms = None;
        }
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.toggle_enabled();
        }
    }

    /// AC-02: focus gate. Display polling pauses the moment the
    /// Autopilot lane loses focus and resumes on refocus.
    ///
    /// Refocus arms exactly one immediate request (`last_started_ms
    /// = None`) — not one request per interval missed. That is the
    /// "no catch-up burst" guarantee: the number of requests after
    /// a long unfocused stretch is 1, independent of how long the
    /// lane was away.
    pub fn set_focused(&mut self, focused: bool) {
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        if focused {
            self.last_started_ms = None;
        }
    }

    /// Milliseconds until the next request is due. Zero when due
    /// now (or when nothing has been polled yet).
    pub fn time_until_due_ms(&self, now_ms: u64) -> u64 {
        match self.last_started_ms {
            None => 0,
            Some(started) => {
                let elapsed = now_ms.saturating_sub(started);
                self.interval_ms().saturating_sub(elapsed)
            }
        }
    }

    /// The gate. Pure: does not mutate the poller, so a caller can
    /// inspect the decision without committing to a request.
    pub fn decide(&self, now_ms: u64) -> PollDecision {
        if !self.focused {
            return PollDecision::Unfocused;
        }
        if !self.enabled {
            return PollDecision::Disabled;
        }
        if self.in_flight {
            return PollDecision::Coalesced;
        }
        if self.time_until_due_ms(now_ms) > 0 {
            return PollDecision::NotDue;
        }
        PollDecision::Fire
    }

    /// Evaluate the gate and record the outcome. Returns the same
    /// decision [`decide`](Self::decide) would; on `Fire` the
    /// poller is marked in-flight and the caller **must** call
    /// [`finish`](Self::finish) when the request completes.
    pub fn begin(&mut self, now_ms: u64) -> PollDecision {
        let decision = self.decide(now_ms);
        match decision {
            PollDecision::Fire => {
                self.in_flight = true;
                self.last_started_ms = Some(now_ms);
                self.fired += 1;
            }
            PollDecision::Coalesced => self.coalesced += 1,
            PollDecision::Unfocused => self.skipped_unfocused += 1,
            PollDecision::NotDue | PollDecision::Disabled => {}
        }
        decision
    }

    /// Release the single-flight lock.
    ///
    /// The next request's due time is measured from the *start* of
    /// this one, so a call that overran the interval becomes
    /// immediately due instead of stacking a backlog — one
    /// overrun, one follow-up request.
    pub fn finish(&mut self, _now_ms: u64) {
        self.in_flight = false;
    }

    /// AC-03: manual refresh. Always permitted (even with
    /// auto-refresh off), and it **resets** the timer so the
    /// automatic poll does not fire again immediately after the
    /// operator's own refresh.
    ///
    /// Returns `false` when a request is already outstanding —
    /// single-flight applies to the manual path too, so mashing
    /// `r` cannot spawn overlapping subprocesses.
    pub fn manual_refresh(&mut self, now_ms: u64) -> bool {
        if self.in_flight {
            self.coalesced += 1;
            return false;
        }
        self.in_flight = true;
        self.last_started_ms = Some(now_ms);
        self.fired += 1;
        true
    }

    /// AC-03: the footer indicator. The toggle state is visible in
    /// the lane footer so the operator can tell a paused poll from
    /// a stalled drive.
    pub fn footer_label(&self) -> String {
        if !self.enabled {
            return "poll: off".to_string();
        }
        format!(
            "poll: {}s ({})",
            self.policy.secs,
            self.policy.source.label()
        )
    }

    /// AC-05: diff a freshly fetched pair of payloads against the
    /// last rendered snapshot.
    ///
    /// Pure with respect to session state: the only thing mutated
    /// is the poller's own render bookkeeping. Returns
    /// [`PollOutcome::Unchanged`] when nothing the display cares
    /// about moved, so the caller can skip the re-render (and the
    /// dirty-version bump) entirely.
    pub fn observe(&mut self, session_show: &Value, status: &Value) -> PollOutcome {
        let next = Snapshot::from_payloads(session_show, status);
        let outcome = match &self.last_snapshot {
            Some(prev) if prev == &next => PollOutcome::Unchanged,
            Some(prev) => PollOutcome::StateChanged(StateChange {
                session_id: next.session_id.clone(),
                from_sequence: Some(prev.sequence),
                to_sequence: next.sequence,
                from_revision: Some(prev.revision),
                to_revision: next.revision,
                stale_display: next.sequence < prev.sequence,
            }),
            None => PollOutcome::StateChanged(StateChange {
                session_id: next.session_id.clone(),
                from_sequence: None,
                to_sequence: next.sequence,
                from_revision: None,
                to_revision: next.revision,
                stale_display: false,
            }),
        };
        self.last_snapshot = Some(next);
        outcome
    }

    /// The last snapshot the poller rendered, if any.
    pub fn last_snapshot(&self) -> Option<&Snapshot> {
        self.last_snapshot.as_ref()
    }
}

// ─── S5: snapshot diffing ────────────────────────────────────────

/// The display-relevant fingerprint of one poll.
///
/// `sequence` and `revision` are preserved verbatim from the mp
/// payloads so a *stale display* (a snapshot older than the one
/// already rendered — possible when two readers race) is
/// detectable rather than silently rendered as progress.
///
/// `digest` collapses everything else the lane renders into one
/// value, so a change the sequence/revision pair does not
/// capture (e.g. a pane's role output) still triggers a redraw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub session_id: String,
    pub sequence: u64,
    pub revision: u64,
    pub digest: u64,
}

impl Snapshot {
    pub fn from_payloads(session_show: &Value, status: &Value) -> Self {
        let session = session_show.get("session").unwrap_or(session_show);
        let session_id = session
            .get("id")
            .or_else(|| session.get("session_id"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let sequence = first_u64(session, &["sequence", "seq", "event_seq"])
            .or_else(|| first_u64(status, &["sequence", "seq"]))
            .unwrap_or(0);
        let revision = first_u64(session, &["revision", "rev"])
            .or_else(|| first_u64(status, &["revision", "rev"]))
            .unwrap_or(0);
        Self {
            session_id,
            sequence,
            revision,
            digest: digest_of(session_show, status),
        }
    }
}

fn first_u64(node: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(v) = node.get(*key) {
            if let Some(n) = v.as_u64() {
                return Some(n);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.trim().parse::<u64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn digest_of(session_show: &Value, status: &Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    session_show.to_string().hash(&mut hasher);
    status.to_string().hash(&mut hasher);
    hasher.finish()
}

/// AC-05: a local UI state-change event. Carries the sequence /
/// revision transition so the renderer can label the change and
/// flag a stale display. It is emitted **to the UI only** — the
/// poller does not relay progress to the orchestrator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateChange {
    pub session_id: String,
    pub from_sequence: Option<u64>,
    pub to_sequence: u64,
    pub from_revision: Option<u64>,
    pub to_revision: u64,
    /// True when the fetched snapshot is *older* than the one
    /// already on screen.
    pub stale_display: bool,
}

/// The result of one observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    /// Nothing the display cares about moved — skip the redraw.
    Unchanged,
    /// Re-render; the payload describes the transition.
    StateChanged(StateChange),
}

impl PollOutcome {
    pub fn changed(&self) -> bool {
        matches!(self, PollOutcome::StateChanged(_))
    }

    pub fn state_change(&self) -> Option<&StateChange> {
        match self {
            PollOutcome::StateChanged(c) => Some(c),
            PollOutcome::Unchanged => None,
        }
    }
}

// ─── S6: observer-only argv contract ────────────────────────────

/// The complete set of commands the poller is allowed to issue.
///
/// Enumerated (rather than implied by the call sites) so AC-06
/// can assert the contract mechanically: every argv the poller
/// builds must be in this set, and the set contains only
/// `mp autopilot` **read** verbs.
pub fn poll_argv(session_id: &str) -> Vec<Vec<String>> {
    let id = if session_id.is_empty() {
        "alpha"
    } else {
        session_id
    };
    vec![
        vec![
            "autopilot".to_string(),
            "session".to_string(),
            "show".to_string(),
            id.to_string(),
            "--format".to_string(),
            "json".to_string(),
        ],
        vec![
            "autopilot".to_string(),
            "status".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ],
    ]
}

/// Verbs that mutate session state or drive the orchestrator.
/// The poller must never issue one of these.
const MUTATING_VERBS: &[&str] = &[
    "start", "pause", "resume", "cancel", "restart", "steer", "control", "complete", "set",
    "update", "add", "remove", "prompt",
];

/// AC-06: is `argv` an observer-only read?
///
/// `false` for anything that is not an `mp autopilot` read, for
/// any mutating verb, and for any attempt to shell out to
/// `herdr` (raul is never in the dispatch path — the orchestrator
/// owns prompting).
pub fn is_observer_only_argv(argv: &[String]) -> bool {
    let Some(first) = argv.first() else {
        return false;
    };
    if first != "autopilot" {
        return false;
    }
    if argv.iter().any(|a| a.contains("herdr")) {
        return false;
    }
    let verbs: Vec<&str> = argv
        .iter()
        .skip(1)
        .map(String::as_str)
        .filter(|a| !a.starts_with("--"))
        .collect();
    if verbs.iter().any(|v| MUTATING_VERBS.contains(v)) {
        return false;
    }
    matches!(verbs.first().copied(), Some("session") | Some("status"))
}

// ─── S7: health / heartbeat rendering ───────────────────────────

/// What `mp autopilot status` says about liveness, projected for
/// display.
///
/// Every field is *reported*, never computed: raul does not
/// generate pulses, does not decide that an agent is dead, and
/// does not escalate. A stale status is displayed as stale
/// because mp said so; the escalation decision stays in mp.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Health {
    /// mp's `run_state` verbatim (`live` / `paused` / `stale` / …).
    pub run_state: String,
    /// mp's `health` verbatim (`healthy` / `stale` / `unknown`).
    pub health: String,
    /// mp's `heartbeat_at` timestamp verbatim (RFC3339), empty
    /// when the status carries none.
    pub heartbeat_at: String,
    /// mp's own computed age, when the status reports one. raul
    /// does not derive this from the local wall clock — that
    /// would make the display disagree with mp's escalation view.
    pub heartbeat_age_secs: Option<u64>,
    /// True only when mp classified the status as stale.
    pub stale: bool,
}

impl Health {
    /// Project the status payload. Absent fields degrade to
    /// `unknown` / empty rather than to a fabricated value.
    pub fn from_status(status: &Value) -> Self {
        let node = status.get("status").unwrap_or(status);
        let run_state = node
            .get("run_state")
            .and_then(|r| r.get("kind").or(Some(r)))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let health = node
            .get("health")
            .and_then(|h| h.get("state").or(Some(h)))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let heartbeat_at = node
            .get("heartbeat_at")
            .or_else(|| node.get("health").and_then(|h| h.get("heartbeat_at")))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let heartbeat_age_secs = first_u64(node, &["heartbeat_age_secs"]).or_else(|| {
            node.get("health")
                .and_then(|h| first_u64(h, &["heartbeat_age_secs"]))
        });
        let stale = node
            .get("stale")
            .and_then(Value::as_bool)
            .or_else(|| node.get("health").and_then(|h| h.get("stale")?.as_bool()))
            .unwrap_or(health == "stale" || run_state == "stale");
        Self {
            run_state,
            health,
            heartbeat_at,
            heartbeat_age_secs,
            stale,
        }
    }

    /// One-line badge for the status graph header.
    ///
    /// The `stale` marker is informational — the footer says
    /// "escalation: mp" so the operator knows raul is not the
    /// thing that will act on it.
    pub fn badge(&self) -> String {
        let mut out = format!("health: {}", self.health);
        if let Some(age) = self.heartbeat_age_secs {
            out.push_str(&format!(" · heartbeat {age}s ago"));
        } else if !self.heartbeat_at.is_empty() {
            out.push_str(&format!(" · heartbeat {}", self.heartbeat_at));
        }
        if self.stale {
            out.push_str(" · stale (escalation: mp)");
        }
        out
    }
}

// ─── Production wiring ──────────────────────────────────────────

/// Monotonic millisecond clock for the production idle hook.
///
/// A process-lifetime baseline `Instant`, so `now_ms` is
/// monotonic and cheap. Tests never call this — they pass their
/// own fake clock values.
pub fn now_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static BASE: OnceLock<Instant> = OnceLock::new();
    BASE.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Fetch the two read payloads. The argv comes from
/// [`poll_argv`], so the observer-only contract is enforced by
/// construction rather than by convention.
fn fetch_payloads(runner: &MpRunner, session_id: &str) -> (Value, Value) {
    let mut out = Vec::new();
    for argv in poll_argv(session_id) {
        debug_assert!(
            is_observer_only_argv(&argv),
            "poller argv must be observer-only: {argv:?}"
        );
        let (cmd, rest) = argv.split_first().expect("poll_argv rows are non-empty");
        let args: Vec<&str> = rest.iter().map(String::as_str).collect();
        let bytes = runner.run_raw_allow_failure(cmd, &args).unwrap_or_default();
        out.push(serde_json::from_slice(&bytes).unwrap_or(Value::Null));
    }
    let status = out.pop().unwrap_or(Value::Null);
    let session_show = out.pop().unwrap_or(Value::Null);
    (session_show, status)
}

/// The production entry point, called from `run_loop`'s idle hook.
///
/// Gate → fetch → diff → (maybe) re-render → release. Returns the
/// decision so the caller can leave the loop alone when nothing
/// fired, and so tests can assert the gating without a runner.
///
/// AC-05: the lane's typed surfaces are rebuilt (and the dirty
/// version bumped) **only** when [`AutopilotPoller::observe`]
/// reports a change. An idle drive polling every 2s therefore
/// costs zero redraws.
pub fn poll_autopilot_lane(runner: &MpRunner, app: &mut App, now_ms: u64) -> PollDecision {
    let mut poller = std::mem::take(&mut app.autopilot_poller);
    let decision = poller.begin(now_ms);
    if decision.should_fire() {
        let session_id = app.autopilot.active_session_id().unwrap_or_default();
        let (session_show, status) = fetch_payloads(runner, &session_id);
        let config = runner
            .run_raw_allow_failure("config", &["show", "--format", "json"])
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .unwrap_or(Value::Null);
        poller.apply_policy_from_payloads(&session_show, &config);
        if poller.observe(&session_show, &status).changed() {
            crate::tui::autopilot::refresh::refresh_from_json(
                &mut app.autopilot,
                &session_show,
                &status,
            );
            app.autopilot.health = Some(Health::from_status(&status));
            app.touch();
        }
        poller.finish(now_ms);
    }
    app.autopilot_poller = poller;
    decision
}

/// The manual `r` path, shared with `Action::AutopilotRefresh`.
///
/// Resets the poll timer (AC-03) so the automatic poll does not
/// fire again immediately, and always re-renders — an explicit
/// refresh should visibly land even when the payload is
/// unchanged.
pub fn manual_refresh_lane(runner: &MpRunner, app: &mut App, now_ms: u64) -> bool {
    let mut poller = std::mem::take(&mut app.autopilot_poller);
    let admitted = poller.manual_refresh(now_ms);
    if admitted {
        let session_id = app.autopilot.active_session_id().unwrap_or_default();
        let (session_show, status) = fetch_payloads(runner, &session_id);
        let _ = poller.observe(&session_show, &status);
        crate::tui::autopilot::refresh::refresh_from_json(
            &mut app.autopilot,
            &session_show,
            &status,
        );
        app.autopilot.health = Some(Health::from_status(&status));
        app.touch();
        poller.finish(now_ms);
    }
    app.autopilot_poller = poller;
    admitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_policy_is_the_builtin_two_seconds() {
        let p = resolve_refresh_policy(&Value::Null, &Value::Null);
        assert_eq!(p.secs, DEFAULT_REFRESH_SECS);
        assert_eq!(p.source, RefreshSource::BuiltinDefault);
    }

    #[test]
    fn session_override_beats_project_config() {
        let session = json!({"session": {"config_overrides": {"refresh_secs": 9}}});
        let config = json!({"autopilot": {"refresh_secs": 5}});
        let p = resolve_refresh_policy(&session, &config);
        assert_eq!(p.secs, 9);
        assert_eq!(p.source, RefreshSource::SessionOverride);
    }

    #[test]
    fn unfocused_poller_never_fires() {
        let mut p = AutopilotPoller::new();
        assert_eq!(p.begin(0), PollDecision::Unfocused);
        assert_eq!(p.fired_count(), 0);
    }

    #[test]
    fn observer_only_argv_rejects_control_verbs() {
        assert!(is_observer_only_argv(&[
            "autopilot".into(),
            "status".into()
        ]));
        assert!(!is_observer_only_argv(&[
            "autopilot".into(),
            "control".into(),
            "pause".into()
        ]));
        assert!(!is_observer_only_argv(&["herdr".into(), "agent".into()]));
    }
}
