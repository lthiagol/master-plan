//! M154: hunk export pipeline.
//!
//! `mp reviews hunk <M>` consumes the milestone's findings + comments
//! (mp's durable review record, see `crates/mp-model::milestone`) and
//! renders them as hunk-compatible annotations. Two output channels,
//! both gated on `[review] hunk = true`:
//!
//! 1. **Live batch** (stdout, default). Pipe-ready for `hunk session
//!    comment apply --stdin`. Each entry has a single `newLine` —
//!    hunk's live session takes single-line comments and uses
//!    `comment apply` to thread them onto a running diff.
//!
//! 2. **Agent-context sidecar** (`--file <path>`). Startup-loadable by
//!    `hunk diff --agent-context <path>`. Each annotation has a
//!    `[start, end]` range tuple (the live batch's `newLine` collapses
//!    to `[N, N]` here). The sidecar is regenerated fresh on each
//!    export — hunk reads it once at startup and does not hot-reload
//!    it.
//!
//! Findings drive the export (per design_decisions "comments-revival":
//! agents never adopted the M133 comment primitive — 0 entries in
//! reviews.json — so the export maps findings, which agents DO
//! produce). Comments are emitted when present, attached to the
//! same file path. Unanchored findings / comments surface as file-
//! level summary notes (no newLine / no range).
//!
//! Severity maps to hunk's `confidence` field: `high` → `high`,
//! `medium` → `medium`, `low` → `low`, anything else → `medium` (the
//! shape is documented in the spec, not a free-form string).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::ProjectConfig;
use crate::model::{Finding, FindingAnchor, ReviewComment};
use crate::paths::PlanContext;

/// Top-level wrapper for the live batch output. The `comments` key
/// is the documented name in hunk's `comment apply` schema
/// (`hunk/src/core/agent.ts: normalizeCommentApply`); a future
/// hunk-side change to a different wrapper name would be caught
/// by AC-03's "validates against hunk's expected shape" test
/// (which is exactly the round-trip contract this struct pins).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HunkCommentBatch {
    pub comments: Vec<HunkComment>,
}

/// One entry in the live batch. `filePath` may be `None` for
/// file-level (unanchored) comments — hunk accepts null filePath
/// for non-line-anchored notes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HunkComment {
    #[serde(rename = "filePath")]
    pub file_path: Option<String>,
    #[serde(rename = "newLine")]
    pub new_line: Option<u32>,
    pub summary: String,
    pub rationale: String,
    pub author: String,
    /// M154 AC-03: confidence is informational on the live batch —
    /// hunk's live `comment apply` doesn't gate on it, but the
    /// field is the same key the sidecar uses, so we keep the
    /// shape uniform across the two channels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
}

/// Top-level wrapper for the agent-context sidecar. hunk loads
/// this once at startup via `hunk diff --agent-context <path>`; the
/// file is not hot-reloaded (per AC-06).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HunkAgentContext {
    /// Schema version — bumped on shape changes; consumers should
    /// reject unrecognized versions.
    pub version: u32,
    pub files: Vec<HunkAgentFile>,
}

/// One file's worth of annotations. `path` is the relative or
/// absolute file path; hunk uses it to match the diff being viewed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HunkAgentFile {
    pub path: String,
    pub annotations: Vec<HunkAgentAnnotation>,
}

/// One entry in the sidecar. `newRange` and `oldRange` are
/// `[start, end]` tuples; `start == end` is the single-line shape
/// that matches the live batch's `newLine: N`. Unanchored entries
/// (no range) are omitted — see `export_sidecar` for the rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HunkAgentAnnotation {
    /// `[start, end]` line numbers, 1-based. Present when the source
    /// anchor was on the new side of a diff.
    #[serde(rename = "newRange", skip_serializing_if = "Option::is_none")]
    pub new_range: Option<[u32; 2]>,
    /// `[start, end]` line numbers, 1-based. Present when the source
    /// anchor was on the old side of a diff.
    #[serde(rename = "oldRange", skip_serializing_if = "Option::is_none")]
    pub old_range: Option<[u32; 2]>,
    pub summary: String,
    pub rationale: String,
    pub author: String,
    /// Severity → confidence mapping; hunk renders this as a
    /// visibility / opacity hint on the annotation chip.
    pub confidence: String,
}

const HUNK_AGENT_CONTEXT_VERSION: u32 = 1;

/// M154 AC-03 / AC-06: export the milestone's findings + comments
/// as a hunk-compatible live batch + (optionally) agent-context
/// sidecar. Returns the live batch unconditionally; the caller
/// decides whether to also write the sidecar via
/// [`export_sidecar_to_path`].
///
/// `config.review_hunk_enabled()` gates the export; callers should
/// have already verified the flag at the CLI layer. This function
/// does NOT re-check — it assumes the caller already errored if the
/// flag is false. Returning the live batch even when the flag is
/// off would silently bypass the opt-in; trust the gate at the
/// entry point.
pub fn export_batch(
    ctx: &PlanContext,
    config: &ProjectConfig,
    milestone_id: &str,
) -> Result<HunkCommentBatch> {
    let input = collect_exportable_inputs(ctx, milestone_id)?;
    let author = config.review_hunk_author().to_string();
    let mut comments = Vec::new();

    for finding in &input.findings {
        if let Some(comment) = finding_to_hunk_comment(finding, &author) {
            comments.push(comment);
        }
    }
    for c in &input.comments {
        if let Some(comment) = comment_to_hunk_comment(c, &author) {
            comments.push(comment);
        }
    }

    Ok(HunkCommentBatch { comments })
}

/// M154 AC-06: render the agent-context sidecar to JSON at the
/// given path. The on-disk shape is loaded by hunk via
/// `hunk diff --agent-context <path>`. Writes via the existing
/// `store::atomic_write` (M113 S2) so a SIGINT mid-write leaves
/// the previous sidecar intact (or absent) rather than a torn JSON.
pub fn export_sidecar_to_path(
    ctx: &PlanContext,
    config: &ProjectConfig,
    milestone_id: &str,
    path: &std::path::Path,
) -> Result<HunkAgentContext> {
    let input = collect_exportable_inputs(ctx, milestone_id)?;
    let sidecar = build_sidecar(&input.findings, &input.comments, config, milestone_id)?;
    let body = serde_json::to_string_pretty(&sidecar)
        .with_context(|| format!("serialize hunk sidecar to {}", path.display()))?;
    crate::store::atomic_write(path, format!("{body}\n"))
        .with_context(|| format!("write hunk sidecar to {}", path.display()))?;
    Ok(sidecar)
}

/// Build the sidecar without writing it. Pure function over the
/// already-collected inputs; exposed for AC-06's shape-validation
/// test and for tests that want to assert JSON shape without
/// touching disk.
pub fn build_sidecar(
    findings: &[Finding],
    comments: &[ReviewComment],
    config: &ProjectConfig,
    milestone_id: &str,
) -> Result<HunkAgentContext> {
    let author = config.review_hunk_author().to_string();
    let mut by_path: std::collections::BTreeMap<String, Vec<HunkAgentAnnotation>> =
        std::collections::BTreeMap::new();
    let mut file_level: Vec<HunkAgentAnnotation> = Vec::new();

    for f in findings {
        if let Some(annotation) = finding_to_hunk_annotation(f, &author) {
            match &annotation {
                HunkAgentAnnotation {
                    new_range: None,
                    old_range: None,
                    ..
                } => file_level.push(annotation),
                _ => by_path
                    .entry(anchor_path(&f.anchor))
                    .or_default()
                    .push(annotation),
            }
        }
    }
    for c in comments {
        if let Some(annotation) = comment_to_hunk_annotation(c, &author) {
            match &annotation {
                HunkAgentAnnotation {
                    new_range: None,
                    old_range: None,
                    ..
                } => file_level.push(annotation),
                _ => by_path
                    .entry(anchor_path(&c.anchor))
                    .or_default()
                    .push(annotation),
            }
        }
    }

    // File-level notes attach to a synthetic "<milestone>" pseudo-path
    // so they show up in the sidecar without dropping data. The
    // agent-context schema has no "global notes" slot; this is the
    // closest equivalent. (L26 docstring-lists-fields: the schema
    // is documented as `files[]`, no global slot — we honor that.)
    if !file_level.is_empty() {
        by_path
            .entry(format!("__milestone-{milestone_id}__"))
            .or_default()
            .extend(file_level);
    }

    let files = by_path
        .into_iter()
        .map(|(path, annotations)| HunkAgentFile { path, annotations })
        .collect();

    Ok(HunkAgentContext {
        version: HUNK_AGENT_CONTEXT_VERSION,
        files,
    })
}

/// M154: extract findings + comments for one milestone from the
/// on-disk reviews store + milestone file. Centralizes the lookup
/// so the live-batch and sidecar paths stay in lockstep.
struct ExportInputs {
    findings: Vec<Finding>,
    comments: Vec<ReviewComment>,
}

fn collect_exportable_inputs(ctx: &PlanContext, milestone_id: &str) -> Result<ExportInputs> {
    let normalized = crate::paths::normalize_milestone_id(milestone_id);
    let milestone_path = crate::milestone::load_milestone_path(ctx, &normalized)?;
    let milestone = crate::store::load_milestone(&milestone_path)?;
    let reviews = crate::reviews::load_reviews(ctx)?;
    let comments: Vec<ReviewComment> = reviews
        .comments
        .into_iter()
        .filter(|c| c.milestone_id == normalized)
        .collect();
    Ok(ExportInputs {
        findings: milestone.findings,
        comments,
    })
}

/// M154 AC-03: convert a finding to a live-batch entry. Returns
/// `None` when the finding has no description / summary / rationale
/// to export (e.g. a stub finding the runner parked without a body)
/// — the export shouldn't carry empty entries.
fn finding_to_hunk_comment(f: &Finding, author: &str) -> Option<HunkComment> {
    let summary = pick_summary(&f.summary, &f.description);
    let rationale = f.rationale.clone();
    if summary.is_empty() && rationale.is_empty() {
        return None;
    }
    let (file_path, new_line) = anchor_to_fileline(&f.anchor);
    Some(HunkComment {
        file_path,
        new_line,
        summary,
        rationale,
        author: author.to_string(),
        confidence: severity_to_confidence(&f.severity),
    })
}

fn comment_to_hunk_comment(c: &ReviewComment, author: &str) -> Option<HunkComment> {
    if c.body.trim().is_empty() {
        return None;
    }
    let (file_path, new_line) = anchor_to_fileline(&c.anchor);
    // For comments the "summary" is the body itself (hunk doesn't
    // have a comment-with-summary split). Rationale is empty — the
    // body IS the rationale.
    Some(HunkComment {
        file_path,
        new_line,
        summary: c.body.trim().to_string(),
        rationale: String::new(),
        author: author.to_string(),
        confidence: None,
    })
}

fn finding_to_hunk_annotation(f: &Finding, author: &str) -> Option<HunkAgentAnnotation> {
    let summary = pick_summary(&f.summary, &f.description);
    let rationale = f.rationale.clone();
    if summary.is_empty() && rationale.is_empty() {
        return None;
    }
    let (new_range, old_range) = anchor_to_ranges(&f.anchor);
    Some(HunkAgentAnnotation {
        new_range,
        old_range,
        summary,
        rationale,
        author: author.to_string(),
        confidence: severity_to_confidence(&f.severity).unwrap_or_else(|| "medium".to_string()),
    })
}

fn comment_to_hunk_annotation(c: &ReviewComment, author: &str) -> Option<HunkAgentAnnotation> {
    if c.body.trim().is_empty() {
        return None;
    }
    let (new_range, old_range) = anchor_to_ranges(&c.anchor);
    Some(HunkAgentAnnotation {
        new_range,
        old_range,
        summary: c.body.trim().to_string(),
        rationale: String::new(),
        author: author.to_string(),
        confidence: "medium".to_string(),
    })
}

/// Pull the source-side anchor's `path` field, defaulting to a
/// sentinel so unanchored findings still group sensibly.
fn anchor_path(anchor: &Option<FindingAnchor>) -> String {
    anchor
        .as_ref()
        .map(|a| a.path.clone())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "<unanchored>".to_string())
}

/// Convert a [`FindingAnchor`] into the live batch's
/// `(filePath, newLine)` tuple. Returns `(None, None)` for unanchored
/// findings (hunk accepts null filePath for file-level notes).
fn anchor_to_fileline(anchor: &Option<FindingAnchor>) -> (Option<String>, Option<u32>) {
    let Some(a) = anchor else {
        return (None, None);
    };
    if a.path.is_empty() {
        return (None, None);
    }
    let line = match a.side.as_deref() {
        Some("old") => a.old_range.as_ref().map(|r| r.start_line),
        _ => a.new_range.as_ref().map(|r| r.start_line),
    };
    (Some(a.path.clone()), line)
}

/// Convert a [`FindingAnchor`] into the sidecar's `(newRange, oldRange)`
/// tuple. Returns `(None, None)` for unanchored findings.
fn anchor_to_ranges(anchor: &Option<FindingAnchor>) -> (Option<[u32; 2]>, Option<[u32; 2]>) {
    let Some(a) = anchor else {
        return (None, None);
    };
    if a.path.is_empty() {
        return (None, None);
    }
    let new_range = a.new_range.as_ref().map(|r| [r.start_line, r.end_line]);
    let old_range = a.old_range.as_ref().map(|r| [r.start_line, r.end_line]);
    (new_range, old_range)
}

/// Severity → confidence mapping. hunk renders confidence as the
/// visibility / opacity of the annotation chip; the spec pins this
/// mapping (per `done_when: "severity->confidence mapping pinned by
/// a test"` in step S3).
fn severity_to_confidence(severity: &str) -> Option<String> {
    match severity {
        "high" => Some("high".to_string()),
        "medium" => Some("medium".to_string()),
        "low" => Some("low".to_string()),
        // Pre-M101 findings had non-standard severities; map
        // anything else to "medium" so the sidecar still has a
        // hunk-renderable confidence value. Findings validate at
        // add time (FindingDraft::validate) so the live case is
        // always one of the three documented values.
        _ => None,
    }
}

/// Pick a non-empty summary between the explicit `--summary` and
/// the finding description's first non-empty line. The hunk-side
/// schema requires a non-empty `summary`, so an empty input would
/// be rejected at apply time — we filter at this layer instead.
fn pick_summary(summary: &str, description: &str) -> String {
    if !summary.trim().is_empty() {
        return summary.trim().to_string();
    }
    description
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    //! AC-03 / AC-06: the live batch + sidecar shapes are pinned by
    //! these unit tests. They run without a PlanContext (the
    //! pure builders `finding_to_hunk_comment` / `build_sidecar` /
    //! `anchor_to_ranges` / `severity_to_confidence` / `pick_summary`
    //! are all pure functions over FindingAnchor / Finding /
    //! ReviewComment / &str).
    use super::*;
    use crate::model::{FindingAnchor, Range};

    fn anchor(path: &str, line: u32, side: &str) -> Option<FindingAnchor> {
        Some(FindingAnchor {
            path: path.to_string(),
            commit: String::new(),
            new_range: if side == "new" {
                Some(Range {
                    start_line: line,
                    end_line: line,
                })
            } else {
                None
            },
            old_range: if side == "old" {
                Some(Range {
                    start_line: line,
                    end_line: line,
                })
            } else {
                None
            },
            hunk_index: None,
            side: Some(side.to_string()),
        })
    }

    fn finding_with(id: &str, sev: &str, summary: &str, anc: Option<FindingAnchor>) -> Finding {
        Finding {
            id: id.to_string(),
            severity: sev.to_string(),
            category: "correctness".to_string(),
            description: format!("desc for {id}"),
            status: "open".to_string(),
            author: "test".to_string(),
            fixed_in: String::new(),
            created: "2026-07-15T00:00:00+00:00".to_string(),
            resolved: String::new(),
            phase: "external".to_string(),
            anchor: anc,
            thread: vec![],
            summary: summary.to_string(),
            rationale: "rationale text".to_string(),
            confidence: sev.to_string(),
            tags: vec![],
        }
    }

    fn comment_with(id: &str, body: &str, anc: Option<FindingAnchor>) -> ReviewComment {
        ReviewComment {
            id: id.to_string(),
            milestone_id: "M154".to_string(),
            author: "reviewer".to_string(),
            body: body.to_string(),
            finding_id: String::new(),
            created_at: "2026-07-15T00:00:00+00:00".to_string(),
            anchor: anc,
        }
    }

    /// AC-03: a single finding produces one live-batch entry with
    /// the documented shape: `filePath`, `newLine`, `summary`,
    /// `rationale`, `author`. `confidence` is informational on the
    /// live batch.
    #[test]
    fn live_batch_entry_shape_pinned() {
        let f = finding_with(
            "F-01",
            "high",
            "summary text",
            anchor("crates/mp/src/install.rs", 42, "new"),
        );
        let author = "mp";
        let entry = finding_to_hunk_comment(&f, author).expect("entry");
        assert_eq!(entry.file_path.as_deref(), Some("crates/mp/src/install.rs"));
        assert_eq!(entry.new_line, Some(42));
        assert_eq!(entry.summary, "summary text");
        assert_eq!(entry.rationale, "rationale text");
        assert_eq!(entry.author, "mp");
        assert_eq!(entry.confidence.as_deref(), Some("high"));
    }

    /// AC-03: side=old maps to old_range, not new_range.
    #[test]
    fn side_old_maps_to_old_range_in_sidecar() {
        let f = finding_with(
            "F-02",
            "low",
            "stale comment note",
            anchor("src/foo.rs", 7, "old"),
        );
        let annotation = finding_to_hunk_annotation(&f, "mp").expect("annotation");
        assert!(
            annotation.old_range.is_some(),
            "old_range populated for side=old"
        );
        let r = annotation.old_range.unwrap();
        assert_eq!(r, [7, 7]);
        assert!(
            annotation.new_range.is_none(),
            "new_range absent on side=old"
        );
        assert_eq!(annotation.confidence, "low");
    }

    /// AC-06: sidecar shape pinned. version=1, files[].annotations[]
    /// carry the [start,end] tuple shape hunk consumes.
    #[test]
    fn sidecar_shape_pinned() {
        let findings = vec![finding_with(
            "F-01",
            "high",
            "summary text",
            anchor("src/foo.rs", 42, "new"),
        )];
        let comments = vec![comment_with(
            "C-01",
            "comment body",
            anchor("src/bar.rs", 7, "new"),
        )];
        let mut cfg = ProjectConfig::default();
        cfg.review.hunk = true;
        cfg.review.hunk_author = "mp-test".to_string();
        let sidecar = build_sidecar(&findings, &comments, &cfg, "M154").expect("sidecar");
        assert_eq!(sidecar.version, 1);
        assert_eq!(sidecar.files.len(), 2, "one file per anchor");
        let by_path: std::collections::HashMap<_, _> =
            sidecar.files.iter().map(|f| (f.path.as_str(), f)).collect();
        assert!(by_path.contains_key("src/foo.rs"));
        assert!(by_path.contains_key("src/bar.rs"));
        let foo = by_path["src/foo.rs"];
        assert_eq!(foo.annotations.len(), 1);
        assert_eq!(foo.annotations[0].new_range, Some([42, 42]));
    }

    /// AC-06: unanchored findings group under a synthetic per-
    /// milestone pseudo-path so they surface in the sidecar
    /// instead of dropping on the floor. hunk renders the
    /// annotation with no range (file-level summary note).
    #[test]
    fn unanchored_findings_group_under_synthetic_path() {
        let findings = vec![finding_with("F-01", "medium", "design-level note", None)];
        let cfg = ProjectConfig::default();
        let sidecar = build_sidecar(&findings, &[], &cfg, "M154").expect("sidecar");
        assert_eq!(sidecar.files.len(), 1);
        assert!(
            sidecar.files[0].path.starts_with("__milestone-"),
            "synthetic path used; got {:?}",
            sidecar.files[0].path
        );
        assert_eq!(sidecar.files[0].annotations.len(), 1);
        let ann = &sidecar.files[0].annotations[0];
        assert!(
            ann.new_range.is_none() && ann.old_range.is_none(),
            "file-level notes have no range; got new={:?} old={:?}",
            ann.new_range,
            ann.old_range
        );
    }

    /// AC-03: severity -> confidence mapping is exact. high=high,
    /// medium=medium, low=low. Any other value yields None (which
    /// gets filtered out at apply time).
    #[test]
    fn severity_confidence_mapping_is_exact() {
        assert_eq!(severity_to_confidence("high").as_deref(), Some("high"));
        assert_eq!(severity_to_confidence("medium").as_deref(), Some("medium"));
        assert_eq!(severity_to_confidence("low").as_deref(), Some("low"));
        assert!(severity_to_confidence("critical").is_none());
        assert!(severity_to_confidence("").is_none());
    }

    /// AC-03: pick_summary prefers the explicit `--summary`; falls
    /// back to the first non-empty line of `--desc`; returns "" if
    /// both are empty (and the calling builder returns None for the
    /// entry, filtering empty comments from the batch).
    #[test]
    fn pick_summary_prefers_summary_then_desc() {
        assert_eq!(pick_summary("explicit", "ignored"), "explicit");
        assert_eq!(pick_summary("", "  \nfirst\nsecond"), "first");
        assert_eq!(pick_summary("   ", "   "), "");
    }

    /// AC-06: pre-M154 comments (no anchor field) round-trip
    /// through build_sidecar without losing data.
    #[test]
    fn pre_m154_unanchored_comment_surfaces_as_file_level_note() {
        // Pre-M154 on-disk: anchor is None (the field doesn't exist
        // on the JSON). The reviewer comment was a milestone-level
        // thread entry, no file.
        let comment = ReviewComment {
            id: "C-01".to_string(),
            milestone_id: "M154".to_string(),
            author: "reviewer".to_string(),
            body: "overall design feedback".to_string(),
            finding_id: String::new(),
            created_at: "2026-07-10T00:00:00+00:00".to_string(),
            anchor: None,
        };
        let cfg = ProjectConfig::default();
        let sidecar = build_sidecar(&[], &[comment], &cfg, "M154").expect("sidecar");
        // Unanchored comment surfaces under the synthetic path; no
        // range; body becomes the summary.
        assert_eq!(sidecar.files.len(), 1);
        assert_eq!(sidecar.files[0].annotations.len(), 1);
        assert_eq!(
            sidecar.files[0].annotations[0].summary,
            "overall design feedback"
        );
    }

    /// AC-06: empty findings (no summary / no rationale / no body)
    /// are filtered — the export shouldn't carry empty entries that
    /// hunk would reject at apply time.
    #[test]
    fn empty_findings_and_comments_are_filtered() {
        // Strip BOTH summary and rationale; pick_summary's fallback
        // is empty (description is "" too), so the builder returns
        // None.
        let f_no_text = Finding {
            id: "F-01".to_string(),
            severity: "high".to_string(),
            category: "correctness".to_string(),
            description: String::new(),
            status: "open".to_string(),
            author: "test".to_string(),
            fixed_in: String::new(),
            created: "2026-07-15T00:00:00+00:00".to_string(),
            resolved: String::new(),
            phase: "external".to_string(),
            anchor: None,
            thread: vec![],
            summary: String::new(),
            rationale: String::new(),
            confidence: "high".to_string(),
            tags: vec![],
        };
        let c_no_body = comment_with("C-01", "", None);

        assert!(finding_to_hunk_comment(&f_no_text, "mp").is_none());
        assert!(finding_to_hunk_annotation(&f_no_text, "mp").is_none());
        assert!(comment_to_hunk_comment(&c_no_body, "mp").is_none());
        assert!(comment_to_hunk_annotation(&c_no_body, "mp").is_none());
    }
}
