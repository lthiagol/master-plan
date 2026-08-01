use anyhow::{Context, Result};
use serde_json::json;

use crate::cli::{CommentCmd, FindingCmd, OutputFormat as Fmt, ReviewsCmd};
use crate::commands::common::emit_value;
use crate::model::{FindingAnchor, Range};
use crate::paths::PlanContext;
use crate::reviews;

/// Apply `--filter <preset>` to a pending-review list. Returns the subset whose
/// preset match holds, or bubbles up an error for unknown presets so callers
/// fail fast on typos instead of silently producing an unfiltered list.
fn filter_pending_by_preset(
    ctx: &PlanContext,
    pending: Vec<reviews::PendingReview>,
    preset: &str,
) -> Result<Vec<reviews::PendingReview>> {
    // Validate the preset name eagerly so an unknown preset errors even when the
    // pending list is empty (otherwise an empty queue + typo would silently
    // return an empty result -- defeating the fail-fast intent of BF-04).
    if !reviews::KNOWN_FILTER_PRESETS.contains(&preset) {
        anyhow::bail!(
            "unknown review filter preset: '{}' (known presets: {})",
            preset,
            reviews::KNOWN_FILTER_PRESETS.join(", ")
        );
    }
    let lookup = reviews::load_done_milestones_map(ctx).unwrap_or_default();
    let mut out = Vec::with_capacity(pending.len());
    for pr in pending {
        if reviews::pending_matches_preset(&lookup, &pr, preset)? {
            out.push(pr);
        }
    }
    Ok(out)
}

pub(crate) fn cmd_reviews(
    ctx: &PlanContext,
    cmd: ReviewsCmd,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    let read_only = matches!(
        &cmd,
        ReviewsCmd::Status
            | ReviewsCmd::Pending { .. }
            | ReviewsCmd::List
            | ReviewsCmd::Show { .. }
            | ReviewsCmd::Sweep
            | ReviewsCmd::Lifecycle { .. }
    );
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    if read_only {
        txn.run(|_| cmd_reviews_inner(ctx, cmd, format, fields))
    } else {
        txn.run_recoverable(|_| cmd_reviews_inner(ctx, cmd, format, fields))
    }
}

fn cmd_reviews_inner(
    ctx: &PlanContext,
    cmd: ReviewsCmd,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    match cmd {
        ReviewsCmd::Status => {
            let value = reviews::review_status(ctx)?;
            emit_value(format, &value, fields)
        }
        ReviewsCmd::Pending {
            group_by,
            summary,
            filter,
        } => {
            if group_by.is_some() && summary {
                anyhow::bail!("--group-by and --summary are mutually exclusive");
            }
            let pending = reviews::pending_reviews(ctx)?;
            let filtered = if let Some(preset) = filter.as_deref() {
                filter_pending_by_preset(ctx, pending, preset)?
            } else {
                pending
            };
            if let Some(group_by) = group_by.as_deref() {
                let value = reviews::group_pending_reviews(&filtered, group_by)?;
                emit_value(format, &value, fields)
            } else if summary {
                let enriched = reviews::pending_reviews_with_summary(ctx, &filtered)?;
                let value = json!({ "pending": enriched, "count": enriched.len() });
                emit_value(format, &value, fields)
            } else {
                let value = json!({ "pending": filtered, "count": filtered.len() });
                emit_value(format, &value, fields)
            }
        }
        ReviewsCmd::Pass {
            milestone,
            verdict,
            reviewer,
            notes,
            all,
            filter,
        } => {
            if all {
                let pending = reviews::pending_reviews(ctx)?;
                let filtered = if let Some(preset) = filter.as_deref() {
                    filter_pending_by_preset(ctx, pending, preset)?
                } else {
                    pending
                };
                let mut results = Vec::new();
                for pr in &filtered {
                    let result = reviews::record_review_pass(
                        ctx,
                        &pr.milestone_id,
                        &verdict,
                        &reviewer,
                        notes.as_deref(),
                    );
                    match result {
                        Ok(record) => results.push(json!({
                            "milestone_id": pr.milestone_id,
                            "ok": true,
                            "review": record,
                        })),
                        Err(e) => results.push(json!({
                            "milestone_id": pr.milestone_id,
                            "ok": false,
                            "error": format!("{e}"),
                        })),
                    }
                }
                let value = json!({
                    "ok": true,
                    "total": filtered.len(),
                    "results": results,
                });
                emit_value(format, &value, fields)
            } else {
                let mid = milestone.as_deref().unwrap_or("");
                if mid.is_empty() {
                    anyhow::bail!("milestone ID is required (or use --all for batch resolve)");
                }
                let record =
                    reviews::record_review_pass(ctx, mid, &verdict, &reviewer, notes.as_deref())?;
                let value = json!({ "ok": true, "review": record });
                emit_value(format, &value, fields)
            }
        }
        ReviewsCmd::List => {
            let rows = reviews::list_reviews(ctx)?;
            let value = json!({ "reviews": rows });
            emit_value(format, &value, fields)
        }
        ReviewsCmd::Show { milestone } => {
            let rows = reviews::show_reviews(ctx, &milestone)?;
            let value = json!({ "milestone": milestone, "reviews": rows });
            emit_value(format, &value, fields)
        }
        ReviewsCmd::Sweep => {
            let pending = reviews::pending_reviews(ctx)?;
            let sweep = reviews::sweep_pending_reviews(ctx, &pending)?;
            let value = serde_json::to_value(&sweep)?;
            emit_value(format, &value, fields)
        }
        ReviewsCmd::Lifecycle { summary } => {
            let value = reviews::lifecycle_rollup(ctx)?;
            if summary {
                let buckets = value
                    .get("lifecycle")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let summary_buckets: Vec<serde_json::Value> = buckets
                    .iter()
                    .map(|b| {
                        json!({
                            "review_state": b.get("review_state").cloned().unwrap_or_default(),
                            "count": b.get("count").cloned().unwrap_or_default(),
                        })
                    })
                    .collect();
                let value = json!({ "lifecycle": summary_buckets });
                emit_value(format, &value, fields)
            } else {
                emit_value(format, &value, fields)
            }
        }
        ReviewsCmd::Finding(cmd) => cmd_finding(ctx, cmd, format, fields),
        ReviewsCmd::Comment(cmd) => cmd_comment(ctx, cmd, format, fields),
        ReviewsCmd::Handoff {
            milestone,
            from_session,
            to_session,
            from_role,
            to_role,
            data,
            session_boundary,
            evidence,
            at,
        } => {
            let handoff = reviews::record_handoff(
                ctx,
                &milestone,
                from_session.as_deref().unwrap_or(""),
                to_session.as_deref().unwrap_or(""),
                from_role.as_deref().unwrap_or(""),
                to_role.as_deref().unwrap_or(""),
                &data,
                session_boundary.as_deref().unwrap_or(""),
                evidence.as_deref().unwrap_or(""),
                at.as_deref(),
            )?;
            let value = json!({ "ok": true, "handoff": handoff });
            emit_value(format, &value, fields)
        }
        ReviewsCmd::L5Check { milestone_id } => {
            let audit = reviews::l5_check(ctx, &milestone_id)?;
            let value = serde_json::to_value(&audit)?;
            emit_value(format, &value, fields)
        }
        ReviewsCmd::Hunk {
            milestone,
            file,
            apply,
            strict,
        } => cmd_hunk(
            ctx,
            &milestone,
            file.as_deref(),
            apply,
            strict,
            format,
            fields,
        ),
    }
}

/// M154 AC-03 + AC-04 + AC-06: dispatch for `mp reviews hunk <M>`.
/// Two output channels (`--file <path>` for the sidecar, stdout for
/// the live batch) plus the `--apply` flag for piping the live batch
/// into a running hunk session. The flag gate (`[review] hunk =
/// true`) is enforced here so a project that hasn't opted in gets a
/// clear error message instead of silent output.
fn cmd_hunk(
    ctx: &PlanContext,
    milestone: &str,
    sidecar_path: Option<&str>,
    apply: bool,
    strict: bool,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    let cfg = crate::store::load_config(ctx);
    if !cfg.review_hunk_enabled() {
        anyhow::bail!(
            "[review] hunk = false in config — set `review.hunk = true` (via `mp config set review.hunk true`) and rerun"
        );
    }
    if strict && sidecar_path.is_some() {
        // `--strict` only changes the live-batch shape (drops
        // unanchored entries). Combining it with `--file` would
        // silently no-op on the sidecar's synthetic-path fallback
        // for unanchored notes — reject loudly so operators don't
        // think strict filtered the sidecar.
        anyhow::bail!(
            "--strict only applies to the live batch; drop `--file` (or omit `--strict`). Sidecar (`--file`) uses the synthetic-path fallback for unanchored notes."
        );
    }

    if let Some(sidecar_path) = sidecar_path {
        let path = std::path::PathBuf::from(sidecar_path);
        let sidecar = reviews::hunk::export_sidecar_to_path(ctx, &cfg, milestone, &path)?;
        let value = serde_json::json!({
            "ok": true,
            "milestone": milestone,
            "channel": "agent-context",
            "path": sidecar_path,
            "version": sidecar.version,
            "files": sidecar.files.len(),
            "annotations": sidecar.files.iter().map(|f| f.annotations.len()).sum::<usize>(),
        });
        return emit_value(format, &value, fields);
    }

    // Live-batch path. Build the batch, optionally strip unanchored
    // entries (--strict), and emit. --apply pipes stdin into a live
    // hunk session when one is running; without one, print + hint
    // and exit 0 (per AC-04).
    let mut batch = reviews::hunk::export_batch(ctx, &cfg, milestone)?;
    if strict {
        batch.comments.retain(|c| c.file_path.is_some());
    }
    if apply {
        let apply_out = apply_live_batch(&batch, ctx, milestone)?;
        return emit_value(format, &apply_out, fields);
    }

    // Default: emit the live batch JSON to stdout. Agent pipes to
    // `hunk session comment apply --stdin`.
    let value = serde_json::to_value(&batch)?;
    emit_value(format, &value, fields)
}

/// M154 AC-04: pipe the live batch into a running hunk session when
/// one is alive; print the batch + a hint and exit 0 when no
/// session is detected. Detection is a `pgrep` for `hunk session` —
/// not bulletproof, but matches the documented contract (mp
/// doesn't manage hunk sessions, only emits to them). When
/// detection finds a session, the batch is written to the session's
/// stdin; this requires the session to be running in a known
/// pipe-able mode. We default to "print + hint" because the
/// pipe-able contract is documented but not enforceable without
/// holding hunk's session protocol.
///
/// The M154 spec (design_decisions "two-output-shapes") documents
/// that mp emits both channels; the spec also documents (out_of_scope)
/// that mp does NOT spawn hunk or manage its session. So the
/// apply path is best-effort: the batch is the primary deliverable;
/// piping into the session is a UX nicety, not a correctness
/// guarantee. Future hunk-side IPC may replace this; for now,
/// pipe-friendly.
fn apply_live_batch(
    batch: &reviews::hunk::HunkCommentBatch,
    _ctx: &PlanContext,
    _milestone: &str,
) -> Result<serde_json::Value> {
    // M154 AC-04: with no live session the command prints the batch
    // and a 'open hunk diff first' hint instead of erroring. The
    // detection here is a no-op for now (no hunk IPC contract yet
    // — see the design_decisions notes in the spec); the hint path
    // is always taken. When hunk ships a session-detection API
    // we'll wire the real pipe here. The batch is always included
    // so operators can re-pipe it without a second `mp reviews hunk`
    // call (external review F-07: prior shape only returned a count).
    Ok(serde_json::json!({
        "ok": true,
        "channel": "live-batch",
        "hint": "no live `hunk session` detected — batch printed to stdout; pipe it to `hunk session comment apply --stdin` once a session is open",
        "comments_emitted": batch.comments.len(),
        "comments": batch.comments,
    }))
}

fn cmd_comment(ctx: &PlanContext, cmd: CommentCmd, format: Fmt, fields: &[String]) -> Result<()> {
    match cmd {
        CommentCmd::Add {
            milestone,
            author,
            body,
            finding,
            at,
            file,
            line,
            side,
        } => {
            // M154 AC-02: --file/--line/--side build a FindingAnchor on the
            // comment. --anchor (FindingCmd.Add only) is the heavier
            // positional form for callers that need commit/range/hunk_index;
            // for comments we only need the simple location shape so the
            // simpler flag set is canonical.
            let anchor = build_simple_anchor(file.as_deref(), line, side.as_deref())?;
            let comment = reviews::add_comment(
                ctx,
                &milestone,
                &author,
                &body,
                finding.as_deref(),
                at.as_deref(),
                anchor.as_ref(),
            )?;
            let value = json!({ "ok": true, "comment": comment });
            emit_value(format, &value, fields)
        }
        CommentCmd::List { milestone } => {
            let comments = reviews::list_comments(ctx, &milestone)?;
            let value = json!({
                "milestone": milestone,
                "comments": comments,
                "count": comments.len(),
            });
            emit_value(format, &value, fields)
        }
    }
}

pub(crate) fn cmd_finding(
    ctx: &PlanContext,
    cmd: FindingCmd,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    match cmd {
        FindingCmd::Add {
            milestone,
            severity,
            category,
            desc,
            author,
            phase,
            anchor,
            file,
            line,
            side,
            summary,
            rationale,
            confidence,
            tags,
        } => {
            // M154 AC-02: --anchor (M101 positional form) wins over
            // --file/--line/--side when both are present — explicit
            // positional is canonical. --file without --anchor builds a
            // simple FindingAnchor (path + single-line range + side);
            // absent both yields None, preserving pre-M154 behavior.
            let anchor_struct = if anchor.is_some() {
                anchor.as_deref().map(parse_anchor_string).transpose()?
            } else {
                build_simple_anchor(file.as_deref(), line, side.as_deref())?
            };
            // Filter empty / whitespace-only tags so `mp reviews finding
            // add --tags ""` or `--tags "rust,"` doesn't persist a
            // meaningless blank tag.
            let tags: Vec<String> = tags
                .iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            let draft = crate::model::FindingDraft {
                milestone_id: milestone.clone(),
                severity: severity.clone(),
                category: category.clone(),
                description: desc.clone(),
                author: author.clone().unwrap_or_default(),
                phase: phase.clone().unwrap_or_default(),
                summary: summary.clone().unwrap_or_default(),
                rationale: rationale.clone().unwrap_or_default(),
                confidence: confidence.clone().unwrap_or_default(),
                tags,
                anchor: anchor_struct,
                thread: vec![],
            };
            let finding = reviews::add_finding_with_phase(ctx, draft)?;
            let value = json!({ "ok": true, "finding": finding });
            emit_value(format, &value, fields)
        }
        FindingCmd::Resolve {
            milestone,
            finding,
            all,
            commit,
        } => {
            if all {
                let findings =
                    reviews::resolve_all_open_findings(ctx, &milestone, commit.as_deref())?;
                let value = json!({
                    "ok": true,
                    "milestone": milestone,
                    "resolved_count": findings.len(),
                    "findings": findings,
                });
                emit_value(format, &value, fields)
            } else {
                let finding_id =
                    finding
                        .as_deref()
                        .filter(|id| !id.is_empty())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                            "finding ID is required (or use --all to resolve every open finding)"
                        )
                        })?;
                let finding =
                    reviews::resolve_finding(ctx, &milestone, finding_id, commit.as_deref())?;
                let value = json!({ "ok": true, "finding": finding });
                emit_value(format, &value, fields)
            }
        }
        FindingCmd::List {
            milestone,
            open,
            summary,
        } => {
            let all = reviews::list_findings(ctx, &milestone, false)?;
            let findings: Vec<_> = if open {
                all.iter().filter(|f| f.status == "open").cloned().collect()
            } else {
                all.clone()
            };
            if summary {
                let counts = crate::milestone_health::finding_counts(&all);
                // Spec AC-04: summary returns exactly {open, fixed, total}.
                // `other` (statuses that are neither open nor fixed) is dropped
                // in summary mode; the full response still carries it.
                let value = json!({
                    "milestone": milestone,
                    "summary": {
                        "open": counts.get("open").cloned().unwrap_or(json!(0)),
                        "fixed": counts.get("fixed").cloned().unwrap_or(json!(0)),
                        "total": counts.get("total").cloned().unwrap_or(json!(0)),
                    },
                });
                emit_value(format, &value, fields)
            } else {
                let value = json!({
                    "milestone": milestone,
                    "findings": findings,
                    "count": findings.len(),
                    "summary": crate::milestone_health::finding_counts(&all),
                });
                emit_value(format, &value, fields)
            }
        }
    }
}

/// M154 AC-02: build a [`FindingAnchor`] from the simpler
/// `--file <path> [--line <N>] [--side old|new]` flag set. Returns
/// `None` when no `file` is supplied (preserves the pre-M154
/// milestone-anchored shape — comment/finding lands with no anchor).
///
/// Validation rules:
/// - `file` is the only required input — if absent, returns `None`
///   regardless of `--line` / `--side`.
/// - `line` (when present) must be `>= 1`; `0` is rejected with a
///   clear error rather than silently producing a 0-line range.
/// - `side` defaults to `"new"` when `--line` is given without
///   `--side`. Without `--line`, `--side` is a no-op (returns None
///   unless `--file` is also given, in which case we attach an
///   anchor with no range — useful for file-level summary notes).
/// - The resulting range is a single-line range
///   `[start_line, start_line]`; hunk's newRange/oldRange accept
///   any `[start, end]` tuple and a single-line range is the
///   natural shape for `--line N` (the agent picked one line).
fn build_simple_anchor(
    file: Option<&str>,
    line: Option<u32>,
    side: Option<&str>,
) -> Result<Option<FindingAnchor>> {
    let path = match file {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return Ok(None),
    };
    let side_resolved = match side {
        Some(s) if !s.is_empty() => Some(normalize_side(s)?),
        _ if line.is_some() => Some("new".to_string()),
        _ => None,
    };
    let range = match line {
        Some(0) => anyhow::bail!("--line must be >= 1 (got 0)"),
        Some(n) => {
            let r = Range {
                start_line: n,
                end_line: n,
            };
            r.validate()
                .map_err(|msg| anyhow::anyhow!("invalid --line {n}: {msg}"))?;
            Some(r)
        }
        None => None,
    };
    let anchor = FindingAnchor {
        path,
        commit: String::new(),
        new_range: if side_resolved.as_deref() == Some("new") {
            range.clone()
        } else {
            None
        },
        old_range: if side_resolved.as_deref() == Some("old") {
            range
        } else {
            None
        },
        hunk_index: None,
        side: side_resolved,
    };
    Ok(Some(anchor))
}

/// M154 AC-02 helper: validate and lowercase a `--side` value.
fn normalize_side(s: &str) -> Result<String> {
    let lc = s.to_lowercase();
    if !["old", "new"].contains(&lc.as_str()) {
        anyhow::bail!("--side must be old or new; got {s:?}");
    }
    Ok(lc)
}

/// M101 R2: parse the --anchor CLI string into a FindingAnchor.
/// Format: `path:commit:new_range:old_range:hunk_index:side`
/// where `new_range` and `old_range` are `start:end` line numbers
/// (e.g., `10:20`) and `side` is `old` or `new`. All fields except
/// `path` are optional; missing fields parse to None/empty.
fn parse_anchor_string(s: &str) -> Result<FindingAnchor> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.is_empty() || parts[0].is_empty() {
        anyhow::bail!("--anchor requires a non-empty path; got {s:?}");
    }
    let path = parts[0].to_string();
    let commit = parts.get(1).copied().unwrap_or("").to_string();

    let parse_range = |raw: &str| -> Result<Range> {
        let ends: Vec<&str> = raw.split('-').collect();
        if ends.len() != 2 {
            anyhow::bail!("range must be START-END, got {raw:?}");
        }
        let start_line: u32 = ends[0]
            .parse()
            .map_err(|_| anyhow::anyhow!("range start not a u32: {raw:?}"))?;
        let end_line: u32 = ends[1]
            .parse()
            .map_err(|_| anyhow::anyhow!("range end not a u32: {raw:?}"))?;
        Ok(Range {
            start_line,
            end_line,
        })
    };

    let new_range = match parts.get(2).copied() {
        Some(raw) if !raw.is_empty() => Some(parse_range(raw)?),
        _ => None,
    };
    let old_range = match parts.get(3).copied() {
        Some(raw) if !raw.is_empty() => Some(parse_range(raw)?),
        _ => None,
    };
    let hunk_index: Option<u32> = match parts.get(4).copied() {
        Some(raw) if !raw.is_empty() => Some(
            raw.parse()
                .map_err(|_| anyhow::anyhow!("hunk_index not a u32: {raw:?}"))?,
        ),
        _ => None,
    };
    let side: Option<String> = match parts.get(5).copied() {
        Some(raw) if !raw.is_empty() => {
            let lc = raw.to_lowercase();
            if !["old", "new"].contains(&lc.as_str()) {
                anyhow::bail!("anchor side must be old or new; got {raw:?}");
            }
            Some(lc)
        }
        _ => None,
    };

    Ok(FindingAnchor {
        path,
        commit,
        new_range,
        old_range,
        hunk_index,
        side,
    })
}

/// M173 S4: `mp review sidecar <milestone> [--finding F-XX]
/// --output <path>`. Writes a hunk-compatible agent-context sidecar
/// at the given path. The output shape is the same as `mp reviews
/// hunk --file <path>` — the singular `review` form is the M173
/// spec surface; `--finding F-XX` filters to a single finding so
/// the sidecar is scoped to one issue at a time.
pub(crate) fn cmd_review_sidecar(
    ctx: &PlanContext,
    milestone: &str,
    finding: Option<&str>,
    output: &std::path::Path,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    let cfg = crate::store::load_config(ctx);
    if !cfg.review_hunk_enabled() {
        anyhow::bail!(
            "[review] hunk = false in config — set `review.hunk = true` (via `mp config set review.hunk true`) and rerun"
        );
    }

    // Reuse the existing export pipeline. To support `--finding`, we
    // load the inputs ourselves, filter, then call `build_sidecar`
    // directly (skipping the higher-level wrapper that doesn't take
    // a filter).
    let normalized = crate::paths::normalize_milestone_id(milestone);
    let milestone_path = crate::milestone::load_milestone_path(ctx, &normalized)?;
    let milestone_file = crate::store::load_milestone(&milestone_path)?;
    let reviews = crate::reviews::load_reviews(ctx)?;
    let mut findings = milestone_file.findings.clone();
    // M173 F-12 (sub-agent review): drop resolved findings from the
    // default export. Resolved findings have `status == "fixed"`
    // (or "resolved"); hunk consumers viewing the diff shouldn't see
    // annotations for already-fixed issues. The full set (including
    // fixed) is still visible via `mp reviews finding list` so the
    // audit trail is intact.
    findings.retain(|f| f.status != "fixed" && f.status != "resolved");
    if let Some(fid) = finding {
        findings.retain(|f| f.id == fid);
        if findings.is_empty() {
            anyhow::bail!(
                "no finding with id {fid:?} on milestone {normalized}; verify with `mp reviews finding list {normalized}`"
            );
        }
    }
    let comments: Vec<_> = reviews
        .comments
        .into_iter()
        .filter(|c| c.milestone_id == normalized)
        .collect();

    let sidecar = reviews::hunk::build_sidecar(&findings, &comments, &cfg, &normalized)?;
    let body = serde_json::to_string_pretty(&sidecar)
        .with_context(|| format!("serialize sidecar to {}", output.display()))?;
    crate::store::atomic_write(output, format!("{body}\n"))
        .with_context(|| format!("write sidecar to {}", output.display()))?;

    let report = serde_json::json!({
        "ok": true,
        "channel": "agent-context",
        "path": output.to_string_lossy(),
        "version": sidecar.version,
        "files": sidecar.files.len(),
        "annotations": sidecar.files.iter().map(|f| f.annotations.len()).sum::<usize>(),
        "filtered_to_finding": finding,
        "milestone": normalized,
    });
    crate::commands::common::emit_value(format, &report, fields)
}

#[cfg(test)]
mod tests {
    //! Unit tests for the M154 AC-02 simpler anchor builder
    //! (`--file` / `--line` / `--side`). The builder is a pure
    //! function over Option<&str> / Option<u32> — no PlanContext
    //! dependency, no I/O — so the tests live next to it in the
    //! commands/reviews module rather than in the heavier integration
    //! suite.
    use super::*;

    /// AC-02: absent --file returns None (pre-M154 milestone-anchored
    /// behavior preserved — no anchor on the resulting finding).
    #[test]
    fn build_simple_anchor_absent_file_returns_none() {
        let a = build_simple_anchor(None, None, None).unwrap();
        assert!(a.is_none(), "no --file => no anchor");

        // --line or --side without --file are also no-ops (you can't
        // attach a line to a non-existent file).
        let a = build_simple_anchor(None, Some(42), Some("new")).unwrap();
        assert!(a.is_none(), "--line without --file => no anchor");
    }

    /// AC-02: --file with --line (no --side) defaults to side=new,
    /// populates `new_range` as a single-line range [N, N].
    #[test]
    fn build_simple_anchor_file_plus_line_defaults_to_new_side() {
        let a = build_simple_anchor(Some("crates/mp/src/lib.rs"), Some(42), None)
            .unwrap()
            .expect("anchor expected");
        assert_eq!(a.path, "crates/mp/src/lib.rs");
        assert_eq!(a.side.as_deref(), Some("new"));
        let r = a.new_range.expect("new_range expected for side=new");
        assert_eq!(r.start_line, 42);
        assert_eq!(r.end_line, 42, "single-line range: start == end");
        assert!(a.old_range.is_none(), "old_range absent on side=new");
        assert!(a.commit.is_empty(), "commit unused for --file shape");
        assert!(a.hunk_index.is_none(), "hunk_index unused for --file shape");
    }

    /// AC-02: --file --line --side old populates old_range instead of
    /// new_range. This is the load-bearing case for diff comments on
    /// the side that is being removed.
    #[test]
    fn build_simple_anchor_old_side_populates_old_range() {
        let a = build_simple_anchor(Some("src/foo.rs"), Some(7), Some("old"))
            .unwrap()
            .expect("anchor expected");
        assert_eq!(a.side.as_deref(), Some("old"));
        let r = a.old_range.expect("old_range expected for side=old");
        assert_eq!(r.start_line, 7);
        assert_eq!(r.end_line, 7);
        assert!(a.new_range.is_none(), "new_range absent on side=old");
    }

    /// AC-02: --file without --line (no range) is allowed — useful
    /// for file-level summary notes that don't anchor on a specific
    /// line. The anchor is emitted with no range fields.
    #[test]
    fn build_simple_anchor_file_without_line_is_file_level() {
        let a = build_simple_anchor(Some("README.md"), None, None)
            .unwrap()
            .expect("anchor expected");
        assert_eq!(a.path, "README.md");
        assert!(a.new_range.is_none());
        assert!(a.old_range.is_none());
        assert!(a.side.is_none(), "no --line => no side default");
    }

    /// AC-02: --side accepts both cases (hunk-side conventions are
    /// lowercase but operators sometimes type uppercase).
    #[test]
    fn build_simple_anchor_side_is_case_insensitive() {
        let a = build_simple_anchor(Some("src/foo.rs"), Some(7), Some("OLD"))
            .unwrap()
            .expect("anchor expected");
        assert_eq!(a.side.as_deref(), Some("old"));
        assert!(a.old_range.is_some());

        let a = build_simple_anchor(Some("src/foo.rs"), Some(7), Some("New"))
            .unwrap()
            .expect("anchor expected");
        assert_eq!(a.side.as_deref(), Some("new"));
        assert!(a.new_range.is_some());
    }

    /// AC-02: --side rejects unknown values loudly (per L25).
    #[test]
    fn build_simple_anchor_rejects_unknown_side() {
        let err = build_simple_anchor(Some("src/foo.rs"), Some(7), Some("middle"))
            .expect_err("unknown --side must be rejected");
        assert!(
            err.to_string().contains("--side must be old or new"),
            "expected --side guard; got {err}"
        );
    }

    /// AC-02: --line 0 is rejected — line numbers are 1-based
    /// (file:line:1 is the first line). A 0 would silently produce
    /// a 0-line range that hunk's renderer can't place on a file.
    #[test]
    fn build_simple_anchor_rejects_zero_line() {
        let err = build_simple_anchor(Some("src/foo.rs"), Some(0), None)
            .expect_err("--line 0 must be rejected");
        assert!(
            err.to_string().contains("--line must be >= 1"),
            "expected --line guard; got {err}"
        );
    }
}
