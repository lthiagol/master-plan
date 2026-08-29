//! M197 WP1 / AC-01: harness auto-detection.
//!
//! `mp init` and the `mp watch` precondition gate both need to know
//! which harnesses have all three CPD base skills installed
//! (`mp-flow` / `mp-runner` / `mp-coordinator`). The harness auto-set
//! logic only fires when exactly one harness matches AND both
//! `[agent.runner].harness` and `[agent.coordinator].harness` are
//! unset — a multi-harness project gets the ambiguity surfaced in
//! `mp doctor` instead of silently picking one.
//!
//! This module is the single source for the "which harness(es) are
//! fully installed" query. It mirrors the per-harness
//! `skill_installed` calculation that `mp doctor` already uses, so
//! the auto-set path and the doctor check cannot disagree on what
//! "installed" means.
//!
//! Layering:
//! - Pure: [`detect_installed_harnesses`] — scans the global
//!   `~/.agents/skills/<id>/SKILL.md` (or per-harness override) and
//!   returns the harness ids that have all three CPD skills.
//! - Pure: [`auto_set_target`] — given the current
//!   `(runner_harness, coordinator_harness)` and the installed
//!   harness list, decide whether to auto-set (and to which harness),
//!   surface the ambiguity, or do nothing. The result is an enum
//!   so the caller can route on the intent (silent auto-set vs.
//!   surface-ambiguity) without re-implementing the rules.

/// The three CPD base skills that must all be present under a
/// harness's global skill dir for that harness to count as
/// "installed" for auto-set purposes. Kept in lockstep with the
/// `check_harnesses()` list in `crate::doctor` so the two surfaces
/// cannot drift.
pub const CPD_BASE_SKILLS: &[&str] = &["mp-flow", "mp-runner", "mp-coordinator"];

/// What the auto-set layer decided for the (runner, coordinator,
/// installed-harness-set) input. Callers render the decision in
/// `mp init` (silent auto-set), `mp watch` precondition gate (lazy
/// fallback before spawn), and `mp doctor` (visibility).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoSetDecision {
    /// Both role harnesses are already set; nothing to do.
    NoOp,
    /// Exactly one installed harness + both role harnesses unset.
    /// Caller should auto-set both roles to `harness` and surface
    /// the auto-set in its report.
    AutoSet { harness: String },
    /// Multiple installed harnesses + both role harnesses unset.
    /// Caller should NOT auto-pick; instead surface the ambiguity
    /// (the list of installed harnesses) so the operator chooses.
    Ambiguous { installed: Vec<String> },
}

/// Scan the harness registry and return the harness ids whose
/// global skill dir contains all three CPD base skills. Pure over
/// the filesystem; no side effects. The order is the registry order
/// (`default_registry()`) so the ambiguity message is deterministic.
pub fn detect_installed_harnesses() -> Vec<String> {
    crate::harness::default_registry()
        .into_iter()
        .filter(is_harness_fully_installed)
        .map(|h| h.id.to_string())
        .collect()
}

/// True when every CPD base skill in [`CPD_BASE_SKILLS`] has a
/// `SKILL.md` under the harness's resolved global skill dir. Honors
/// `MP_<HARNESS>_SKILL_DIR` overrides via
/// [`crate::harness::resolved_global_skill_dir`].
pub fn is_harness_fully_installed(h: &crate::harness::HarnessDescriptor) -> bool {
    let dir = crate::harness::resolved_global_skill_dir(h);
    CPD_BASE_SKILLS
        .iter()
        .all(|s| dir.join(s).join("SKILL.md").is_file())
}

/// Decide whether to auto-set the agent role harnesses given the
/// current config and the set of fully-installed harnesses. The
/// rules (per M197 WP1 / AC-01):
///
/// 1. If either `agent.runner.harness` or `agent.coordinator.harness`
///    is already set, do nothing (the operator already chose).
/// 2. If exactly one harness is fully installed, auto-set both roles
///    to that harness.
/// 3. If zero harnesses are fully installed, do nothing (the
///    preconditions will surface a clearer error downstream).
/// 4. If multiple harnesses are fully installed, do not pick —
///    return [`AutoSetDecision::Ambiguous`] so the caller surfaces
///    the installed list in `mp doctor` (or the watch precondition
///    gate) for the operator to resolve.
pub fn auto_set_target(
    runner_harness: Option<&str>,
    coordinator_harness: Option<&str>,
    installed: &[String],
) -> AutoSetDecision {
    if runner_harness.is_some() || coordinator_harness.is_some() {
        return AutoSetDecision::NoOp;
    }
    match installed.len() {
        0 => AutoSetDecision::NoOp,
        1 => AutoSetDecision::AutoSet {
            harness: installed[0].clone(),
        },
        _ => AutoSetDecision::Ambiguous {
            installed: installed.to_vec(),
        },
    }
}

/// M197 F-09: apply an [`AutoSetDecision`] to the two role-harness
/// slots. Single source of truth shared by the `mp init` template
/// path (which writes through raw `serde_json::Value` ops because the
/// project is being bootstrapped from a profile JSON) and the lazy
/// `try_lazy_auto_set` fallback (which writes through the typed
/// [`ProjectConfig`] struct after `store::load_config`). Without
/// this helper the two sites drift whenever a new
/// [`crate::config::RoleConfig`] field is added (L6 + L9).
///
/// Mutates `runner` and `coordinator` in place when the decision is
/// [`AutoSetDecision::AutoSet`]; leaves them alone otherwise.
pub fn apply_auto_set_decision(
    runner: &mut Option<String>,
    coordinator: &mut Option<String>,
    decision: &AutoSetDecision,
) {
    if let AutoSetDecision::AutoSet { harness } = decision {
        *runner = Some(harness.clone());
        *coordinator = Some(harness.clone());
    }
}

/// M197 F-10: render the structured parts of an [`AutoSetDecision`]
/// (the harness slot values, the installed list, or a "no harness"
/// marker) as a short, surface-neutral label. Shared between
/// `mp doctor` (DoctorCheck.message) and the `mp watch` precondition
/// gate (PreconditionCheck.message) so the two surfaces cannot drift
/// on what the decision actually carries — each surface wraps the
/// label with its own sentence shape (the doctor surface writes
/// a full sentence; the precondition surface writes a tagged
/// `harness auto-set: …` line).
pub fn decision_label(
    runner: Option<&str>,
    coordinator: Option<&str>,
    decision: &AutoSetDecision,
) -> String {
    match decision {
        AutoSetDecision::NoOp => {
            if runner.is_some() || coordinator.is_some() {
                format!(
                    "runner={} coordinator={}",
                    runner.unwrap_or("(unset)"),
                    coordinator.unwrap_or("(unset)")
                )
            } else {
                "no fully-installed harness detected".to_string()
            }
        }
        AutoSetDecision::AutoSet { harness } => {
            format!("would set runner + coordinator to '{harness}'")
        }
        AutoSetDecision::Ambiguous { installed } => {
            format!(
                "ambiguous — multiple installed harnesses [{}]",
                installed.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_set_target_skips_when_both_roles_set() {
        let installed = vec!["opencode".to_string()];
        let decision = auto_set_target(Some("opencode"), Some("opencode"), &installed);
        assert_eq!(decision, AutoSetDecision::NoOp);
    }

    #[test]
    fn auto_set_target_skips_when_only_one_role_set() {
        let installed = vec!["opencode".to_string()];
        // Even with one installed harness, a single set role
        // signals "operator already chose" — leave the other alone.
        let decision = auto_set_target(Some("opencode"), None, &installed);
        assert_eq!(decision, AutoSetDecision::NoOp);
    }

    #[test]
    fn auto_set_target_picks_singleton_installed() {
        let installed = vec!["opencode".to_string()];
        let decision = auto_set_target(None, None, &installed);
        assert_eq!(
            decision,
            AutoSetDecision::AutoSet {
                harness: "opencode".into()
            }
        );
    }

    #[test]
    fn auto_set_target_noop_when_nothing_installed() {
        let installed: Vec<String> = vec![];
        let decision = auto_set_target(None, None, &installed);
        assert_eq!(decision, AutoSetDecision::NoOp);
    }

    #[test]
    fn auto_set_target_flags_ambiguity_on_multiple_installs() {
        let installed = vec!["opencode".to_string(), "pi".to_string()];
        let decision = auto_set_target(None, None, &installed);
        assert_eq!(
            decision,
            AutoSetDecision::Ambiguous {
                installed: vec!["opencode".into(), "pi".into()]
            }
        );
    }

    #[test]
    fn cpd_base_skills_pin_the_three_skill_ids() {
        // The auto-set contract depends on the same CPD triad that
        // `mp doctor` uses for `skill_installed`. Drift between the
        // two surfaces would silently change what "installed" means.
        assert_eq!(CPD_BASE_SKILLS, &["mp-flow", "mp-runner", "mp-coordinator"]);
    }

    #[test]
    fn apply_auto_set_decision_writes_on_autoset_only() {
        let mut runner = None;
        let mut coord = None;
        apply_auto_set_decision(
            &mut runner,
            &mut coord,
            &AutoSetDecision::AutoSet {
                harness: "opencode".into(),
            },
        );
        assert_eq!(runner.as_deref(), Some("opencode"));
        assert_eq!(coord.as_deref(), Some("opencode"));

        // NoOp / Ambiguous must NOT mutate the slots — F-09.
        let mut runner = Some("pi".into());
        let mut coord = Some("pi".into());
        apply_auto_set_decision(&mut runner, &mut coord, &AutoSetDecision::NoOp);
        assert_eq!(runner.as_deref(), Some("pi"));
        assert_eq!(coord.as_deref(), Some("pi"));

        apply_auto_set_decision(
            &mut runner,
            &mut coord,
            &AutoSetDecision::Ambiguous {
                installed: vec!["opencode".into(), "pi".into()],
            },
        );
        assert_eq!(runner.as_deref(), Some("pi"));
        assert_eq!(coord.as_deref(), Some("pi"));
    }

    #[test]
    fn decision_label_covers_all_three_variants() {
        // F-10: every variant of `AutoSetDecision` renders a
        // surface-neutral label so `mp doctor` and the `mp watch`
        // precondition gate cannot drift their wording on the
        // same decision.
        let autoset = AutoSetDecision::AutoSet {
            harness: "opencode".into(),
        };
        assert_eq!(
            decision_label(None, None, &autoset),
            "would set runner + coordinator to 'opencode'"
        );

        let noop = AutoSetDecision::NoOp;
        assert_eq!(
            decision_label(Some("pi"), Some("pi"), &noop),
            "runner=pi coordinator=pi"
        );
        assert_eq!(
            decision_label(None, None, &noop),
            "no fully-installed harness detected"
        );

        let ambig = AutoSetDecision::Ambiguous {
            installed: vec!["opencode".into(), "pi".into()],
        };
        assert_eq!(
            decision_label(None, None, &ambig),
            "ambiguous — multiple installed harnesses [opencode, pi]"
        );
    }
}
