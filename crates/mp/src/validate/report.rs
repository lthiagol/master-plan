#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub milestone: Option<String>,
}

/// M142 AC-06: L5 audit info embedded in the validate output under
/// `l5_audit`. Always non-blocking — `severity` is `"advisory"` for
/// every violation. `mp validate --summary` does NOT count L5
/// violations toward `error_count` and the exit code stays 0 when
/// only advisory L5 violations exist.
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct L5AuditSection {
    /// True when no L5 violations were detected across the plan's
    /// milestones.
    pub ok: bool,
    /// Aggregate violation count across every milestone.
    pub violation_count: usize,
    /// Per-milestone L5 audit rollup (mirrors `mp reviews l5-check`).
    pub milestones: Vec<L5MilestoneAudit>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct L5MilestoneAudit {
    pub milestone_id: String,
    pub ok: bool,
    pub violation_count: usize,
    pub total_handoffs: usize,
    pub cross_role_handoffs: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationReport {
    pub ok: bool,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
    /// M142 AC-06: L5 audit rollup (advisory; never gates `ok`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l5_audit: Option<L5AuditSection>,
}

pub(crate) fn issue(code: &str, message: &str, milestone: Option<String>) -> ValidationIssue {
    ValidationIssue {
        code: code.to_string(),
        message: message.to_string(),
        milestone,
    }
}

pub(crate) fn report(
    errors: Vec<ValidationIssue>,
    warnings: Vec<ValidationIssue>,
) -> ValidationReport {
    ValidationReport {
        ok: errors.is_empty(),
        errors,
        warnings,
        l5_audit: None,
    }
}
