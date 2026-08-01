use anyhow::Result;
use serde::Serialize;
use serde_json::json;

use crate::milestone;
use crate::paths::{self, PlanContext};
use crate::reviews;

#[derive(Debug, Serialize)]
pub struct StepCounts {
    pub total: usize,
    pub done: usize,
    pub pending: usize,
    pub failed: usize,
    pub other: usize,
}

#[derive(Debug, Serialize)]
pub struct AcCounts {
    pub total: usize,
    pub passed: usize,
    pub pending: usize,
    pub failed: usize,
    pub other: usize,
}

#[derive(Debug, Serialize)]
pub struct FindingCounts {
    pub total: usize,
    pub open: usize,
    pub fixed: usize,
    pub other: usize,
}

#[derive(Debug, Serialize)]
pub struct VerificationHealth {
    pub evidence: String,
    pub force_bypassed: bool,
}

/// Agent-oriented rollup replacing ad-hoc `mp show milestone | jq` health checks.
#[derive(Debug, Serialize)]
pub struct MilestoneHealthSummary {
    pub milestone_id: String,
    pub display: String,
    pub title: String,
    pub execution_status: String,
    pub spec_status: String,
    pub review_state: String,
    pub steps: StepCounts,
    pub acceptance_criteria: AcCounts,
    pub findings: FindingCounts,
    pub verification: VerificationHealth,
}

fn count_steps(items: impl Iterator<Item = String>) -> StepCounts {
    let mut done = 0usize;
    let mut pending = 0usize;
    let mut failed = 0usize;
    let mut other = 0usize;
    let mut total = 0usize;
    for status in items {
        total += 1;
        match status.as_str() {
            "done" => done += 1,
            "pending" | "in-progress" | "planned" => pending += 1,
            "failed" => failed += 1,
            _ => other += 1,
        }
    }
    StepCounts {
        total,
        done,
        pending,
        failed,
        other,
    }
}

fn count_acs(items: impl Iterator<Item = String>) -> AcCounts {
    let mut passed = 0usize;
    let mut pending = 0usize;
    let mut failed = 0usize;
    let mut other = 0usize;
    let mut total = 0usize;
    for status in items {
        total += 1;
        match status.as_str() {
            "passed" => passed += 1,
            "pending" => pending += 1,
            "failed" => failed += 1,
            _ => other += 1,
        }
    }
    AcCounts {
        total,
        passed,
        pending,
        failed,
        other,
    }
}

fn count_findings(findings: &[crate::model::Finding]) -> FindingCounts {
    let mut open = 0usize;
    let mut fixed = 0usize;
    let mut other = 0usize;
    for f in findings {
        match f.status.as_str() {
            "open" => open += 1,
            "fixed" => fixed += 1,
            _ => other += 1,
        }
    }
    FindingCounts {
        total: findings.len(),
        open,
        fixed,
        other,
    }
}

/// True when a verification evidence string marks the milestone/AC as force-
/// bypassed. Shared by the health summary and the spec-review surface so the
/// two never disagree on what counts as a bypass. Keep the marker strings in
/// sync with the writing paths (`mp milestone complete --force` and friends).
pub fn evidence_marks_force_bypass(evidence: &str) -> bool {
    evidence.contains("[force-bypassed") || evidence.contains("[step-tests force-bypassed")
}

pub fn build_milestone_health_summary(
    ctx: &PlanContext,
    milestone_id: &str,
) -> Result<MilestoneHealthSummary> {
    let m = milestone::load_milestone_by_id(ctx, milestone_id)?;
    let review_state = reviews::milestone_review_state(ctx, &m)?;
    let evidence = m.verification.evidence.clone();
    let force_bypassed = evidence_marks_force_bypass(&evidence);

    Ok(MilestoneHealthSummary {
        milestone_id: m.milestone.id.clone(),
        display: paths::display_milestone_id(&m.milestone.id),
        title: m.milestone.title.clone(),
        execution_status: m.milestone.execution_status.clone(),
        spec_status: m.milestone.spec_status.clone(),
        review_state: review_state.to_string(),
        steps: count_steps(m.steps.iter().map(|s| s.status.clone())),
        acceptance_criteria: count_acs(m.acceptance_criteria.iter().map(|ac| ac.status.clone())),
        findings: count_findings(&m.findings),
        verification: VerificationHealth {
            evidence,
            force_bypassed,
        },
    })
}

pub fn finding_counts(findings: &[crate::model::Finding]) -> serde_json::Value {
    let counts = count_findings(findings);
    serde_json::to_value(counts).unwrap_or(json!({}))
}

#[cfg(test)]
mod tests {
    use super::evidence_marks_force_bypass;

    #[test]
    fn evidence_marks_force_bypass_recognizes_marker() {
        assert!(evidence_marks_force_bypass(
            "[force-bypassed: AC-01 threshold unmet] re-measured drop = 17.4s"
        ));
        assert!(evidence_marks_force_bypass(
            "context: [step-tests force-bypassed because cargo nextest cold was OOM-killed]"
        ));
        // Marker anywhere in the string is sufficient — `evidence.contains` semantics.
        assert!(evidence_marks_force_bypass(
            "preamble\n[force-bypassed]\npostamble"
        ));
    }

    #[test]
    fn evidence_marks_force_bypass_rejects_absent_marker() {
        assert!(!evidence_marks_force_bypass(""));
        assert!(!evidence_marks_force_bypass(
            "no marker here; force-bypass is a substring token but the bracketed form is what counts"
        ));
        // Substring of the marker (without the bracket) does NOT count.
        assert!(!evidence_marks_force_bypass("force-bypassed: no bracket"));
        // A different bracketed marker (not the canonical one) does NOT count.
        assert!(!evidence_marks_force_bypass(
            "[force-bypass] without trailing d"
        ));
    }

    /// Mirrors the predicate's caller (`build_milestone_health_summary`):
    /// `verification.force_bypassed` must be true iff the evidence contains
    /// the marker. This is the property the M165 post-completion amend
    /// relies on — flipping the marker string in the file flips the summary.
    #[test]
    fn force_bypassed_flag_tracks_marker_in_evidence() {
        let with_marker = "[force-bypassed: legacy] tail";
        let without_marker = "clean evidence; no marker";
        assert!(evidence_marks_force_bypass(with_marker));
        assert!(!evidence_marks_force_bypass(without_marker));
    }
}
