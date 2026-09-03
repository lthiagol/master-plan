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
