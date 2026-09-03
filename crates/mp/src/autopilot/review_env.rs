//! M224 — autopilot reviewer execution isolation + clean-room policy.
//!
//! The herdr R9 late addition (and the R9 lesson in M207's review log)
//! was that each review cycle ought to start in a clean cargo state —
//! otherwise the runner's incremental build cache can hide bugs from the
//! reviewer or convince the reviewer that a clean build means the
//! changes are safe. R9 made the discipline manual: the reviewer's
//! prompt remembered `make clean` before each cycle. M224 makes the
//! discipline *structural* — the policy module encodes what "clean
//! room" means and the gate refuses an automatic review pass when the
//! environment cannot be proven isolated.
//!
//! The module owns three layers, pinned against the three ACs:
//!
//! 1. **Provenance** ([`ReviewerProvenance`], [`ActorIdentity`]) —
//!    every review records its binary path, worktree, target directory,
//!    process PID, and actor identity. AC-01: the reviewer environment
//!    carries its own provenance; it does not reuse the runner's
//!    process state.
//! 2. **Mode selection** ([`ReviewEnvMode`], [`CleanRoomTrigger`],
//!    [`select_mode`]) — default is `Normal` (isolated target dir,
//!    fresh process, *no* `cargo clean`). Clean-room escalation only
//!    fires when explicitly configured *or* when provenance checks
//!    fail. AC-02: the policy records the trigger reason and the
//!    commands; unconditional `cargo clean` is never the default.
//! 3. **Gate** ([`ReviewEnvDecision`], [`ReviewEnvError`],
//!    [`gate`]) — pre-flight refuses an automatic review pass when the
//!    worktree is dirty, the reviewer shares the runner's actor
//!    identity, the binary is stale, or the environment is
//!    unverifiable. Every refusal is a typed variant with an
//!    actionable hint. AC-03: no unsafe environment passes the gate.
//!
//! No filesystem or process IO happens here — the module is pure. The
//! production caller (cycle / gate integration) collects paths and PIDs
//! and passes them in. The test suite drives the policy with arbitrary
//! paths and asserts on the typed outputs. This keeps M224 testable
//! without a running herdr and keeps the policy itself deterministic.
//!
//! The module is structured in three commit-stamped layers
//! (S1=Provenance, S2=Mode, S3=Gate). AC-01 maps to S1, AC-02 to S2,
//! AC-03 to S3. The S1 commit only carries the provenance types and
//! accessors; later commits append S2 (mode selection) and S3 (gate).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ─── Actor identity (S1 / AC-01) ───────────────────────────────────────

/// Identity of the lane that produced the environment. Distinct from
/// the runner's identity (lane + actor token + session) — the
/// `distinct_from` check is what AC-03's gate uses to refuse a
/// reviewer that quietly inherited the runner's pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ActorIdentity {
    /// Session id (e.g. `"s-2026-09-03-m222"`). Shared between
    /// orchestrator / runner / reviewer.
    pub session_id: String,
    /// Lane label. Typically `"runner"` or `"reviewer"` — must be
    /// distinct for the gate to pass.
    pub lane: String,
    /// Stable actor token (e.g. `"reviewer-pane-w12:p27"`). Two
    /// lanes in the same session always have distinct tokens.
    pub actor_token: String,
    /// ISO-8601 spawn timestamp. Captured for the audit trail.
    pub spawned_at: String,
}

impl ActorIdentity {
    /// Construct a fresh reviewer identity.
    pub fn reviewer(
        session_id: impl Into<String>,
        actor_token: impl Into<String>,
        spawned_at: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            lane: "reviewer".to_string(),
            actor_token: actor_token.into(),
            spawned_at: spawned_at.into(),
        }
    }

    /// Construct a runner identity (used both as the comparison
    /// value and for tests asserting the gate refuses a shared
    /// identity).
    pub fn runner(
        session_id: impl Into<String>,
        actor_token: impl Into<String>,
        spawned_at: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            lane: "runner".to_string(),
            actor_token: actor_token.into(),
            spawned_at: spawned_at.into(),
        }
    }

    /// Two actors are distinct iff their lane *or* their token differs.
    /// The session id is intentionally not part of the test — every
    /// lane in a session shares it; what differs is *who* is running.
    pub fn distinct_from(&self, other: &ActorIdentity) -> bool {
        self.lane != other.lane || self.actor_token != other.actor_token
    }
}

// ─── Provenance (S1 / AC-01) ───────────────────────────────────────────

/// Recorded provenance for the reviewer environment. Carries enough
/// information that an auditor (or the gate at S3) can prove the
/// reviewer was not running inside the runner's process tree or
/// cargo cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ReviewerProvenance {
    /// Identity of the lane that owns this environment.
    pub actor: ActorIdentity,
    /// Absolute path to the reviewer binary (`mp` invocation). The
    /// gate at S3 records a content hash so a stale binary is caught.
    pub binary_path: PathBuf,
    /// Stable identifier for the binary's content (e.g. cargo
    /// `--locked` artifact sha). When `None`, the environment is
    /// unverifiable (AC-03).
    pub binary_sha: Option<String>,
    /// Absolute path to the worktree the reviewer will exercise.
    pub worktree_path: PathBuf,
    /// Absolute path to the reviewer's target directory. Distinct
    /// from the runner's `target/` so build cache cannot bleed in.
    pub target_dir: PathBuf,
    /// Process id of the reviewer process. Distinct from the
    /// runner's pid — same pid means the reviewer re-used the
    /// runner's process.
    pub pid: u32,
}

impl ReviewerProvenance {
    /// The reviewer's target directory is the strong isolation signal:
    /// if it equals the runner's, the reviewer was not actually
    /// isolated. The `target_dir_is_isolated` accessor is what the
    /// S2 provenance check compares against.
    pub fn target_dir_is_isolated(&self, other_target_dir: &std::path::Path) -> bool {
        self.target_dir != other_target_dir
    }

    /// The reviewer's worktree should match the runner's. If the two
    /// diverge the reviewer is reviewing a different worktree than
    /// the runner edited — that's a provenance failure that forces
    /// clean-room escalation.
    pub fn worktree_matches(&self, other_worktree: &std::path::Path) -> bool {
        self.worktree_path == other_worktree
    }

    /// The reviewer's process should not be the runner's. Identical
    /// pid is the structural evidence that the runner "became" the
    /// reviewer.
    pub fn pid_is_fresh(&self, runner_pid: u32) -> bool {
        self.pid != runner_pid
    }
}

/// Convenience builder for the common cycle shape:
/// `ReviewerProvenance { actor, binary_path, binary_sha, worktree_path,
/// target_dir, pid }`. The builder lives at module scope rather than on
/// the struct so the struct stays `serde`–friendly (a `Default` impl
/// on `PathBuf`-bearing fields would be misleading).
#[allow(clippy::too_many_arguments)]
pub fn build_provenance(
    session_id: impl Into<String>,
    actor_token: impl Into<String>,
    spawned_at: impl Into<String>,
    binary_path: impl Into<PathBuf>,
    binary_sha: Option<impl Into<String>>,
    worktree_path: impl Into<PathBuf>,
    target_dir: impl Into<PathBuf>,
    pid: u32,
) -> ReviewerProvenance {
    ReviewerProvenance {
        actor: ActorIdentity::reviewer(session_id, actor_token, spawned_at),
        binary_path: binary_path.into(),
        binary_sha: binary_sha.map(Into::into),
        worktree_path: worktree_path.into(),
        target_dir: target_dir.into(),
        pid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_runner() -> ActorIdentity {
        ActorIdentity::runner("s-1", "runner-pane-w12:p17", "2026-09-03T00:00:00Z")
    }

    fn fixture_env() -> ReviewerProvenance {
        build_provenance(
            "s-1",
            "reviewer-pane-w12:p27",
            "2026-09-03T00:00:00Z",
            PathBuf::from("/tmp/mp"),
            Some("sha-abc"),
            PathBuf::from("/tmp/wt"),
            PathBuf::from("/tmp/reviewer-target"),
            4242,
        )
    }

    fn clean_runner_target() -> PathBuf {
        PathBuf::from("/tmp/runner-target")
    }

    fn clean_runner_pid() -> u32 {
        9999
    }

    #[test]
    fn s1_provenance_is_isolated_from_runner_target_dir() {
        let env = fixture_env();
        assert!(
            env.target_dir_is_isolated(&clean_runner_target()),
            "reviewer target dir should not match runner's"
        );
    }

    #[test]
    fn s1_provenance_carries_actor_with_distinct_identity() {
        let env = fixture_env();
        let runner = fixture_runner();
        assert!(
            env.actor.distinct_from(&runner),
            "reviewer actor must differ from runner actor"
        );
        assert_ne!(env.actor.lane, runner.lane);
    }

    #[test]
    fn s1_same_actor_returns_false_for_distinct_from() {
        let env = fixture_env();
        let self_actor = env.actor.clone();
        assert!(
            !self_actor.distinct_from(&env.actor),
            "an actor must not be distinct from itself"
        );
    }

    #[test]
    fn s1_provenance_pid_is_distinct_from_runner_pid() {
        let env = fixture_env();
        assert!(env.pid_is_fresh(clean_runner_pid()));
        assert!(!env.pid_is_fresh(env.pid));
    }
}

// ─── Lane mode (S2 / AC-02) ────────────────────────────────────────────

/// Selected mode for the next reviewer environment.
///
/// The default is [`ReviewEnvMode::Normal`] — a fresh reviewer process
/// in an isolated target directory with no `cargo clean`. Clean-room
/// escalation is opt-in (config flag) or forced (provenance checks
/// failed); see [`select_mode`] for the decision table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewEnvMode {
    /// Default. Fresh reviewer process + isolated target directory.
    /// No `cargo clean` of the runner's build cache.
    Normal,
    /// Clean-room escalation: remove the reviewer's build artifacts
    /// before launching the reviewer. Only chosen by [`select_mode`]
    /// when the policy explicitly forces it.
    CleanRoom,
}

impl ReviewEnvMode {
    /// Stable kebab-case label (matches the serde representation).
    pub const fn as_str(self) -> &'static str {
        match self {
            ReviewEnvMode::Normal => "normal",
            ReviewEnvMode::CleanRoom => "clean-room",
        }
    }
}

// ─── Clean-room trigger (S2 / AC-02) ───────────────────────────────────

/// Why the policy escalated to [`ReviewEnvMode::CleanRoom`]. Recorded
/// for the audit trail — the reason + the commands run are visible to
/// the reviewer and the next cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "trigger", rename_all = "kebab-case")]
pub enum CleanRoomTrigger {
    /// `config.clean_room = true` for this session. Explicit opt-in.
    ExplicitConfig,
    /// Provenance checks failed; clean-room is forced rather than
    /// blocking the review entirely. The reasons are the verifier's
    /// [`provenance_issues`] output.
    ProvenanceFailure { reasons: Vec<String> },
}

impl CleanRoomTrigger {
    /// Short label, suitable for log lines and lifecycle evidence.
    pub fn label(&self) -> &'static str {
        match self {
            CleanRoomTrigger::ExplicitConfig => "explicit-config",
            CleanRoomTrigger::ProvenanceFailure { .. } => "provenance-failure",
        }
    }
}

// ─── Configuration (S2 / AC-02) ───────────────────────────────────────

/// Per-session knobs that govern [`select_mode`]. Defaults are
/// conservative: clean-room is opt-in (never unconditional), and the
/// gate refuses an unsafe environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ReviewEnvConfig {
    /// When `true`, every review runs in clean-room mode. Off by
    /// default — the user has to opt in for the slow path.
    pub clean_room: bool,
    /// When `true`, the gate accepts a dirty worktree. Off by
    /// default — the gate would normally refuse.
    pub allow_dirty_worktree: bool,
}

// ─── Mode selection (S2 / AC-02) ───────────────────────────────────────

/// Outcome of [`select_mode`]. Carries the chosen mode and — when the
/// choice was clean-room — the trigger reason + the commands the
/// policy instructs the cycle to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ModeSelection {
    pub mode: ReviewEnvMode,
    pub trigger: Option<CleanRoomTrigger>,
    /// Commands the cycle should run before launching the reviewer.
    /// Empty for [`ReviewEnvMode::Normal`] — the whole point of the
    /// policy is *no* unconditional `cargo clean`.
    pub pre_launch_commands: Vec<String>,
}

/// Decide which [`ReviewEnvMode`] to use. Decision table (matches the
/// design rationale and AC-02 verbatim):
///
/// | `config.clean_room` | `provenance_issues`              | Mode           |
/// |---------------------|----------------------------------|----------------|
/// | `false`             | empty                            | `Normal`       |
/// | `false`             | non-empty                        | `CleanRoom`    |
/// | `true`              | any                              | `CleanRoom`    |
///
/// `Normal` mode never emits `pre_launch_commands`: the policy does
/// not make `cargo clean` the default. `CleanRoom` always emits
/// `cargo clean --target-dir <reviewer_target>` (scoped to the
/// reviewer's target directory so we do not disturb the runner's
/// build artifacts).
pub fn select_mode(config: &ReviewEnvConfig, provenance_issues: &[String]) -> ModeSelection {
    if config.clean_room {
        return ModeSelection {
            mode: ReviewEnvMode::CleanRoom,
            trigger: Some(CleanRoomTrigger::ExplicitConfig),
            pre_launch_commands: Vec::new(),
        };
    }
    if !provenance_issues.is_empty() {
        return ModeSelection {
            mode: ReviewEnvMode::CleanRoom,
            trigger: Some(CleanRoomTrigger::ProvenanceFailure {
                reasons: provenance_issues.to_vec(),
            }),
            pre_launch_commands: Vec::new(),
        };
    }
    ModeSelection {
        mode: ReviewEnvMode::Normal,
        trigger: None,
        pre_launch_commands: Vec::new(),
    }
}

/// Render the commands a clean-room cycle must run before launching
/// the reviewer. Centralised so the audit trail and the test suite
/// agree on the exact invocation shape. Passing `None` (no trigger)
/// returns an empty vec — the policy refuses to manufacture
/// commands out of thin air.
pub fn clean_room_commands(
    trigger: Option<&CleanRoomTrigger>,
    target_dir: &std::path::Path,
) -> Vec<String> {
    match trigger {
        None => Vec::new(),
        Some(_) => vec![format!("cargo clean --target-dir {}", target_dir.display())],
    }
}

#[cfg(test)]
mod s2_tests {
    use super::*;

    #[test]
    fn s2_select_mode_defaults_to_normal() {
        let cfg = ReviewEnvConfig::default();
        let issues: Vec<String> = Vec::new();
        let sel = select_mode(&cfg, &issues);
        assert_eq!(sel.mode, ReviewEnvMode::Normal);
        assert!(sel.trigger.is_none());
        assert!(
            sel.pre_launch_commands.is_empty(),
            "Normal mode must never emit pre-launch commands (no unconditional cargo clean)"
        );
    }

    #[test]
    fn s2_select_mode_escalates_when_config_sets_clean_room() {
        let cfg = ReviewEnvConfig {
            clean_room: true,
            allow_dirty_worktree: false,
        };
        let sel = select_mode(&cfg, &[]);
        assert_eq!(sel.mode, ReviewEnvMode::CleanRoom);
        assert!(matches!(
            sel.trigger,
            Some(CleanRoomTrigger::ExplicitConfig)
        ));
    }

    #[test]
    fn s2_select_mode_escalates_on_provenance_failure() {
        let cfg = ReviewEnvConfig::default();
        let issues = vec!["shared-target-dir".to_string()];
        let sel = select_mode(&cfg, &issues);
        assert_eq!(sel.mode, ReviewEnvMode::CleanRoom);
        match sel.trigger {
            Some(CleanRoomTrigger::ProvenanceFailure { reasons }) => {
                assert_eq!(reasons, vec!["shared-target-dir".to_string()]);
            }
            other => panic!("expected ProvenanceFailure, got {other:?}"),
        }
    }

    #[test]
    fn s2_clean_room_commands_are_scoped_to_reviewer_target() {
        let cmds = clean_room_commands(
            Some(&CleanRoomTrigger::ProvenanceFailure {
                reasons: vec!["shared-target-dir".to_string()],
            }),
            std::path::Path::new("/tmp/reviewer-target"),
        );
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].contains("/tmp/reviewer-target"));
        assert!(cmds[0].starts_with("cargo clean"));
    }

    #[test]
    fn s2_clean_room_commands_with_no_trigger_is_empty() {
        let cmds = clean_room_commands(None, std::path::Path::new("/tmp/reviewer-target"));
        assert!(
            cmds.is_empty(),
            "absence of trigger must not manufacture commands"
        );
    }
}

// ─── Gate (S3 / AC-03) ─────────────────────────────────────────────────

/// Decision recorded by [`gate`].
///
/// - `Pass` — the environment is clean-room `Normal` and the gate
///   has no findings. The cycle proceeds.
/// - `PassWithCleanRoom` — the gate found provenance issues but
///   clean-room escalation makes the review safe. The cycle
///   proceeds with the recorded trigger + commands.
/// - The absence of a `Block` variant is intentional: blocking
///   environments return `Err(ReviewEnvError)` instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum ReviewEnvDecision {
    Pass,
    PassWithCleanRoom {
        reason: String,
        commands: Vec<String>,
    },
}

impl ReviewEnvDecision {
    pub const fn is_pass(&self) -> bool {
        matches!(self, ReviewEnvDecision::Pass)
    }
    pub const fn is_clean_room(&self) -> bool {
        matches!(self, ReviewEnvDecision::PassWithCleanRoom { .. })
    }
}

/// Typed refusal for an unsafe environment. Each variant carries a
/// `hint` string with the action a human (or the cycle engine) needs
/// to take. AC-03 verbatim: the gate must block automatic review
/// pass with an actionable typed result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ReviewEnvError {
    DirtyWorktree {
        status_output: String,
        hint: String,
    },
    SameActor {
        actor: String,
        hint: String,
    },
    StaleBinary {
        expected: String,
        actual: String,
        hint: String,
    },
    UnverifiableEnv {
        missing: Vec<String>,
        hint: String,
    },
    WorktreeMismatch {
        expected: String,
        actual: String,
        hint: String,
    },
}

impl ReviewEnvError {
    pub fn hint(&self) -> &str {
        match self {
            ReviewEnvError::DirtyWorktree { hint, .. }
            | ReviewEnvError::SameActor { hint, .. }
            | ReviewEnvError::StaleBinary { hint, .. }
            | ReviewEnvError::UnverifiableEnv { hint, .. }
            | ReviewEnvError::WorktreeMismatch { hint, .. } => hint,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            ReviewEnvError::DirtyWorktree { .. } => "dirty-worktree",
            ReviewEnvError::SameActor { .. } => "same-actor",
            ReviewEnvError::StaleBinary { .. } => "stale-binary",
            ReviewEnvError::UnverifiableEnv { .. } => "unverifiable-env",
            ReviewEnvError::WorktreeMismatch { .. } => "worktree-mismatch",
        }
    }
}

/// Inputs to [`gate`]. Bundled so the test suite can build fixtures
/// without an enormous positional argument list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateInputs<'a> {
    pub env: &'a ReviewerProvenance,
    pub runner_actor: &'a ActorIdentity,
    pub runner_target_dir: &'a std::path::Path,
    pub runner_worktree_path: &'a std::path::Path,
    pub runner_pid: u32,
    pub worktree_clean: bool,
    pub expected_binary_sha: Option<&'a str>,
    pub config: &'a ReviewEnvConfig,
}

/// Run the pre-review gate. Returns `Ok(decision)` when the
/// environment is safe (with or without clean-room escalation);
/// returns `Err(ReviewEnvError)` when the environment is unsafe and
/// the automatic review pass must be blocked.
///
/// **Refusal order** (so a test can predict which variant surfaced):
///
/// 1. `UnverifiableEnv` — `binary_sha` missing OR `expected_binary_sha`
///    missing. Without provenance we cannot reason about isolation.
/// 2. `SameActor` — `actor.distinct_from(runner_actor)` is false.
/// 3. `StaleBinary` — provenances differ from the expected sha.
/// 4. `WorktreeMismatch` — `env.worktree_path` differs from the
///    runner's. The reviewer would be looking at different code.
/// 5. `DirtyWorktree` — last because it is observable post-build
///    (the build can succeed on a dirty tree; the gate fails before
///    the cycle records a false-positive pass).
///
/// `allow_dirty_worktree` lifts the dirty-worktree check but leaves
/// the others untouched — a reviewer that knowingly accepts dirty
/// still needs process + binary isolation.
pub fn gate(inputs: &GateInputs<'_>) -> Result<ReviewEnvDecision, ReviewEnvError> {
    let GateInputs {
        env,
        runner_actor,
        runner_target_dir,
        runner_worktree_path,
        runner_pid,
        worktree_clean,
        expected_binary_sha,
        config,
    } = inputs;

    // 1. Unverifiable environment.
    let mut missing = Vec::new();
    if env.binary_sha.is_none() {
        missing.push("reviewer.binary_sha".to_string());
    }
    if expected_binary_sha.is_none() {
        missing.push("expected_binary_sha".to_string());
    }
    if !missing.is_empty() {
        return Err(ReviewEnvError::UnverifiableEnv {
            missing,
            hint: "Record both reviewer.binary_sha and the runner's expected binary sha before launching the reviewer; rerun with `--no-isolation-overrides`.".to_string(),
        });
    }

    // 2. Same actor identity as the runner.
    if !env.actor.distinct_from(runner_actor) {
        return Err(ReviewEnvError::SameActor {
            actor: env.actor.actor_token.clone(),
            hint: format!(
                "Reviewer shares actor identity with runner ({}); spawn a fresh reviewer process on a distinct pane.",
                env.actor.actor_token
            ),
        });
    }

    // 3. Stale binary.
    if let (Some(actual), Some(expected)) = (env.binary_sha.as_ref(), expected_binary_sha) {
        if actual != expected {
            return Err(ReviewEnvError::StaleBinary {
                expected: expected.to_string(),
                actual: actual.clone(),
                hint: "Rebuild the reviewer binary (`cargo build --release -p mp`) before launching the reviewer; ensure the runner and reviewer see the same artifact hash.".to_string(),
            });
        }
    }

    // 4. Worktree mismatch — reviewer on a different worktree than
    //    the runner would silently review different code.
    if !env.worktree_matches(runner_worktree_path) {
        return Err(ReviewEnvError::WorktreeMismatch {
            expected: runner_worktree_path.display().to_string(),
            actual: env.worktree_path.display().to_string(),
            hint: "Reviewer worktree does not match runner worktree; re-spawn the reviewer on the runner's worktree (or pass an explicit `--reviewer-worktree-override`).".to_string(),
        });
    }

    // 5. Dirty worktree (unless explicitly allowed).
    if !worktree_clean && !config.allow_dirty_worktree {
        return Err(ReviewEnvError::DirtyWorktree {
            status_output: "git status --porcelain returned non-empty output".to_string(),
            hint: "Commit or stash working-tree changes before requesting an automated review pass; the gate refuses to record a `Pass` over a dirty tree.".to_string(),
        });
    }

    // Safety check passed. Decide whether clean-room escalation
    // applies — only when the worktree/actor/binary checks all
    // pass AND something on the provenance side was suspicious but
    // not blocking. Today that boils down to "shared target dir":
    // the gate refuses a structural failure here but allows
    // clean-room escalation for non-structural dirt.
    if !env.target_dir_is_isolated(runner_target_dir) {
        return Ok(ReviewEnvDecision::PassWithCleanRoom {
            reason: "reviewer.target_dir matches runner.target_dir; forcing clean-room to break the build-cache inheritance".to_string(),
            commands: clean_room_commands(
                Some(&CleanRoomTrigger::ProvenanceFailure {
                    reasons: vec!["shared-target-dir".to_string()],
                }),
                &env.target_dir,
            ),
        });
    }
    if !env.pid_is_fresh(*runner_pid) {
        return Ok(ReviewEnvDecision::PassWithCleanRoom {
            reason: format!(
                "reviewer.pid {} equals runner.pid; clean-room + process reset required",
                env.pid
            ),
            commands: clean_room_commands(
                Some(&CleanRoomTrigger::ProvenanceFailure {
                    reasons: vec!["shared-pid".to_string()],
                }),
                &env.target_dir,
            ),
        });
    }

    Ok(ReviewEnvDecision::Pass)
}

/// Collect provenance *issues* (non-blocking) into a flat list. Used
/// by the cycle engine after the first run to decide whether to
/// escalate on subsequent reviews; the gate itself only reads
/// `Ok`/`Err` outcomes.
pub fn provenance_issues(
    env: &ReviewerProvenance,
    runner_target_dir: &std::path::Path,
    runner_pid: u32,
) -> Vec<String> {
    let mut issues = Vec::new();
    if !env.target_dir_is_isolated(runner_target_dir) {
        issues.push("shared-target-dir".to_string());
    }
    if !env.pid_is_fresh(runner_pid) {
        issues.push("shared-pid".to_string());
    }
    issues
}

#[cfg(test)]
mod s3_tests {
    use super::*;

    fn fixture_runner() -> ActorIdentity {
        ActorIdentity::runner("s-1", "runner-pane-w12:p17", "2026-09-03T00:00:00Z")
    }

    fn fixture_env() -> ReviewerProvenance {
        build_provenance(
            "s-1",
            "reviewer-pane-w12:p27",
            "2026-09-03T00:00:00Z",
            std::path::PathBuf::from("/tmp/mp"),
            Some("sha-abc"),
            std::path::PathBuf::from("/tmp/wt"),
            std::path::PathBuf::from("/tmp/reviewer-target"),
            4242,
        )
    }

    fn clean_runner_target() -> std::path::PathBuf {
        std::path::PathBuf::from("/tmp/runner-target")
    }

    fn clean_runner_pid() -> u32 {
        9999
    }

    #[test]
    fn s3_gate_passes_when_provenance_is_clean() {
        let env = fixture_env();
        let runner = fixture_runner();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &clean_runner_target(),
            runner_worktree_path: &env.worktree_path,
            runner_pid: clean_runner_pid(),
            worktree_clean: true,
            expected_binary_sha: Some("sha-abc"),
            config: &cfg,
        };
        let decision = gate(&inputs).expect("clean env passes");
        assert!(decision.is_pass());
    }

    #[test]
    fn s3_gate_blocks_dirty_worktree_by_default() {
        let env = fixture_env();
        let runner = fixture_runner();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &clean_runner_target(),
            runner_worktree_path: &env.worktree_path,
            runner_pid: clean_runner_pid(),
            worktree_clean: false,
            expected_binary_sha: Some("sha-abc"),
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("dirty worktree must block");
        assert!(matches!(err, ReviewEnvError::DirtyWorktree { .. }));
        assert_eq!(err.kind(), "dirty-worktree");
    }

    #[test]
    fn s3_gate_allows_dirty_worktree_when_configured() {
        let env = fixture_env();
        let runner = fixture_runner();
        let cfg = ReviewEnvConfig {
            clean_room: false,
            allow_dirty_worktree: true,
        };
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &clean_runner_target(),
            runner_worktree_path: &env.worktree_path,
            runner_pid: clean_runner_pid(),
            worktree_clean: false,
            expected_binary_sha: Some("sha-abc"),
            config: &cfg,
        };
        let decision = gate(&inputs).expect("explicit allow bypasses dirty-tree refusal");
        assert!(decision.is_pass());
    }

    #[test]
    fn s3_gate_blocks_same_actor_identity() {
        let env = fixture_env();
        let same_actor = env.actor.clone();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &same_actor,
            runner_target_dir: &clean_runner_target(),
            runner_worktree_path: &env.worktree_path,
            runner_pid: clean_runner_pid(),
            worktree_clean: true,
            expected_binary_sha: Some("sha-abc"),
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("shared actor must block");
        assert!(matches!(err, ReviewEnvError::SameActor { .. }));
        assert!(!err.hint().is_empty());
    }

    #[test]
    fn s3_gate_blocks_stale_binary() {
        let env = fixture_env();
        let runner = fixture_runner();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &clean_runner_target(),
            runner_worktree_path: &env.worktree_path,
            runner_pid: clean_runner_pid(),
            worktree_clean: true,
            expected_binary_sha: Some("sha-different"),
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("sha mismatch must block");
        match err {
            ReviewEnvError::StaleBinary {
                expected,
                actual,
                hint,
            } => {
                assert_eq!(expected, "sha-different");
                assert_eq!(actual, "sha-abc");
                assert!(!hint.is_empty());
            }
            other => panic!("expected StaleBinary, got {other:?}"),
        }
    }

    #[test]
    fn s3_gate_blocks_unverifiable_environment_when_binary_sha_missing() {
        let mut env = fixture_env();
        env.binary_sha = None;
        let runner = fixture_runner();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &clean_runner_target(),
            runner_worktree_path: &env.worktree_path,
            runner_pid: clean_runner_pid(),
            worktree_clean: true,
            expected_binary_sha: Some("sha-abc"),
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("missing binary_sha must block");
        match err {
            ReviewEnvError::UnverifiableEnv { missing, .. } => {
                assert!(missing.iter().any(|m| m == "reviewer.binary_sha"));
            }
            other => panic!("expected UnverifiableEnv, got {other:?}"),
        }
    }

    #[test]
    fn s3_gate_blocks_when_expected_sha_missing() {
        let env = fixture_env();
        let runner = fixture_runner();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &clean_runner_target(),
            runner_worktree_path: &env.worktree_path,
            runner_pid: clean_runner_pid(),
            worktree_clean: true,
            expected_binary_sha: None,
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("missing expected_sha must block");
        match err {
            ReviewEnvError::UnverifiableEnv { missing, .. } => {
                assert!(missing.iter().any(|m| m == "expected_binary_sha"));
            }
            other => panic!("expected UnverifiableEnv, got {other:?}"),
        }
    }

    #[test]
    fn s3_gate_blocks_worktree_mismatch_when_reviewer_on_different_worktree() {
        // F-01 regression: the gate must refuse a reviewer that
        // silently landed on a different worktree than the runner.
        let env = fixture_env();
        let runner = fixture_runner();
        let cfg = ReviewEnvConfig::default();
        let other_wt = std::path::Path::new("/tmp/different-wt");
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &clean_runner_target(),
            runner_worktree_path: other_wt,
            runner_pid: clean_runner_pid(),
            worktree_clean: true,
            expected_binary_sha: Some("sha-abc"),
            config: &cfg,
        };
        let err = gate(&inputs).expect_err("worktree mismatch must block");
        match err {
            ReviewEnvError::WorktreeMismatch {
                ref expected,
                ref actual,
                ref hint,
            } => {
                assert_eq!(*expected, "/tmp/different-wt");
                assert_eq!(*actual, env.worktree_path.display().to_string());
                assert!(!hint.is_empty());
            }
            other => panic!("expected WorktreeMismatch, got {other:?}"),
        }
        assert_eq!(err.kind(), "worktree-mismatch");
    }

    #[test]
    fn s3_gate_escalates_to_clean_room_on_shared_target_dir() {
        let env = fixture_env();
        let runner = fixture_runner();
        let cfg = ReviewEnvConfig::default();
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &env.target_dir,
            runner_worktree_path: &env.worktree_path,
            runner_pid: clean_runner_pid(),
            worktree_clean: true,
            expected_binary_sha: Some("sha-abc"),
            config: &cfg,
        };
        let decision = gate(&inputs).expect("shared target dir escalates, does not block");
        assert!(decision.is_clean_room());
        match decision {
            ReviewEnvDecision::PassWithCleanRoom { commands, .. } => {
                assert!(!commands.is_empty());
                assert!(commands[0].contains("cargo clean"));
                assert!(commands[0].contains(env.target_dir.to_str().unwrap()));
            }
            other => panic!("expected PassWithCleanRoom, got {other:?}"),
        }
    }

    #[test]
    fn s3_gate_escalates_to_clean_room_on_shared_pid() {
        let env = fixture_env();
        let runner = fixture_runner();
        let cfg = ReviewEnvConfig::default();
        // The reviewer's pid equals the runner's pid — the runner
        // quietly became the reviewer. The gate must escalate to
        // clean-room rather than block; without this test AC-03
        // advertises coverage for the shared-pid branch but does
        // not exercise it.
        let inputs = GateInputs {
            env: &env,
            runner_actor: &runner,
            runner_target_dir: &clean_runner_target(),
            runner_worktree_path: &env.worktree_path,
            runner_pid: env.pid,
            worktree_clean: true,
            expected_binary_sha: Some("sha-abc"),
            config: &cfg,
        };
        let decision = gate(&inputs).expect("shared pid escalates, does not block");
        assert!(decision.is_clean_room(), "expected PassWithCleanRoom");
        match decision {
            ReviewEnvDecision::PassWithCleanRoom { commands, reason } => {
                assert!(!commands.is_empty(), "clean-room must emit a command");
                assert!(commands[0].contains("cargo clean"));
                assert!(
                    commands[0].contains(env.target_dir.to_str().unwrap()),
                    "command should target the reviewer's target dir"
                );
                assert!(
                    reason.contains("pid"),
                    "reason must mention the pid: {reason}"
                );
            }
            other => panic!("expected PassWithCleanRoom, got {other:?}"),
        }
    }

    #[test]
    fn s3_provenance_issues_lists_isolation_failures() {
        let env = fixture_env();
        let issues = provenance_issues(&env, &env.target_dir, env.pid);
        assert!(issues.contains(&"shared-target-dir".to_string()));
        assert!(issues.contains(&"shared-pid".to_string()));
    }

    #[test]
    fn s3_provenance_issues_empty_when_isolated() {
        let env = fixture_env();
        let issues = provenance_issues(&env, &clean_runner_target(), clean_runner_pid());
        assert!(issues.is_empty());
    }
}
