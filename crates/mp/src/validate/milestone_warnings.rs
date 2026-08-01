use crate::model::{MilestoneFile, Step};

use super::report::issue;
use super::report::ValidationIssue;

pub(crate) fn validate_milestone(m: &MilestoneFile, warnings: &mut Vec<ValidationIssue>) {
    let id = m.milestone.id.clone();
    if m.intent.outcome.is_empty() && m.milestone.spec_status != "draft" {
        warnings.push(issue("W10", "intent.outcome is empty", Some(id.clone())));
    }
    for dep in &m.milestone.depends_on {
        if dep != "none" && !dep.is_empty() {
            // dependency existence checked elsewhere
            let _ = dep;
        }
    }

    for step in &m.steps {
        if step.done_when.trim().is_empty() {
            warnings.push(issue(
                "W40",
                &format!("step {} has empty done_when", step.id),
                Some(id.clone()),
            ));
        }
    }

    for step in &m.steps {
        let val = step.tests.trim();
        if val == "manual: accepted" {
            warnings.push(issue(
                "W41",
                &format!(
                    "step {} tests is 'manual: accepted' with no justification reason",
                    step.id
                ),
                Some(id.clone()),
            ));
        }
    }
    for ac in &m.acceptance_criteria {
        let val = ac.verification.trim();
        if val == "manual: accepted" {
            warnings.push(issue(
                "W41",
                &format!(
                    "AC {} verification is 'manual: accepted' with no justification reason",
                    ac.id
                ),
                Some(id.clone()),
            ));
        }
    }

    if matches!(m.milestone.risk.as_str(), "medium" | "high") && m.design_decisions.is_empty() {
        warnings.push(issue(
            "W42",
            "milestone with risk=medium/high has no design decisions",
            Some(id.clone()),
        ));
    }
}

pub(crate) fn step_deps_satisfied(step: &Step, all: &[Step]) -> bool {
    step.depends_on_steps.iter().all(|dep| {
        all.iter()
            .find(|s| s.id == *dep)
            .map(|s| s.status == "done" || s.status == "skipped")
            .unwrap_or(false)
    })
}

/// Validate that a verification/tests field follows the AGENTS.md convention:
/// single path/command, or `manual: accepted - <reason>`.
pub(crate) fn validate_verification_field(
    value: &str,
    warnings: &mut Vec<ValidationIssue>,
    milestone: Option<String>,
    label: &str,
) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    // `manual:` values are human descriptions, not executed by the completion
    // guardrail, so a comma there is natural prose — not a multi-value command.
    if trimmed.starts_with("manual:") {
        return;
    }
    if trimmed.contains(',') {
        warnings.push(issue(
            "W20",
            &format!("{label} contains multiple values (use single path/command)"),
            milestone,
        ));
    }
}

/// W43: step action or AC references a non-existent milestone/step id.
///
/// After M100+ the validator is archive-aware (consults `load_all_milestones`
/// ∪ `list_archived_milestones`) and resolves cross-milestone step references
/// like "M106 S11" in an AC description by pairing the `M` and `S` tokens
/// and resolving the `S` against the *referenced* milestone's step_ids
/// (remediation 2026-07-05; the prior version silently skipped S-refs in
/// AC descriptions, which left true typos invisible). Standalone S-refs
/// without an M-prefix are checked against the current milestone's own
/// step_ids, as before.
pub(crate) fn validate_cross_refs(
    m: &MilestoneFile,
    all_milestone_ids: &std::collections::HashSet<String>,
    step_ids_by_milestone: &std::collections::HashMap<String, std::collections::HashSet<String>>,
    warnings: &mut Vec<ValidationIssue>,
) {
    let id = m.milestone.id.clone();
    let my_step_ids: std::collections::HashSet<String> =
        m.steps.iter().map(|s| s.id.clone()).collect();
    for step in &m.steps {
        check_text_refs(
            &format!("step {} action", step.id),
            Some(id.clone()),
            &step.action,
            &my_step_ids,
            all_milestone_ids,
            step_ids_by_milestone,
            warnings,
        );
    }
    for ac in &m.acceptance_criteria {
        check_text_refs(
            &format!("AC {} description", ac.id),
            Some(id.clone()),
            &ac.description,
            &my_step_ids,
            all_milestone_ids,
            step_ids_by_milestone,
            warnings,
        );
    }
}

/// Walk a text string, classify each token as M-ref or S-ref, and emit
/// W43 warnings for any reference that doesn't resolve. The
/// remediation (2026-07-05) added (M, S) pair resolution so that
/// cross-milestone step references like "M106 S11" resolve the S against
/// M106's step_ids (if M106 exists in the index), and only emit W43
/// when *neither* side resolves.
fn check_text_refs(
    label: &str,
    milestone_id: Option<String>,
    text: &str,
    my_step_ids: &std::collections::HashSet<String>,
    all_milestone_ids: &std::collections::HashSet<String>,
    step_ids_by_milestone: &std::collections::HashMap<String, std::collections::HashSet<String>>,
    warnings: &mut Vec<ValidationIssue>,
) {
    // First pass: collect ref tokens in document order with their kind.
    // We group adjacent (M, S) pairs because that's the only context
    // where a step ID has a non-self milestone to resolve against.
    let mut tokens: Vec<(usize, String)> = Vec::new(); // (offset, ref_id)
    let mut cursor = 0usize;
    for word in text.split_whitespace() {
        // Find the word's offset in `text` to preserve adjacency info.
        let offset = match text[cursor..].find(word) {
            Some(o) => cursor + o,
            None => cursor,
        };
        for r in extract_milestone_refs(word) {
            tokens.push((offset, r));
        }
        cursor = offset + word.len();
    }

    // Walk tokens, pairing each S with the most recent M.
    let mut last_m: Option<String> = None;
    let mut pair_index: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    for (i, (off, r)) in tokens.iter().enumerate() {
        if r.starts_with('M') {
            last_m = Some(r.clone());
        } else if r.starts_with('S') {
            if let Some(mref) = &last_m {
                pair_index.insert(i, mref.clone());
            }
        }
        let _ = off;
    }

    for (i, (_, ref_id)) in tokens.iter().enumerate() {
        if ref_id.starts_with('M') {
            if !all_milestone_ids.contains(ref_id) {
                warnings.push(issue(
                    "W43",
                    &format!("{label} references non-existent milestone {ref_id}"),
                    milestone_id.clone(),
                ));
            }
        } else if ref_id.starts_with('S') {
            // If paired with an M-ref, resolve against that milestone's
            // step_ids. Otherwise, check against this milestone's own.
            let paired_m: Option<&str> = pair_index.get(&i).map(String::as_str);
            let resolved = match paired_m {
                Some(mref) => step_ids_by_milestone
                    .get(mref)
                    .map(|s| s.contains(ref_id.as_str()))
                    .unwrap_or(false),
                None => my_step_ids.contains(ref_id.as_str()),
            };
            if !resolved {
                let target = paired_m
                    .map(|m| format!("step {ref_id} of {m}"))
                    .unwrap_or_else(|| format!("step {ref_id}"));
                warnings.push(issue(
                    "W43",
                    &format!("{label} references non-existent {target}"),
                    milestone_id.clone(),
                ));
            }
        }
    }
}

fn extract_milestone_refs(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|word| {
            let cleaned: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
            if cleaned.len() >= 2 && cleaned.starts_with('M') {
                let rest: String = cleaned.chars().skip(1).collect();
                if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                    return Some(format!("M{}", rest));
                }
            }
            if cleaned.len() >= 2 && cleaned.starts_with('S') {
                let rest: String = cleaned.chars().skip(1).collect();
                if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    return Some(cleaned.clone());
                }
            }
            None
        })
        .collect()
}

pub(crate) fn uncovered_acceptance_criteria(m: &MilestoneFile) -> Vec<String> {
    let covered: std::collections::HashSet<String> =
        m.steps.iter().flat_map(|s| s.covers_ac.clone()).collect();
    m.acceptance_criteria
        .iter()
        .map(|ac| ac.id.clone())
        .filter(|id| !covered.contains(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AcceptanceCriterion, MilestoneFile, MilestoneMeta, Step};

    fn fixture_with_m_ref(mref: &str) -> MilestoneFile {
        MilestoneFile {
            milestone: MilestoneMeta {
                id: "112".into(),
                title: "test".into(),
                slug: "test".into(),
                lifecycle: "approved".into(),
                ..Default::default()
            },
            acceptance_criteria: vec![AcceptanceCriterion {
                id: "AC-01".into(),
                description: format!("references {mref} for historical context"),
                verification: "manual: accepted".into(),
                status: "pending".into(),
                evidence: String::new(),
            }],
            steps: vec![Step {
                id: "S1".into(),
                action: format!("touch {mref}"),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// W43 cross-ref validator: archived milestone references are valid in
    /// AC descriptions and step actions. Pre-fix this fired 4 false-positive
    /// warnings because `load_all_milestones` excludes archive by design.
    #[test]
    fn cross_ref_does_not_flag_archived_m_ref() {
        let mut ids: std::collections::HashSet<String> = ["M112".into()].into_iter().collect();
        ids.insert("M98".into());
        ids.insert("M99".into());

        let m = fixture_with_m_ref("M98");
        let step_ids_by_milestone: std::collections::HashMap<
            String,
            std::collections::HashSet<String>,
        > = std::collections::HashMap::new();
        let mut warnings = Vec::new();
        validate_cross_refs(&m, &ids, &step_ids_by_milestone, &mut warnings);
        let w43: Vec<&ValidationIssue> = warnings.iter().filter(|w| w.code == "W43").collect();
        assert!(w43.is_empty(), "expected zero W43 warnings, got {w43:?}");
    }

    /// W43 cross-ref validator: cross-milestone step references in AC
    /// descriptions (e.g., "M106 S11") are filtered out — they're author
    /// prose, not actionable references. Pre-fix this fired false-positives
    /// because the validator tried to find S11 in M112's own step_ids.
    #[test]
    fn cross_ref_does_not_flag_cross_milestone_step_ref_in_ac_description() {
        let ids: std::collections::HashSet<String> =
            ["M112".into(), "M106".into()].into_iter().collect();
        let m = MilestoneFile {
            milestone: MilestoneMeta {
                id: "112".into(),
                title: "test".into(),
                slug: "test".into(),
                lifecycle: "approved".into(),
                ..Default::default()
            },
            acceptance_criteria: vec![AcceptanceCriterion {
                id: "AC-01".into(),
                description: "The M106 S11 test path is no longer needed".into(),
                verification: "manual: accepted".into(),
                status: "pending".into(),
                evidence: String::new(),
            }],
            steps: vec![],
            ..Default::default()
        };
        let step_ids_by_milestone: std::collections::HashMap<
            String,
            std::collections::HashSet<String>,
        > = [(
            "M106".to_string(),
            ["S11".to_string()].into_iter().collect(),
        )]
        .into_iter()
        .collect();
        let mut warnings = Vec::new();
        validate_cross_refs(&m, &ids, &step_ids_by_milestone, &mut warnings);
        let w43: Vec<&ValidationIssue> = warnings.iter().filter(|w| w.code == "W43").collect();
        assert!(
            w43.is_empty(),
            "expected zero W43 warnings for cross-milestone S-ref in AC, got {w43:?}"
        );
    }

    /// W43 cross-ref validator: an M-ref to a milestone that doesn't exist
    /// anywhere (active or archive) still trips the warning.
    #[test]
    fn cross_ref_flags_truly_nonexistent_m_ref() {
        let ids: std::collections::HashSet<String> = ["M112".into()].into_iter().collect();
        let m = fixture_with_m_ref("M9999");
        let step_ids_by_milestone: std::collections::HashMap<
            String,
            std::collections::HashSet<String>,
        > = std::collections::HashMap::new();
        let mut warnings = Vec::new();
        validate_cross_refs(&m, &ids, &step_ids_by_milestone, &mut warnings);
        let w43: Vec<&ValidationIssue> = warnings.iter().filter(|w| w.code == "W43").collect();
        assert!(
            !w43.is_empty(),
            "expected at least one W43 warning for M9999"
        );
        assert!(w43[0].message.contains("M9999"));
    }

    /// W43 cross-ref validator (remediation 2026-07-05): a real S-ref typo
    /// in an AC description still trips the warning. The cross-milestone
    /// "M106 S11" pattern is the false-positive we suppress; an *orphan* S-ref
    /// like "see S99" where M99 has no S99 step should still warn. Pre-fix
    /// S-refs in AC descriptions were silently skipped, hiding typos.
    #[test]
    fn cross_ref_flags_orphan_step_ref_in_ac_description() {
        let ids: std::collections::HashSet<String> = ["M112".into()].into_iter().collect();
        // Build a milestone with step S1 only. The S99 reference is a typo.
        let m = MilestoneFile {
            milestone: MilestoneMeta {
                id: "112".into(),
                title: "test".into(),
                slug: "test".into(),
                lifecycle: "approved".into(),
                ..Default::default()
            },
            acceptance_criteria: vec![AcceptanceCriterion {
                id: "AC-01".into(),
                description: "see S99 of this milestone for the test path".into(),
                verification: "manual: accepted".into(),
                status: "pending".into(),
                evidence: String::new(),
            }],
            steps: vec![Step {
                id: "S1".into(),
                action: "do thing".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let step_ids_by_milestone: std::collections::HashMap<
            String,
            std::collections::HashSet<String>,
        > = std::collections::HashMap::new();
        let mut warnings = Vec::new();
        validate_cross_refs(&m, &ids, &step_ids_by_milestone, &mut warnings);
        let w43: Vec<&ValidationIssue> = warnings.iter().filter(|w| w.code == "W43").collect();
        assert!(
            !w43.is_empty(),
            "expected a W43 warning for orphan S99 in AC description"
        );
        assert!(w43[0].message.contains("S99"));
    }
}
