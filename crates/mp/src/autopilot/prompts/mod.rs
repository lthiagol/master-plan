//! M210: per-role spawn-prompt templates for `mp autopilot`.
//!
//! Every role in the autopilot cycle boots from a session-bound
//! system prompt. The renderer is intentionally a pure function:
//! same `RoleConfig` + session.json slice → byte-identical prompt.
//! The byte-stability is asserted by the golden fixtures in
//! `tests/spawn_prompt_golden.rs` (AC-01) and by the substring
//! pins in `tests/spawn_prompt_contains.rs` (AC-02).
//!
//! The templates live here, not in `templates/skills/*/SKILL.md`.
//! Per the locked decision (DD-01 in the spec): SKILL.md is a
//! teaching layer that drifts; the prompt is enforcement. Updating
//! the role's `Boundaries you must respect` block requires a code
//! commit and a verifier re-pin.
//!
//! ## Why hardcoded strings
//!
//! - Render determinism. A `format!` with conditional sections can
//!   silently drift if a new field is added to `RoleConfig`; the
//!   golden tests catch that.
//! - Auditability. Operators read the role prompt by reading this
//!   file. No interpolation layer to interpret.
//!
//! ## Topology collapsing
//!
//! In 2-pane topology the supervisor pane carries both
//! Orchestrator and Reviewer responsibilities — the render layer
//! concatenates those two role prompts with a named seam. In
//! 1-pane every role is collapsed into one bundle, ordered by
//! role-of-responsibility (Orchestrator → Runner → Reviewer) with
//! the same seam pattern. The seam is the agent's only cue for
//! "you're in O-mode" vs "you're in V-mode" inside a collapsed
//! bundle.

pub mod spawn;
