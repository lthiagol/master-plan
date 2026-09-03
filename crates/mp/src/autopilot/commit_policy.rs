//! M223 — commit attribution policy for the autopilot closure ceremony.
//!
//! The herdr R5/R7/R10 lessons called out three classes of
//! attribution drift the runner lane used to introduce:
//!
//! - **R5** — lifecycle fabrications where the commit log was the
//!   only signal that the closure ceremony ever ran.
//! - **R7** — completion-summary commits that overwrote per-AC
//!   evidence (the commit subject was the only "M<id> complete: …"
//!   record).
//! - **R10** — grouped remediation commits where a single commit
//!   claimed to fix multiple findings. The runner recorded the
//!   same `fixed_in` SHA on several findings and the cycle engine
//!   could not distinguish "one fix per finding" from "bulk fix".
//!
//! This module is the typed classifier + policy that rejects those
//! patterns. It composes with
//! [`crate::autopilot::lifecycle::CommitAttestation`] so the
//! closure protocol can refuse `ResolveFinding` transitions whose
//! `fixed_in` SHA would not pass.
//!
//! ## Classification
//!
//! Every commit in the project history is classified into one of
//! [`CommitKind`]:
//!
//! - `Implementation { step_id }` — `M<id>: S<n> — …` shape.
//!   Single intent (one step).
//! - `SelfReviewFix { finding_id }` — `M<id>: fix self-review …`
//!   shape. Single intent (one finding).
//! - `LifecycleMetadata { milestone_id, transition }` —
//!   `M<id>: lifecycle evidence …` shape. Treated as
//!   *evidence-overwriting* unless the commit body explicitly
//!   preserves the per-AC evidence revisions.
//! - `Ambiguous { reasons }` — more than one intent marker in the
//!   subject (e.g. `M223: S1 — fix F-01`); refused as grouped
//!   remediation.
//! - `Unknown` — does not match any of the above.
//!
//! ## Validation
//!
//! [`validate_fixed_in`] is the policy entry point. It accepts a
//! `CommitIndex` (a fixture or a `git log` reader) and rejects:
//!
//! - **Missing** `fixed_in` (empty string).
//! - **Fabricated** `fixed_in` (SHA not in the index).
//! - **Grouped** remediation (commit's `kind` is `Ambiguous` or the
//!   SHA's metadata says it touches more than one finding).
//!
//! ## Evidence-preservation
//!
//! [`lifecycle_metadata_overwrites_evidence`] is the R7 detector.
//! A `LifecycleMetadata` commit is classified as
//! "evidence-overwriting" unless its body carries the literal
//! per-AC evidence revisions. The autopilot persists a small
//! manifest alongside the commit so a later reader can audit it.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Closed set of commit intents. `Ambiguous` is a fail-fast
/// sentinel: a commit that mentions more than one intent marker
/// is refused as a `fixed_in` candidate (per R10 — grouped
/// remediation is the `fixed_in` drift pattern).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CommitKind {
    /// `M<id>: S<n> — <description>` — implements one step.
    Implementation { milestone_id: String, step_id: String },
    /// `M<id>: fix self-review <area> — <description>` — fixes
    /// exactly one finding.
    SelfReviewFix { milestone_id: String, finding_id: String },
    /// `M<id>: lifecycle evidence …` — moves the milestone's
    /// lifecycle forward. Treated as evidence-overwriting by
    /// default unless the body carries the manifest.
    LifecycleMetadata { milestone_id: String, transition: String },
    /// More than one intent marker in the subject — refused as
    /// `fixed_in` and as `Implementation` candidate.
    Ambiguous { reasons: Vec<String> },
    /// Does not match any of the above markers.
    Unknown,
}

impl CommitKind {
    pub const fn is_implementation(&self) -> bool {
        matches!(self, CommitKind::Implementation { .. })
    }

    pub const fn is_self_review_fix(&self) -> bool {
        matches!(self, CommitKind::SelfReviewFix { .. })
    }

    pub const fn is_lifecycle_metadata(&self) -> bool {
        matches!(self, CommitKind::LifecycleMetadata { .. })
    }

    pub const fn is_ambiguous(&self) -> bool {
        matches!(self, CommitKind::Ambiguous { .. })
    }
}

/// One commit's classification. The body is included so the
/// evidence-preservation check can inspect the per-AC manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitInspection {
    pub sha: String,
    pub subject: String,
    pub body: String,
    pub kind: CommitKind,
}

impl CommitInspection {
    pub fn new(sha: impl Into<String>, subject: impl Into<String>, body: impl Into<String>) -> Self {
        let subject = subject.into();
        let body = body.into();
        let kind = classify_subject(&subject);
        Self {
            sha: sha.into(),
            subject,
            body,
            kind,
        }
    }
}

// ─── Subject classification ──────────────────────────────────────────

/// Classify a commit subject into a [`CommitKind`]. The classifier
/// is deliberately conservative: when the subject carries more
/// than one intent marker the commit is `Ambiguous`. The rule is
/// applied per-milestone-prefix — a `M223: S1 — fix F-01` subject
/// is ambiguous because both the implementation marker (`S1`) and
/// the fix marker (`fix F-01`) are present.
pub fn classify_subject(subject: &str) -> CommitKind {
    let trimmed = subject.trim();
    if trimmed.is_empty() {
        return CommitKind::Unknown;
    }
    // The intent markers are mutually exclusive. Collect every
    // match in a single pass; if more than one fires the commit
    // is `Ambiguous`.
    let mut matches: Vec<(&'static str, CommitKind)> = Vec::new();

    // 1. Lifecycle metadata: "M<id>: lifecycle evidence …" — must
    //    start the tail (this is the canonical marker shape and
    //    not a substring that could appear later in the subject).
    if let Some(rest) = trimmed.strip_prefix("M") {
        if let Some((head, tail)) = rest.split_once(':') {
            let milestone_id = head.trim().to_string();
            let tail = tail.trim_start();
            if let Some(transition) = tail.strip_prefix("lifecycle evidence") {
                matches.push((
                    "lifecycle",
                    CommitKind::LifecycleMetadata {
                        milestone_id: milestone_id.clone(),
                        transition: transition.trim().to_string(),
                    },
                ));
            }
            // Implementation marker: "S<n>" tokenized anywhere in
            // the tail (e.g. "M223: S1 — …" or "M223: S1 + S2 — …"
            // or — the grouped-remediation case —
            // "M223: S1 — fix F-01"). The latter is *also* an
            // implementation marker AND a self-review-fix marker,
            // so we record both and the final length check marks
            // the commit `Ambiguous`.
            let step_ids = collect_step_ids(tail);
            for step_id in step_ids {
                matches.push((
                    "implementation",
                    CommitKind::Implementation {
                        milestone_id: milestone_id.clone(),
                        step_id,
                    },
                ));
            }
            // Self-review-fix marker: "fix self-review F-NN" or
            // "fix F-NN" tokens anywhere in the tail.
            let finding_ids = collect_finding_ids(tail);
            for finding_id in finding_ids {
                matches.push((
                    "self-review-fix",
                    CommitKind::SelfReviewFix {
                        milestone_id: milestone_id.clone(),
                        finding_id,
                    },
                ));
            }
        }
    }

    if matches.len() > 1 {
        return CommitKind::Ambiguous {
            reasons: matches.into_iter().map(|(r, _)| r.to_string()).collect(),
        };
    }
    if let Some((_, kind)) = matches.into_iter().next() {
        return kind;
    }
    CommitKind::Unknown
}

/// Scan `tail` for every `S<n>` token. Tokens must be
/// whitespace-bounded or hyphen/em-dash bounded to avoid
/// matching path components like `S1.json`. The first token is
/// the canonical step id.
fn collect_step_ids(tail: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = tail;
    while let Some(idx) = rest.find('S') {
        let after_s = &rest[idx + 1..];
        // The character after S must be a digit.
        let mut chars = after_s.chars();
        if let Some(c) = chars.next() {
            if c.is_ascii_digit() {
                let digits: String = chars.take_while(|c| c.is_ascii_digit()).collect();
                let token = format!("S{c}{digits}");
                if !out.contains(&token) {
                    out.push(token);
                }
                rest = &after_s[1 + digits.len()..];
                continue;
            }
        }
        rest = &after_s[1..];
    }
    out
}

/// Scan `tail` for every `F-<n>` token. Used by both
/// `fix self-review F-NN` and the bare `fix F-NN` form.
fn collect_finding_ids(tail: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = tail;
    while let Some(idx) = rest.find('F') {
        let after_f = &rest[idx + 1..];
        let mut chars = after_f.chars();
        if chars.next() == Some('-') {
            let digits: String = chars.take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                let token = format!("F-{digits}");
                if !out.contains(&token) {
                    out.push(token);
                }
                rest = &after_f[1 + digits.len()..];
                continue;
            }
        }
        rest = &after_f[1..];
    }
    out
}

/// Pull `S<n>` out of `S<n> — …` or `S<n>: …`. Returns the
/// canonical step id (`S<n>`) on success.
fn parse_step_id(s: &str) -> Option<String> {
    let s = s.trim();
    let mut chars = s.chars();
    if chars.next() != Some('S') {
        return None;
    }
    let digits: String = chars.take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    Some(format!("S{digits}"))
}

/// Pull `F-<n>` out of `F-<n> — …`. Conservative: only matches
/// the canonical `F-NN` form.
fn parse_finding_id(s: &str) -> Option<String> {
    let s = s.trim();
    let mut chars = s.chars();
    if chars.next() != Some('F') {
        return None;
    }
    if chars.next() != Some('-') {
        return None;
    }
    let digits: String = chars.take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    Some(format!("F-{digits}"))
}

// ─── Policy errors ───────────────────────────────────────────────────

/// Errors raised by [`validate_fixed_in`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PolicyError {
    /// The finding has no `fixed_in` SHA at all. The closure
    /// protocol refuses to mark a finding resolved without a
    /// commit reference.
    MissingFixedIn { finding_id: String },
    /// The SHA is fabricated (not in the commit index the policy
    /// trusts). Per AC-02 the policy must reject fabricated SHAs
    /// before they reach `reviews.json`.
    FabricatedSha { finding_id: String, sha: String },
    /// The SHA exists but the commit's `kind` is `Ambiguous` —
    /// it claims to fix multiple findings in one commit. Per
    /// the M200/M202 `fixed_in` drift lesson, grouped remediation
    /// must be split into one commit per finding.
    GroupedRemediation {
        finding_id: String,
        sha: String,
        reasons: Vec<String>,
    },
    /// The SHA is a real commit but it is a *lifecycle metadata*
    /// commit, not a fix commit. The runner used a metadata
    /// commit as a `fixed_in` placeholder — rejected.
    LifecycleMetadataNotFix {
        finding_id: String,
        sha: String,
        transition: String,
    },
    /// The lifecycle metadata commit overwrites per-AC evidence
    /// (R7 guard). The commit body's manifest must include every
    /// AC's evidence revision.
    EvidenceOverwritingMetadata {
        finding_id: String,
        sha: String,
        missing_ac_revisions: Vec<String>,
    },
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::MissingFixedIn { finding_id } => {
                write!(f, "finding {finding_id} has no fixed_in SHA")
            }
            PolicyError::FabricatedSha { finding_id, sha } => write!(
                f,
                "finding {finding_id} fixed_in={sha} is not in the commit index"
            ),
            PolicyError::GroupedRemediation {
                finding_id,
                sha,
                reasons,
            } => write!(
                f,
                "finding {finding_id} fixed_in={sha} is an ambiguous commit ({}); split into one commit per finding",
                reasons.join(", ")
            ),
            PolicyError::LifecycleMetadataNotFix {
                finding_id,
                sha,
                transition,
            } => write!(
                f,
                "finding {finding_id} fixed_in={sha} is a lifecycle metadata commit ({transition}); use a fix commit instead"
            ),
            PolicyError::EvidenceOverwritingMetadata {
                finding_id,
                sha,
                missing_ac_revisions,
            } => write!(
                f,
                "finding {finding_id} fixed_in={sha} is a metadata commit that would overwrite per-AC evidence; missing manifest for {} ACs",
                missing_ac_revisions.len()
            ),
        }
    }
}

impl std::error::Error for PolicyError {}

// ─── Commit index ────────────────────────────────────────────────────

/// In-memory commit index used by tests and by the autopilot
/// closure ceremony. Production wires this to `git log` (see
/// [`crate::autopilot::verifier::git_log_for_path`]).
#[derive(Debug, Default, Clone)]
pub struct CommitIndex {
    /// SHA → inspection.
    commits: std::collections::BTreeMap<String, CommitInspection>,
}

impl CommitIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, inspection: CommitInspection) {
        self.commits.insert(inspection.sha.clone(), inspection);
    }

    pub fn lookup(&self, sha: &str) -> Option<&CommitInspection> {
        self.commits.get(sha)
    }

    pub fn shas(&self) -> BTreeSet<&str> {
        self.commits.keys().map(String::as_str).collect()
    }
}

// ─── Validation ──────────────────────────────────────────────────────

/// Validate a `fixed_in` SHA against the policy. Returns `Ok(())`
/// on success and a typed [`PolicyError`] on rejection.
pub fn validate_fixed_in(
    finding_id: &str,
    fixed_in: &str,
    index: &CommitIndex,
) -> Result<(), PolicyError> {
    if fixed_in.is_empty() {
        return Err(PolicyError::MissingFixedIn {
            finding_id: finding_id.to_string(),
        });
    }
    let Some(inspection) = index.lookup(fixed_in) else {
        return Err(PolicyError::FabricatedSha {
            finding_id: finding_id.to_string(),
            sha: fixed_in.to_string(),
        });
    };
    if let CommitKind::Ambiguous { reasons } = &inspection.kind {
        return Err(PolicyError::GroupedRemediation {
            finding_id: finding_id.to_string(),
            sha: fixed_in.to_string(),
            reasons: reasons.iter().map(|r| (*r).to_string()).collect(),
        });
    }
    if let CommitKind::LifecycleMetadata { transition, .. } = &inspection.kind {
        return Err(PolicyError::LifecycleMetadataNotFix {
            finding_id: finding_id.to_string(),
            sha: fixed_in.to_string(),
            transition: transition.clone(),
        });
    }
    Ok(())
}

/// R7 detector. A `LifecycleMetadata` commit is the canonical
/// closure-cermony signal ("lifecycle evidence cycle N"). Such a
/// commit is *allowed* in the index, but it must carry an
/// evidence manifest in its body — a comma-separated list of
/// `AC-##=<revision>` tokens that the verifier cross-checks
/// against the canonical milestone JSON.
///
/// The body is expected to contain a line of the form:
///
/// ```text
/// Per-AC evidence manifest: AC-01=rev-1, AC-02=rev-1
/// ```
///
/// Returns `Ok(())` if the manifest covers every required
/// revision, or [`PolicyError::EvidenceOverwritingMetadata`] if
/// the manifest is missing or incomplete.
pub fn lifecycle_metadata_overwrites_evidence(
    inspection: &CommitInspection,
    required_ac_revisions: &[(&str, &str)],
) -> Result<(), PolicyError> {
    let CommitKind::LifecycleMetadata { milestone_id, .. } = &inspection.kind else {
        return Ok(());
    };
    let missing: Vec<String> = required_ac_revisions
        .iter()
        .filter(|(ac_id, revision)| {
            !body_carries_evidence_manifest(&inspection.body, ac_id, revision)
        })
        .map(|(ac_id, _)| (*ac_id).to_string())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(PolicyError::EvidenceOverwritingMetadata {
            finding_id: format!("milestone:{milestone_id}"),
            sha: inspection.sha.clone(),
            missing_ac_revisions: missing,
        })
    }
}

fn body_carries_evidence_manifest(body: &str, ac_id: &str, revision: &str) -> bool {
    let needle = format!("{ac_id}={revision}");
    body.lines().any(|line| line.contains(&needle))
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(s: &str) -> CommitKind {
        classify_subject(s)
    }

    #[test]
    fn classifies_implementation() {
        let k = subject("M223: S1 — lifecycle closure protocol");
        match k {
            CommitKind::Implementation {
                milestone_id,
                step_id,
            } => {
                assert_eq!(milestone_id, "223");
                assert_eq!(step_id, "S1");
            }
            other => panic!("expected Implementation, got {other:?}"),
        }
    }

    #[test]
    fn classifies_self_review_fix() {
        let k = subject("M223: fix self-review F-01 — clippy needless-borrow");
        match k {
            CommitKind::SelfReviewFix {
                milestone_id,
                finding_id,
            } => {
                assert_eq!(milestone_id, "223");
                assert_eq!(finding_id, "F-01");
            }
            other => panic!("expected SelfReviewFix, got {other:?}"),
        }
    }

    #[test]
    fn classifies_lifecycle_metadata() {
        let k = subject("M223: lifecycle evidence cycle 1");
        match k {
            CommitKind::LifecycleMetadata {
                milestone_id,
                transition,
            } => {
                assert_eq!(milestone_id, "223");
                assert_eq!(transition, "cycle 1");
            }
            other => panic!("expected LifecycleMetadata, got {other:?}"),
        }
    }

    #[test]
    fn classifies_ambiguous_grouped_fix() {
        // Subject mentions both S1 and fix F-01 — ambiguous.
        let k = subject("M223: S1 — fix F-01 grouped");
        assert!(k.is_ambiguous(), "expected Ambiguous, got {k:?}");
    }

    #[test]
    fn classifies_unknown() {
        assert!(matches!(subject("chore: bump deps"), CommitKind::Unknown));
    }

    #[test]
    fn validates_missing_fixed_in() {
        let index = CommitIndex::new();
        let err = validate_fixed_in("F-01", "", &index).unwrap_err();
        assert!(matches!(err, PolicyError::MissingFixedIn { .. }));
    }

    #[test]
    fn validates_fabricated_sha() {
        let index = CommitIndex::new();
        let err = validate_fixed_in("F-01", "sha-not-in-index", &index).unwrap_err();
        assert!(matches!(err, PolicyError::FabricatedSha { .. }));
    }

    #[test]
    fn validates_grouped_remediation_rejected() {
        let mut index = CommitIndex::new();
        index.insert(CommitInspection::new(
            "sha-grouped",
            "M223: S1 — fix F-01 + fix F-02",
            "",
        ));
        let err = validate_fixed_in("F-01", "sha-grouped", &index).unwrap_err();
        assert!(matches!(err, PolicyError::GroupedRemediation { .. }));
    }

    #[test]
    fn validates_lifecycle_metadata_not_a_fix() {
        let mut index = CommitIndex::new();
        index.insert(CommitInspection::new(
            "sha-meta",
            "M223: lifecycle evidence cycle 1",
            "",
        ));
        let err = validate_fixed_in("F-01", "sha-meta", &index).unwrap_err();
        match err {
            PolicyError::LifecycleMetadataNotFix { transition, .. } => {
                assert_eq!(transition, "cycle 1");
            }
            other => panic!("expected LifecycleMetadataNotFix, got {other:?}"),
        }
    }

    #[test]
    fn validates_real_single_fix_commit_passes() {
        let mut index = CommitIndex::new();
        index.insert(CommitInspection::new(
            "sha-fix",
            "M223: fix self-review F-01 — clippy",
            "",
        ));
        assert!(validate_fixed_in("F-01", "sha-fix", &index).is_ok());
    }

    #[test]
    fn lifecycle_metadata_overwrites_evidence_when_manifest_missing() {
        let inspection = CommitInspection::new(
            "sha-meta",
            "M223: lifecycle evidence cycle 1",
            "just a summary commit, no manifest",
        );
        let err =
            lifecycle_metadata_overwrites_evidence(&inspection, &[("AC-01", "rev-1")]).unwrap_err();
        match err {
            PolicyError::EvidenceOverwritingMetadata {
                missing_ac_revisions,
                ..
            } => {
                assert_eq!(missing_ac_revisions, vec!["AC-01".to_string()]);
            }
            other => panic!("expected EvidenceOverwritingMetadata, got {other:?}"),
        }
    }

    #[test]
    fn lifecycle_metadata_with_complete_manifest_passes() {
        let inspection = CommitInspection::new(
            "sha-meta",
            "M223: lifecycle evidence cycle 1",
            "Per-AC evidence manifest: AC-01=rev-1, AC-02=rev-1, AC-03=rev-1",
        );
        assert!(lifecycle_metadata_overwrites_evidence(
            &inspection,
            &[("AC-01", "rev-1"), ("AC-02", "rev-1"), ("AC-03", "rev-1")],
        )
        .is_ok());
    }

    #[test]
    fn lifecycle_metadata_with_partial_manifest_rejected() {
        let inspection = CommitInspection::new(
            "sha-meta",
            "M223: lifecycle evidence cycle 1",
            "Per-AC evidence manifest: AC-01=rev-1",
        );
        let err = lifecycle_metadata_overwrites_evidence(
            &inspection,
            &[("AC-01", "rev-1"), ("AC-02", "rev-1")],
        )
        .unwrap_err();
        match err {
            PolicyError::EvidenceOverwritingMetadata {
                missing_ac_revisions,
                ..
            } => {
                assert_eq!(missing_ac_revisions, vec!["AC-02".to_string()]);
            }
            other => panic!("expected EvidenceOverwritingMetadata, got {other:?}"),
        }
    }

    #[test]
    fn non_lifecycle_commit_short_circuits_overwrite_check() {
        let inspection = CommitInspection::new(
            "sha-fix",
            "M223: fix self-review F-01 — clippy",
            "",
        );
        assert!(lifecycle_metadata_overwrites_evidence(&inspection, &[("AC-01", "rev-1")]).is_ok());
    }
}