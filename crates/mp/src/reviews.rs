use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

use crate::model::{next_comment_id, next_handoff_id, FindingAnchor, ReviewComment, ReviewHandoff};
use crate::paths::{self, PlanContext};
use crate::store;
use crate::validate::{effective_execution_status, effective_spec_status};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReviewsFile {
    #[serde(default)]
    pub reviews: Vec<ReviewRecord>,
    /// M133 AC-01: structured review comments per milestone. Additive —
    /// pre-M133 on-disk files have no `comments` key, and `#[serde(default)]`
    /// round-trips them as an empty Vec.
    #[serde(default)]
    pub comments: Vec<ReviewComment>,
    /// M133 AC-02: structured coordinator/runner hand-off records per
    /// milestone. Same additive shape as `comments`.
    #[serde(default)]
    pub handoffs: Vec<ReviewHandoff>,
}

/// M154: hunk export pipeline. `mp reviews hunk <M>` renders the
/// milestone's findings + comments as hunk-compatible annotations —
/// either a live `comment apply` JSON batch (stdout) or an
/// `--agent-context` sidecar (file). See the module-level docs in
/// `hunk.rs` for the two output shapes and the rationale behind the
/// findings-driven export (M133 comments have 0 historical entries
/// in reviews.json; findings drive the pipeline).
pub mod hunk;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub milestone_id: String,
    pub verdict: String,
    pub reviewer: String,
    pub reviewed_at: String,
    #[serde(default)]
    pub notes: String,
    pub milestone_completed_at: String,
}

#[derive(Debug, Serialize)]
pub struct PendingReview {
    pub milestone_id: String,
    pub display: String,
    pub title: String,
    pub completed_at: String,
    pub spec_path: String,
}

#[derive(Debug, Serialize)]
pub struct SpecReviewItem {
    pub milestone_id: String,
    pub display: String,
    pub title: String,
}

pub(crate) fn load_reviews(ctx: &PlanContext) -> Result<ReviewsFile> {
    let path = ctx.plan_dir.join("reviews.json");
    if !path.exists() {
        return Ok(ReviewsFile::default());
    }
    let s = store::read_text_bounded(&path, store::MAX_PLAN_FILE_BYTES)
        .with_context(|| format!("read reviews {}", path.display()))?;
    Ok(serde_json::from_str(&s)?)
}

pub fn load_reviews_for_validate(ctx: &PlanContext) -> Result<Vec<ReviewRecord>> {
    Ok(load_reviews(ctx)?.reviews)
}

fn save_reviews(ctx: &PlanContext, file: &ReviewsFile) -> Result<()> {
    let path = ctx.plan_dir.join("reviews.json");
    let json = serde_json::to_string_pretty(file)?;
    store::atomic_write(path, format!("{json}\n"))?;
    Ok(())
}

fn milestone_completed_at(m: &crate::model::MilestoneFile) -> String {
    if !m.verification.date.is_empty() {
        m.verification.date.clone()
    } else {
        m.milestone.updated.clone()
    }
}

fn latest_review_for<'a>(file: &'a ReviewsFile, milestone_id: &str) -> Option<&'a ReviewRecord> {
    file.reviews
        .iter()
        .filter(|r| r.milestone_id == milestone_id)
        .max_by(|a, b| a.reviewed_at.cmp(&b.reviewed_at))
}

/// Latest review record for a milestone (highest `reviewed_at`), if any.
/// Used by the spec-review surface (M80) to anchor the since-last-approval diff.
pub fn latest_review(ctx: &PlanContext, milestone_id: &str) -> Result<Option<ReviewRecord>> {
    let file = load_reviews(ctx)?;
    Ok(latest_review_for(&file, milestone_id).cloned())
}

pub fn is_pending_review(m: &crate::model::MilestoneFile, file: &ReviewsFile) -> bool {
    // M100 ER-8: route through `effective_execution_status` so migrated
    // milestones whose raw field is empty correctly register as done.
    if effective_execution_status(m) != "done" {
        return false;
    }
    let completed = milestone_completed_at(m);
    match latest_review_for(file, &m.milestone.id) {
        None => true,
        Some(r) => completed > r.milestone_completed_at,
    }
}

pub fn pending_reviews(ctx: &PlanContext) -> Result<Vec<PendingReview>> {
    let milestones = store::load_all_milestones(ctx)?;
    pending_reviews_from(ctx, &milestones)
}

/// Like [`pending_reviews`] but reuses a pre-loaded milestone snapshot.
pub fn pending_reviews_from(
    ctx: &PlanContext,
    milestones: &[(std::path::PathBuf, crate::model::MilestoneFile)],
) -> Result<Vec<PendingReview>> {
    let reviews = load_reviews(ctx)?;
    let mut pending = Vec::new();
    for (_, m) in milestones {
        if !is_pending_review(m, &reviews) {
            continue;
        }
        let id = m.milestone.id.clone();
        pending.push(PendingReview {
            display: paths::display_milestone_id(&id),
            milestone_id: id.clone(),
            title: m.milestone.title.clone(),
            completed_at: milestone_completed_at(m),
            spec_path: format!("milestones/{}-{}.json", id, m.milestone.slug),
        });
    }
    pending.sort_by(|a, b| paths::compare_milestone_ids(&a.milestone_id, &b.milestone_id));
    Ok(pending)
}

pub fn pending_review_count(ctx: &PlanContext) -> Result<usize> {
    Ok(pending_reviews(ctx)?.len())
}

pub fn review_status(ctx: &PlanContext) -> Result<serde_json::Value> {
    let reviews = load_reviews(ctx)?;
    let milestones = store::load_all_milestones(ctx)?;

    let mut execution_pending = Vec::new();
    let mut spec_review = Vec::new();

    for (_, m) in milestones {
        // M100 ER-8: route through `effective_spec_status` so migrated
        // milestones whose raw field is empty are caught.
        if effective_spec_status(&m) == "review" {
            let id = m.milestone.id.clone();
            spec_review.push(SpecReviewItem {
                display: paths::display_milestone_id(&id),
                milestone_id: id,
                title: m.milestone.title.clone(),
            });
        }
        if is_pending_review(&m, &reviews) {
            let id = m.milestone.id.clone();
            execution_pending.push(PendingReview {
                display: paths::display_milestone_id(&id),
                milestone_id: id.clone(),
                title: m.milestone.title.clone(),
                completed_at: milestone_completed_at(&m),
                spec_path: format!("milestones/{}-{}.json", id, m.milestone.slug),
            });
        }
    }

    execution_pending
        .sort_by(|a, b| paths::compare_milestone_ids(&a.milestone_id, &b.milestone_id));
    spec_review.sort_by(|a, b| paths::compare_milestone_ids(&a.milestone_id, &b.milestone_id));

    let pending_review_count = execution_pending.len();
    let spec_review_count = spec_review.len();

    let suggested_next = if let Some(first) = spec_review.first() {
        json!({
            "type": "spec-review",
            "milestone_id": first.milestone_id,
            "display": first.display,
            "action": format!("mp milestone approve {}", first.milestone_id),
        })
    } else if let Some(first) = execution_pending.first() {
        json!({
            "type": "execution-review",
            "milestone_id": first.milestone_id,
            "display": first.display,
            "action": format!("mp reviews pass {} --verdict ok --reviewer <who>", first.milestone_id),
        })
    } else {
        json!(null)
    };

    Ok(json!({
        "pending_review_count": pending_review_count,
        "spec_review_count": spec_review_count,
        "execution_review": {
            "count": pending_review_count,
            "pending": execution_pending,
        },
        "spec_review": {
            "count": spec_review_count,
            "milestones": spec_review,
        },
        "suggested_next": suggested_next,
    }))
}

pub fn pending_reviews_with_summary(
    ctx: &PlanContext,
    pending: &[PendingReview],
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::with_capacity(pending.len());
    for pr in pending {
        let m = crate::milestone::load_milestone_by_id(ctx, &pr.milestone_id)?;
        let steps_total = m.steps.len();
        let steps_done = m.steps.iter().filter(|s| s.status == "done").count();
        let findings_open = m.findings.iter().filter(|f| f.status == "open").count();
        rows.push(json!({
            "milestone_id": pr.milestone_id,
            "display": pr.display,
            "title": pr.title,
            "completed_at": pr.completed_at,
            "spec_path": pr.spec_path,
            "summary": {
                "steps_done": steps_done,
                "steps_total": steps_total,
                "findings_open": findings_open,
            },
        }));
    }
    Ok(rows)
}

pub fn list_reviews(ctx: &PlanContext) -> Result<Vec<ReviewRecord>> {
    let file = load_reviews(ctx)?;
    let mut rows = file.reviews.clone();
    rows.sort_by(|a, b| b.reviewed_at.cmp(&a.reviewed_at));
    Ok(rows)
}

pub fn show_reviews(ctx: &PlanContext, milestone_id: &str) -> Result<Vec<ReviewRecord>> {
    let id = paths::normalize_milestone_id(milestone_id);
    let file = load_reviews(ctx)?;
    let mut rows: Vec<_> = file
        .reviews
        .iter()
        .filter(|r| r.milestone_id == id)
        .cloned()
        .collect();
    rows.sort_by(|a, b| b.reviewed_at.cmp(&a.reviewed_at));
    Ok(rows)
}

/// M133: unified review trail for a milestone — review verdicts +
/// threaded comments + handoff records. Used by `mp show milestone`
/// (and `mp reviews show`) to surface the durable review conversation
/// alongside the existing milestone fields. Returns three Vecs, each
/// sorted in the natural reading order: verdicts newest-first
/// (matches the existing `show_reviews` order); comments and handoffs
/// oldest-first (chronological thread order — easier to scan as a
/// conversation).
pub fn review_trail(
    ctx: &PlanContext,
    milestone_id: &str,
) -> Result<(Vec<ReviewRecord>, Vec<ReviewComment>, Vec<ReviewHandoff>)> {
    let verdicts = show_reviews(ctx, milestone_id)?;
    let comments = list_comments(ctx, milestone_id)?;
    let handoffs = list_handoffs(ctx, milestone_id)?;
    Ok((verdicts, comments, handoffs))
}

pub fn record_review_pass(
    ctx: &PlanContext,
    milestone_id: &str,
    verdict: &str,
    reviewer: &str,
    notes: Option<&str>,
) -> Result<ReviewRecord> {
    let verdict = verdict.trim();
    if verdict != "ok" && verdict != "changes-needed" {
        bail!("verdict must be ok or changes-needed");
    }
    if reviewer.trim().is_empty() {
        bail!("--reviewer is required");
    }

    let id = paths::normalize_milestone_id(milestone_id);
    let path = crate::milestone::load_milestone_path(ctx, &id)?;
    let mut m = store::load_milestone(&path)?;
    // M100 ER-8: route through `effective_execution_status` so migrated
    // milestones whose raw field is empty can be reviewed.
    let exec = effective_execution_status(&m);
    if exec != "done" {
        bail!("milestone {id} is not done (execution_status={})", exec);
    }

    // M145 S1 (AC-01): on verdict=ok, auto-promote the lifecycle
    // `done` → `complete` when the legacy-shape triple is present
    // (exec=done, spec=verified). Preserve `lifecycle_at` if already
    // set (so we don't blow away a timestamp written by an earlier
    // migration); else stamp now_rfc3339(). verdict=changes-needed
    // does NOT promote (AC-02). Idempotent: re-running on
    // lifecycle=complete is a no-op.
    //
    // M150 S2: when the auto-promote fires (`done → complete`), emit
    // the stage-done sentinel so `mp watch`'s fast-path picks up the
    // transition in sub-second latency. The sentinel lives on the
    // herdr pane where `mp reviews pass` ran (HERDR_PANE_ID env var);
    // when the env var is unset (e.g. agent ran `mp` from a plain
    // shell), the helper is a no-op and the lifecycle poll in `mp
    // watch` is the fallback (M149 behavior).
    //
    // M202 S4.1: every successful `reviews pass --verdict ok` also
    // closes the external-review stage (the reviewer just passed the
    // milestone). When remediate was already done before this pass —
    // i.e. the milestone is on the second pass through external-review
    // after remediation — the re-review stage closes too. The hook
    // lives here rather than as a `MilestoneEvent` because "reviews
    // pass" is the reviews-registry's verb, not a lifecycle state
    // machine event. Hand-off stays pending either way (AC-11).
    if verdict == "ok" {
        let lc = m.milestone.lifecycle.as_str();
        let spec = effective_spec_status(&m);
        // M145 F-01 (external review): the prior `lc != "complete" && lc == "done"`
        // was redundant — `lc == "done"` already excludes "complete". The narrower
        // scope stays in sync with W-LC-TERMINAL (validate/plan.rs), which only
        // fires for `lifecycle == "done"` so the warning's auto-promote advice is
        // always actionable. See M145 F-02.
        //
        // M196: the executor's end-state was renamed from `"done"` to
        // `"executed"` on the lifecycle side. The auto-promote
        // condition accepts both `"executed"` (canonical) and `"done"`
        // (legacy alias during the migration window) so a
        // half-migrated milestone with `lifecycle: "done"` still
        // auto-promotes on a passing review.
        //
        // The spec-side check is loosened: `verified` is the
        // pre-M196 contract (exec=done + spec=verified → promote),
        // `implemented` is the post-M196 contract (executed →
        // implemented via the lifecycle-to-spec projection; the
        // review itself is the verification). Both fire — the
        // reviewer's verdict is the authoritative signal, not the
        // pre-review spec status.
        let spec_ok = spec == "verified" || spec == "implemented" || spec == "ready";
        if (lc == "executed" || lc == "done") && spec_ok && lc != "complete" {
            let prior_timestamp = m.milestone.lifecycle_at.clone();
            crate::milestone::apply_transition(&mut m, crate::model::MilestoneEvent::Complete)?;
            if prior_timestamp.is_some() {
                m.milestone.lifecycle_at = prior_timestamp;
            }
            m.milestone.updated = store::today();
            store::write_milestone(&path, &m)?;
            crate::watch::emit_stage_done_best_effort("reviews-pass", Some(&id));
        }
        // M202 S4.1: close the external-review stage on every
        // successful `verdict=ok` (whether or not the auto-promote
        // above fired). The Complete transition would otherwise leave
        // external-review at `in_progress` (the milestone is sitting
        // in the review queue); the reviewer's passing verdict is the
        // signal that closes the stage. We persist this mutation
        // explicitly because the auto-promote path above writes to
        // disk BEFORE this hook fires — without an unconditional write
        // an already-complete milestone would silently lose the
        // external-review close.
        m.milestone.flow_stages.insert(
            "external-review".to_string(),
            crate::model::FlowStage {
                status: "done".to_string(),
                at: Some(crate::store::now_rfc3339()),
            },
        );
        // Re-review closes only when remediate was already done
        // before this pass landed. A first-time review of a healthy
        // milestone never touches re-review (it stayed `pending`
        // throughout).
        if m.milestone
            .flow_stages
            .get("remediate")
            .map(|s| s.status == "done")
            .unwrap_or(false)
        {
            m.milestone.flow_stages.insert(
                "re-review".to_string(),
                crate::model::FlowStage {
                    status: "done".to_string(),
                    at: Some(crate::store::now_rfc3339()),
                },
            );
        }
        // M202: persist the flow_stages update so the external-review
        // close actually lands on disk. The auto-promote branch above
        // also wrote a (pre-hook) copy; the second write is idempotent
        // for flow_stages (the BTreeMap gets the same keys with the
        // same status + a fresh at timestamp).
        m.milestone.updated = store::today();
        store::write_milestone(&path, &m)?;
    }

    let record = ReviewRecord {
        milestone_id: id,
        verdict: verdict.to_string(),
        reviewer: reviewer.to_string(),
        reviewed_at: store::today(),
        notes: notes.unwrap_or("").to_string(),
        milestone_completed_at: milestone_completed_at(&m),
    };

    let mut file = load_reviews(ctx)?;
    file.reviews.push(record.clone());
    save_reviews(ctx, &file)?;
    Ok(record)
}

// ── M133 AC-01: review comments ──────────────────────────────────────────────

/// Persist a review comment on a milestone. Validates draft invariants
/// (author + body required, optional finding link shape, RFC3339
/// timestamp) and writes atomically via the existing reviews.json
/// storage path (`store::atomic_write`).
///
/// M154 AC-02: optional `anchor` parameter — when provided, the comment
/// carries the same file/line/side shape hunk consumes. Absent anchor
/// preserves the pre-M154 milestone-anchored behavior (no migration;
/// backward compatible).
pub fn add_comment(
    ctx: &PlanContext,
    milestone_id: &str,
    author: &str,
    body: &str,
    finding_id: Option<&str>,
    created_at: Option<&str>,
    anchor: Option<&FindingAnchor>,
) -> Result<ReviewComment> {
    let finding_id = finding_id.unwrap_or("");
    let created_at = created_at.unwrap_or("");
    ReviewComment::validate_draft(author, body, finding_id, created_at)
        .map_err(|msg| anyhow::anyhow!("invalid review comment: {msg}"))?;

    let id = paths::normalize_milestone_id(milestone_id);
    // Validate that the milestone exists on disk. `load_milestone_path`
    // returns `Err("milestone <id> not found")` when the file is
    // missing, which is the same signal the explicit `path.exists()`
    // check below would have given — but with a cleaner error
    // message and no TOCTOU window. Comments attach to in-flight
    // (not-yet-done) milestones, not just done ones, because threaded
    // review conversation starts early in the lifecycle.
    let path = crate::milestone::load_milestone_path(ctx, &id)?;

    // M133 review remediation: when a finding link is supplied, verify
    // it references a finding that actually exists on this milestone.
    // `validate_draft` only checks the F-NN shape; without this
    // referential check a comment could link to `F-99` on a milestone
    // with no findings, creating a durable dangling reference.
    if !finding_id.is_empty() {
        let m = store::load_milestone(&path)?;
        let exists = m.findings.iter().any(|f| f.id == finding_id);
        if !exists {
            anyhow::bail!(
                "finding {finding_id} not found on milestone {id}; cannot link comment to a non-existent finding"
            );
        }
    }

    let created_at_final = if created_at.is_empty() {
        store::now_rfc3339()
    } else {
        created_at.to_string()
    };

    let mut file = load_reviews(ctx)?;
    let comment_id = next_comment_id(&file.comments);
    let comment = ReviewComment {
        id: comment_id,
        milestone_id: id,
        author: author.trim().to_string(),
        body: body.trim().to_string(),
        finding_id: finding_id.to_string(),
        created_at: created_at_final,
        anchor: anchor.cloned(),
    };

    file.comments.push(comment.clone());
    save_reviews(ctx, &file)?;
    Ok(comment)
}

/// List all review comments for a milestone, oldest-first (chronological
/// order — the same order they were authored in, which is also the
/// natural thread-reading order for a human reviewer).
pub fn list_comments(ctx: &PlanContext, milestone_id: &str) -> Result<Vec<ReviewComment>> {
    let id = paths::normalize_milestone_id(milestone_id);
    let file = load_reviews(ctx)?;
    let mut rows: Vec<_> = file
        .comments
        .into_iter()
        .filter(|c| paths::normalize_milestone_id(&c.milestone_id) == id)
        .collect();
    rows.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(rows)
}

// ── M133 AC-02: review handoffs ──────────────────────────────────────────────

/// Persist a coordinator/runner hand-off record on a milestone. The
/// persisted shape mirrors the hand-off protocol documented in
/// `mp-flow`'s Hand-off protocol section (`from_session` /
/// `to_session` / `data` / `session_boundary` / `evidence`) so the
/// CLI surface and the skill documentation stay in lockstep. Validates
/// that at least one of `from_session` / `to_session` is set and that
/// `data` is non-empty.
///
/// M142 AC-07: env-var auto-injection — if `MP_SESSION_ID` and
/// `MP_SESSION_ROLE` are set in the environment, they pre-populate
/// `from_session` / `from_role` (and `to_role` as the complement of
/// `from_role`) for the recording side. `to_session` is NOT filled
/// from `MP_SESSION_ID` — that would mint a same-session cross-role
/// hand-off (the L5 violation the audit exists to catch). Pass
/// `--to-session` (or a separate receiving-side identity) explicitly.
/// Manual `--from-session` / `--from-role` flags override the env
/// values (operator escape hatch). The env contract is
/// forgeable-by-humans and auditable-by-review (see
/// `mp reviews handoff --help`).
#[allow(clippy::too_many_arguments)]
pub fn record_handoff(
    ctx: &PlanContext,
    milestone_id: &str,
    from_session: &str,
    to_session: &str,
    from_role: &str,
    to_role: &str,
    data: &str,
    session_boundary: &str,
    evidence: &str,
    created_at: Option<&str>,
) -> Result<ReviewHandoff> {
    let created_at = created_at.unwrap_or("");

    // M142 AC-07: env-var pre-population runs BEFORE validate_draft so
    // a harness that sets MP_SESSION_ID + MP_SESSION_ROLE satisfies the
    // "at least one of --from-session / --to-session is required"
    // contract without the agent having to type anything. Manual
    // --from-X / --to-X flags take precedence over the env contract.
    //
    // M142 L1 (review): env values are sanitized — trimmed and
    // rejected if they contain control characters (which would
    // break downstream JSON parsers / display). The harness is
    // trusted to set these; defense in depth at the boundary where
    // the env var lands in durable storage.
    //
    // External-review F-02: MP_SESSION_ID populates ONLY from_session.
    // Auto-filling to_session with the same id systematically creates
    // `same_session_across_role_boundary`.
    fn sanitize(raw: Option<String>) -> Option<String> {
        let s = raw?.trim().to_string();
        if s.is_empty() {
            return None;
        }
        if s.chars().any(|c| c.is_control()) {
            return None;
        }
        Some(s)
    }
    let env_session = sanitize(std::env::var("MP_SESSION_ID").ok());
    let env_role = sanitize(std::env::var("MP_SESSION_ROLE").ok());

    let manual_session_is_blank = |manual: &str| manual.trim().is_empty();
    let final_from_session = if !manual_session_is_blank(from_session) {
        from_session.trim().to_string()
    } else {
        env_session.unwrap_or_default()
    };
    let final_to_session = if !manual_session_is_blank(to_session) {
        to_session.trim().to_string()
    } else {
        // Receiving session must be explicit — never mirror MP_SESSION_ID.
        String::new()
    };
    let final_from_role = if !from_role.trim().is_empty() {
        from_role.trim().to_string()
    } else {
        env_role.clone().unwrap_or_default()
    };
    let final_to_role = if !to_role.trim().is_empty() {
        to_role.trim().to_string()
    } else {
        match final_from_role.as_str() {
            "coordinator" => "runner".to_string(),
            "runner" => "coordinator".to_string(),
            _ => String::new(),
        }
    };

    ReviewHandoff::validate_draft(&final_from_session, &final_to_session, data, created_at)
        .map_err(|msg| anyhow::anyhow!("invalid review handoff: {msg}"))?;

    let id = paths::normalize_milestone_id(milestone_id);
    // Same milestone-existence contract as `add_comment` above —
    // `load_milestone_path` returns the error directly.
    let _path = crate::milestone::load_milestone_path(ctx, &id)?;

    let created_at_final = if created_at.is_empty() {
        store::now_rfc3339()
    } else {
        created_at.to_string()
    };

    let mut file = load_reviews(ctx)?;
    let handoff_id = next_handoff_id(&file.handoffs);
    let handoff = ReviewHandoff {
        id: handoff_id,
        milestone_id: id,
        from_session: final_from_session,
        to_session: final_to_session,
        from_role: final_from_role,
        to_role: final_to_role,
        data: data.trim().to_string(),
        session_boundary: session_boundary.trim().to_string(),
        evidence: evidence.trim().to_string(),
        created_at: created_at_final,
    };

    file.handoffs.push(handoff.clone());
    save_reviews(ctx, &file)?;
    Ok(handoff)
}

/// List all hand-off records for a milestone, oldest-first.
pub fn list_handoffs(ctx: &PlanContext, milestone_id: &str) -> Result<Vec<ReviewHandoff>> {
    let id = paths::normalize_milestone_id(milestone_id);
    let file = load_reviews(ctx)?;
    let mut rows: Vec<_> = file
        .handoffs
        .into_iter()
        .filter(|h| paths::normalize_milestone_id(&h.milestone_id) == id)
        .collect();
    rows.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(rows)
}

pub fn group_pending_reviews(
    pending: &[PendingReview],
    group_by: &str,
) -> Result<serde_json::Value> {
    let mut groups: HashMap<String, usize> = HashMap::new();
    for pr in pending {
        let key = match group_by {
            "completed_at" => {
                if pr.completed_at.len() >= 7 {
                    pr.completed_at[..7].to_string()
                } else {
                    pr.completed_at.clone()
                }
            }
            "milestone_id" => pr.milestone_id.clone(),
            _ => bail!("unknown group_by field: {group_by}"),
        };
        *groups.entry(key).or_insert(0) += 1;
    }
    let sorted: Vec<_> = {
        let mut entries: Vec<_> = groups.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    };
    Ok(json!({
        "groups": sorted.into_iter().map(|(k, v)| json!({"key": k, "count": v})).collect::<Vec<_>>(),
        "total": pending.len(),
    }))
}

pub fn load_done_milestones_map(
    ctx: &PlanContext,
) -> Result<HashMap<String, crate::model::MilestoneFile>> {
    let all = store::load_all_milestones(ctx)?;
    Ok(all
        .into_iter()
        // M100 ER-8: route through `effective_execution_status` so
        // migrated milestones whose raw field is empty are caught.
        .filter(|(_, m)| effective_execution_status(m) == "done")
        .map(|(_, m)| (paths::normalize_milestone_id(&m.milestone.id), m))
        .collect())
}

/// Presets accepted by `mp reviews pending --filter` and `mp reviews pass --all --filter`.
/// Keep in sync with the doc string on `ReviewsCmd::Pending.filter`.
pub const KNOWN_FILTER_PRESETS: &[&str] = &["force-bypassed"];

pub fn pending_matches_preset(
    lookup: &HashMap<String, crate::model::MilestoneFile>,
    pr: &PendingReview,
    preset: &str,
) -> Result<bool> {
    match preset {
        "force-bypassed" => Ok(lookup
            .get(&pr.milestone_id)
            .map(|m| {
                m.verification.evidence.contains("[force-bypassed")
                    || m.verification
                        .evidence
                        .contains("[step-tests force-bypassed")
            })
            .unwrap_or(false)),
        other => Err(anyhow::anyhow!(
            "unknown review filter preset: '{}' (known presets: {})",
            other,
            KNOWN_FILTER_PRESETS.join(", ")
        )),
    }
}

#[derive(Debug, Serialize)]
pub struct ReviewSweep {
    pub total: usize,
    pub buckets: Vec<SweepBucket>,
}

#[derive(Debug, Serialize)]
pub struct SweepBucket {
    pub kind: String,
    pub count: usize,
    pub milestone_ids: Vec<String>,
    pub reason: String,
}

pub fn sweep_pending_reviews(ctx: &PlanContext, pending: &[PendingReview]) -> Result<ReviewSweep> {
    let lookup = load_done_milestones_map(ctx)?;

    let mut force_bypassed = Vec::new();
    let mut manual_only = Vec::new();
    let mut runnable = Vec::new();

    for pr in pending {
        let m = match lookup.get(&pr.milestone_id) {
            Some(m) => m,
            None => {
                runnable.push(pr.milestone_id.clone());
                continue;
            }
        };

        let evidence = &m.verification.evidence;
        if evidence.contains("[force-bypassed") || evidence.contains("[step-tests force-bypassed") {
            force_bypassed.push(pr.milestone_id.clone());
        } else if all_acs_manual(m) {
            manual_only.push(pr.milestone_id.clone());
        } else {
            runnable.push(pr.milestone_id.clone());
        }
    }

    Ok(ReviewSweep {
        total: pending.len(),
        buckets: vec![
            SweepBucket {
                kind: "force-bypassed".to_string(),
                count: force_bypassed.len(),
                milestone_ids: force_bypassed,
                reason: "completed via --force; low-risk rubber-stamp review".to_string(),
            },
            SweepBucket {
                kind: "manual-only".to_string(),
                count: manual_only.len(),
                milestone_ids: manual_only,
                reason: "every AC verification starts with 'manual:' — no commands to run; needs human judgment".to_string(),
            },
            SweepBucket {
                kind: "runnable".to_string(),
                count: runnable.len(),
                milestone_ids: runnable,
                reason: "at least one AC verification is a runnable command; review the test output".to_string(),
            },
        ],
    })
}

fn all_acs_manual(m: &crate::model::MilestoneFile) -> bool {
    if m.acceptance_criteria.is_empty() {
        return false;
    }
    m.acceptance_criteria
        .iter()
        .all(|ac| ac.verification.trim().starts_with("manual:"))
}

// ── Findings ─────────────────────────────────────────────────────────────────

/// Numeric ordering for `MilestoneMeta.priority`. Higher rank = higher
/// priority. Used to decide whether a remediation entry should
/// escalate `priority` (only if the current rank is below `high`).
/// Returns 0 for unknown values so the caller can use a strict-less-
/// than comparison safely.
fn priority_rank(value: &str) -> u8 {
    match value {
        "urgent" => 4,
        "high" => 3,
        "normal" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn next_finding_id(m: &crate::model::MilestoneFile) -> String {
    let max = m
        .findings
        .iter()
        .filter_map(|f| {
            f.id.strip_prefix("F-")
                .and_then(|n| n.parse::<usize>().ok())
        })
        .max()
        .unwrap_or(0);
    format!("F-{:02}", max + 1)
}

pub fn add_finding(
    ctx: &PlanContext,
    milestone_id: &str,
    severity: &str,
    category: &str,
    description: &str,
    author: Option<&str>,
) -> Result<crate::model::Finding> {
    let draft = crate::model::FindingDraft {
        milestone_id: milestone_id.to_string(),
        severity: severity.to_string(),
        category: category.to_string(),
        description: description.to_string(),
        author: author.unwrap_or("").to_string(),
        phase: String::new(),
        summary: String::new(),
        rationale: String::new(),
        confidence: String::new(),
        tags: vec![],
        anchor: None,
        thread: vec![],
    };
    add_finding_with_phase(ctx, draft)
}

/// M101: add a finding with explicit phase tag (self | external). Empty phase
/// preserves the legacy behavior (no phase, no transition gating from the
/// finding side).
///
/// M105 S5 (B-45): adds an explicit `confidence` parameter (empty default)
/// and validates it the same way `severity` is validated — using the
/// model-layer `crate::model::is_valid_confidence` helper. The CLI surface for
/// setting confidence lands in M101 AC-08; this guard establishes the
/// invariant M101 will respect via tests.
///
/// M101 R4: refactored to take a `FindingDraft` struct instead of 8
/// positional parameters. The CLI handler in `crates/mp/src/commands/reviews.rs`
/// builds one of these from the new flags. The validation steps that
/// were inline here now live on `FindingDraft::validate()` so the CLI,
/// future programmatic callers, and the add_finding_with_phase path
/// all share one validator.
pub fn add_finding_with_phase(
    ctx: &PlanContext,
    draft: crate::model::FindingDraft,
) -> Result<crate::model::Finding> {
    draft
        .validate()
        .map_err(|msg| anyhow::anyhow!("invalid FindingDraft: {msg}"))?;

    let id = paths::normalize_milestone_id(&draft.milestone_id);
    let path = crate::milestone::load_milestone_path(ctx, &id)?;
    let mut m = store::load_milestone(&path)?;

    // M101 R3: validate any pre-existing thread entries on the
    // milestone for RFC3339 timestamps. The just-added finding has no
    // thread entries yet (the CLI doesn't expose thread editing), but
    // we validate here so a future thread-add write path can
    // delegate to the same check.
    for finding in &m.findings {
        for entry in &finding.thread {
            if let Err(msg) = entry.validate() {
                anyhow::bail!(
                    "milestone {} has invalid thread entry on finding {}: {}",
                    id,
                    finding.id,
                    msg
                );
            }
        }
    }

    let fid = next_finding_id(&m);
    let finding = crate::model::Finding {
        id: fid,
        severity: draft.severity.clone(),
        category: draft.category.clone(),
        description: draft.description.clone(),
        status: "open".to_string(),
        author: draft.author.clone(),
        fixed_in: String::new(),
        created: store::today(),
        resolved: String::new(),
        phase: draft.phase.clone(),
        anchor: draft.anchor.clone(),
        thread: draft.thread.clone(),
        summary: draft.summary.clone(),
        rationale: draft.rationale.clone(),
        confidence: draft.confidence.clone(),
        tags: draft.tags.clone(),
    };

    m.findings.push(finding.clone());

    // M101 R1 S17: auto-enter remediation when filing an open finding
    // at a checkpoint milestone. Self-phase (or empty-phase per M125)
    // findings on `executed`/`complete` milestones escalate to
    // remediation (the executor's end-state is now `executed`
    // post-M196; the legacy `done` alias is preserved for the
    // migration window so half-migrated milestones still
    // participate). External-phase findings on the same active
    // destinations do the same — after demoting self-reviewed/reviewed
    // to aliases, stage-8 external review lands on
    // `executed`/`complete`. Legacy on-disk self-reviewed/reviewed
    // strings still enter for external findings. Priority
    // auto-escalates to high (only if the current priority ranks
    // below high — urgent stays urgent); never auto-reverts on
    // remediation exit (M101 AC-13 invariant).
    let lc_was = m.milestone.lifecycle.clone();
    let entered_remediation = match m.milestone.lifecycle.as_str() {
        "done" | "executed" | "complete"
            if m.has_open_self_findings() || m.has_open_external_findings() =>
        {
            crate::milestone::apply_transition(
                &mut m,
                crate::model::MilestoneEvent::EnterRemediation,
            )?;
            if priority_rank(&m.milestone.priority) < priority_rank("high") {
                m.milestone.priority = "high".to_string();
            }
            true
        }
        "self-reviewed" | "reviewed" if m.has_open_external_findings() => {
            crate::milestone::apply_transition(
                &mut m,
                crate::model::MilestoneEvent::EnterRemediation,
            )?;
            if priority_rank(&m.milestone.priority) < priority_rank("high") {
                m.milestone.priority = "high".to_string();
            }
            true
        }
        _ => false,
    };

    m.milestone.updated = store::today();
    store::write_milestone(&path, &m)?;
    // M180 S3: emit one lifecycle-transition event when the
    // remediation entry actually changed the lifecycle. The
    // `add_finding` path on a non-checkpoint milestone writes the
    // finding but leaves lifecycle alone — no event.
    crate::activity::record_lifecycle_transition(
        ctx,
        &m.milestone.id,
        &lc_was,
        &m.milestone.lifecycle,
    )?;
    if entered_remediation {
        eprintln!(
            "milestone {id} entered remediation (priority=high). \
             resolve findings via `mp reviews finding resolve <id>` to exit."
        );
    }
    Ok(finding)
}

pub fn resolve_finding(
    ctx: &PlanContext,
    milestone_id: &str,
    finding_id: &str,
    commit: Option<&str>,
) -> Result<crate::model::Finding> {
    let id = paths::normalize_milestone_id(milestone_id);
    let path = crate::milestone::load_milestone_path(ctx, &id)?;
    let mut m = store::load_milestone(&path)?;

    let finding = m
        .findings
        .iter_mut()
        .find(|f| f.id == finding_id)
        .ok_or_else(|| anyhow::anyhow!("finding {finding_id} not found on milestone {id}"))?;

    finding.status = "fixed".to_string();
    finding.fixed_in = commit.unwrap_or("").to_string();
    finding.resolved = store::today();
    let result = finding.clone();
    let lc_was = m.milestone.lifecycle.clone();

    // M101 R1 S18: auto-exit remediation when the last open finding
    // closes. Priority STAYS at high (M101 AC-13 invariant — no
    // auto-revert). The next review pass is responsible for the
    // gate-cleared comment in verification.evidence.
    //
    // BF-14 (M131): pre_state is now the value captured AT remediation
    // entry (`remediation_pre_state`, set in `add_finding_with_phase`),
    // not reconstructed at exit by scanning the finding set. The first
    // M131 attempt scanned for any external-phase finding, which was
    // order-independent but misclassified a milestone carrying a
    // *resolved* external finding alongside a later self finding — it
    // would exit to "self-reviewed" even though the most recent entry
    // was from the done/complete (self) track. Persisting at entry is
    // correct by construction and subsumes both the original order bug
    // and that residual case.
    //
    // The `unwrap_or` fallback covers milestones already in remediation
    // when this field shipped (pre-field on-disk records have it
    // `None`); it preserves the prior best-effort derivation so those
    // in-flight remediations aren't wedged.
    let open_remaining = m.findings.iter().filter(|f| f.status == "open").count();
    if open_remaining == 0 && m.milestone.lifecycle == "remediation" {
        crate::milestone::apply_transition(&mut m, crate::model::MilestoneEvent::ExitRemediation)?;
    }

    m.milestone.updated = store::today();
    store::write_milestone(&path, &m)?;
    // M180 S3: emit one lifecycle-transition event when the
    // remediation exit actually changed the lifecycle. Resolving a
    // finding on a non-remediation milestone is a no-op here.
    crate::activity::record_lifecycle_transition(
        ctx,
        &m.milestone.id,
        &lc_was,
        &m.milestone.lifecycle,
    )?;
    Ok(result)
}

pub fn resolve_all_open_findings(
    ctx: &PlanContext,
    milestone_id: &str,
    commit: Option<&str>,
) -> Result<Vec<crate::model::Finding>> {
    let open = list_findings(ctx, milestone_id, true)?;
    let mut resolved = Vec::with_capacity(open.len());
    for f in open {
        resolved.push(resolve_finding(ctx, milestone_id, &f.id, commit)?);
    }
    Ok(resolved)
}

pub fn list_findings(
    ctx: &PlanContext,
    milestone_id: &str,
    open_only: bool,
) -> Result<Vec<crate::model::Finding>> {
    let id = paths::normalize_milestone_id(milestone_id);
    let path = crate::milestone::load_milestone_path(ctx, &id)?;
    let m = store::load_milestone(&path)?;

    Ok(m.findings
        .into_iter()
        .filter(|f| !open_only || f.status == "open")
        .collect())
}

// ── Review state ─────────────────────────────────────────────────────────────

pub fn milestone_review_state(
    ctx: &PlanContext,
    m: &crate::model::MilestoneFile,
) -> Result<&'static str> {
    let reviews = load_reviews(ctx)?;
    let norm = paths::normalize_milestone_id(&m.milestone.id);
    let has_review = reviews
        .reviews
        .iter()
        .any(|r| paths::normalize_milestone_id(&r.milestone_id) == norm);
    Ok(review_state(m, has_review))
}

pub fn review_state(m: &crate::model::MilestoneFile, has_review: bool) -> &'static str {
    // M100: route through effective_execution_status so the gate fires the
    // same before and after the bulk legacy → lifecycle migration. After
    // migration, execution_status is empty and the legacy-only comparison
    // would miss milestones that progressed to lifecycle=done|self-reviewed
    // |reviewed|complete.
    let exec = crate::validate::effective_execution_status(m);
    if exec != "done" && m.milestone.lifecycle != "remediation" {
        return "";
    }
    if !has_review {
        return "pending-review";
    }
    let open_count = m.findings.iter().filter(|f| f.status == "open").count();
    if open_count > 0 {
        return "open-findings";
    }
    if !m.findings.is_empty() {
        // "remediated" only when every finding was actually fixed; dismissed
        // (wontfix) findings leave the review concluded but not remediated.
        if m.findings.iter().all(|f| f.status == "fixed") {
            return "remediated";
        }
        return "reviewed-clean";
    }
    "reviewed-clean"
}

pub fn lifecycle_rollup(ctx: &PlanContext) -> Result<serde_json::Value> {
    let reviews = load_reviews(ctx)?;
    let milestones = store::load_all_milestones(ctx)?;
    let reviewed_ids: std::collections::HashSet<String> = reviews
        .reviews
        .iter()
        .map(|r| paths::normalize_milestone_id(&r.milestone_id))
        .collect();

    let mut groups: HashMap<&str, Vec<serde_json::Value>> = HashMap::new();

    for (_, m) in &milestones {
        let state = review_state(
            m,
            reviewed_ids.contains(&paths::normalize_milestone_id(&m.milestone.id)),
        );
        if state.is_empty() {
            continue;
        }
        groups.entry(state).or_default().push(json!({
            "id": m.milestone.id,
            "display": paths::display_milestone_id(&m.milestone.id),
            "title": m.milestone.title,
            "findings_open": m.findings.iter().filter(|f| f.status == "open").count(),
            "findings_total": m.findings.len(),
        }));
    }

    let mut rollup = Vec::new();
    for state in &[
        "pending-review",
        "open-findings",
        "remediated",
        "reviewed-clean",
    ] {
        if let Some(items) = groups.remove(state) {
            rollup.push(json!({
                "review_state": state,
                "count": items.len(),
                "milestones": items,
            }));
        }
    }

    Ok(
        json!({ "lifecycle": rollup, "total_done": milestones.iter().filter(|(_, m)| effective_execution_status(m) == "done").count() }),
    )
}

// ── M142 AC-01..AC-05: L5 evidence audit ────────────────────────────────────

/// L5 violation reasons. Three deterministic classes per the milestone
/// spec: `same_session_across_role_boundary`, `missing_session_identity`,
/// `role_inversion`. The L5 check is advisory — see AC-06 (`mp validate`
/// integration) for why blocking is out of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum L5ViolationReason {
    /// `from_session == to_session` at a cross-role hand-off.
    SameSessionAcrossRoleBoundary,
    /// Either `from_session` or `to_session` is empty. Empty roles are
    /// not counted here (they skip same-session and role-inversion
    /// checks instead).
    MissingSessionIdentity,
    /// `from_role` / `to_role` contradict the receiving stage's
    /// expected role (e.g. hand-off 8→9 is runner-receives, so
    /// `to_role` must be `runner`).
    RoleInversion,
}

#[derive(Debug, Clone, Serialize)]
pub struct L5Violation {
    /// ID of the offending handoff (H-NN).
    pub at_handoff: String,
    /// Producing side of the hand-off (handoff id or role label).
    pub from: String,
    /// Receiving side of the hand-off.
    pub to: String,
    /// One of: same_session_across_role_boundary | missing_session_identity | role_inversion.
    pub reason: L5ViolationReason,
    /// Always "advisory" — L5 is non-blocking by design.
    pub severity: &'static str,
    /// Human-readable description for `raul` rendering.
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct L5Audit {
    pub ok: bool,
    pub violations: Vec<L5Violation>,
    pub summary: L5AuditSummary,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct L5AuditSummary {
    pub total_handoffs: usize,
    pub cross_role_handoffs: usize,
    pub violation_count: usize,
}

/// The four hand-off points per `mp-flow`'s Hand-off protocol section.
/// Each hand-off point names the stage transition and the expected role
/// rotation: the receiving side of the hand-off is the owner of the
/// destination stage.
fn expected_receiving_role(from_stage: u32, to_stage: u32) -> Option<&'static str> {
    match (from_stage, to_stage) {
        // Hand-off (a): coordinator Approve (4) → runner Claim & execute (5)
        (4, 5) => Some("runner"),
        // Hand-off (b): runner Complete (7) → coordinator External review (8)
        (7, 8) => Some("coordinator"),
        // Hand-off (c): coordinator External review (8) → runner Remediate (9)
        (8, 9) => Some("runner"),
        // Hand-off (d): runner Remediate (9) → coordinator Re-review (10)
        (9, 10) => Some("coordinator"),
        _ => None,
    }
}

/// Run the L5 audit on a milestone's hand-off records. Three violation
/// classes: same-session-across-role-boundary, missing-session-identity,
/// role-inversion. The check is advisory: `ok: false` is a warning, not
/// a gate failure. The function returns `Result<L5Audit>` because
/// `list_handoffs` propagates IO/parse errors (e.g., corrupt
/// `reviews.json`); the CLI surfaces a non-zero exit code in that
/// case rather than silently producing an empty audit.
///
/// Hand-offs without a `from_role` / `to_role` (pre-M142 records) skip
/// the role checks but still surface missing-identity and
/// same-session-across-role-boundary violations if the session strings
/// are populated.
pub fn l5_check(ctx: &PlanContext, milestone_id: &str) -> Result<L5Audit> {
    let id = paths::normalize_milestone_id(milestone_id);
    // Note: we do NOT require the milestone to exist in plan.json
    // here. Session-scoped milestones live in
    // `sessions/<session>/milestone.json` and `load_milestone_path`
    // only looks under `milestones/`. The L5 audit operates on the
    // persisted hand-off records (reviews.json); if there are no
    // handoffs for the id, the audit reports 0 violations regardless
    // of milestone existence. The caller (CLI or validate) can
    // verify milestone existence separately if needed.
    let handoffs = list_handoffs(ctx, &id)?;
    let mut violations = Vec::new();
    let mut cross_role_count = 0usize;

    for h in &handoffs {
        let from_role = h.from_role.as_str();
        let to_role = h.to_role.as_str();
        let from_session = h.from_session.as_str();
        let to_session = h.to_session.as_str();

        // Violation class (b): missing session identity.
        if from_session.is_empty() || to_session.is_empty() {
            violations.push(L5Violation {
                at_handoff: h.id.clone(),
                from: from_role.to_string(),
                to: to_role.to_string(),
                reason: L5ViolationReason::MissingSessionIdentity,
                severity: "advisory",
                message: format!(
                    "handoff {} has missing session identity (from_session={from_session:?}, to_session={to_session:?})",
                    h.id
                ),
            });
        }
        // Count cross-role hand-offs (a hand-off is cross-role if
        // both role fields are populated and different).
        let roles_populated_and_different =
            !from_role.is_empty() && !to_role.is_empty() && from_role != to_role;
        if roles_populated_and_different {
            cross_role_count += 1;
        }

        // Violation class (a): the same session id appears on BOTH sides
        // of a hand-off. That alone is the strongest L5 signal — one agent
        // session authored and then received the work, defeating the
        // independent-review discipline. We fire whenever both session ids
        // are populated and equal. When role info is also present and
        // differs we name the cross-role boundary; when roles are empty
        // (pre-M142 records, or an operator who passed identical session
        // ids without role info) we still flag the same-session reuse so
        // it cannot pass silently — the missing-identity class (b) only
        // fires on *empty* sessions and would otherwise miss this case
        // (M142 code-review: the prior role-gated check let a same-session
        // hand-off with empty roles through undetected).
        if !from_session.is_empty() && from_session == to_session {
            let boundary = if roles_populated_and_different {
                format!("{from_role}→{to_role}")
            } else {
                "(roles unrecorded)".to_string()
            };
            violations.push(L5Violation {
                at_handoff: h.id.clone(),
                from: from_role.to_string(),
                to: to_role.to_string(),
                reason: L5ViolationReason::SameSessionAcrossRoleBoundary,
                severity: "advisory",
                message: format!(
                    "handoff {} has same session id {from_session:?} across a {boundary} boundary",
                    h.id
                ),
            });
        }
    }

    // Violation class (c): role inversion. Requires structured role
    // fields AND a way to identify the hand-off's stage transition.
    // The persisted shape doesn't carry stage numbers, so we use the
    // ordering convention: the i-th hand-off in the milestone's
    // hand-off list maps to hand-off point (i % 4) of (a)/(b)/(c)/(d),
    // with (a)→(4,5), (b)→(7,8), (c)→(8,9), (d)→(9,10). The
    // handoff-points cycle after the 4th handoff (idx 4 maps back to
    // (a)→(4,5), idx 5 to (b)→(7,8), etc.). This works for the
    // common 4-handoff milestone; for milestones that legitimately
    // record more than 4 handoffs (e.g., re-issuing a hand-off after
    // a re-review), the cycling means later handoffs are checked
    // against the same role expectations as the first 4. The role
    // rotation is the same in both directions (coordinator→runner
    // and back), so the check stays semantically valid.
    for (idx, h) in handoffs.iter().enumerate() {
        if h.from_role.is_empty() || h.to_role.is_empty() {
            continue;
        }
        // `idx % 4` is always in 0..=3 — the four hand-off points
        // cycle (a)/(b)/(c)/(d) every 4 records. Hand-offs without
        // an explicit `handoff_point` field fall back to the
        // ordering convention. See AC-05: milestones that record
        // more than 4 hand-offs use this cycling; a follow-up may
        // thread the hand-off point into the persisted record for
        // strict-mode detection.
        let (from_stage, to_stage) = match idx % 4 {
            0 => (4, 5),
            1 => (7, 8),
            2 => (8, 9),
            _ => (9, 10),
        };
        if let Some(expected) = expected_receiving_role(from_stage, to_stage) {
            if h.to_role != expected {
                violations.push(L5Violation {
                    at_handoff: h.id.clone(),
                    from: h.from_role.clone(),
                    to: h.to_role.clone(),
                    reason: L5ViolationReason::RoleInversion,
                    severity: "advisory",
                    message: format!(
                        "handoff {} has to_role={:?} but stage transition {from_stage}→{to_stage} expects {expected:?}",
                        h.id, h.to_role
                    ),
                });
            }
        }
    }

    Ok(L5Audit {
        ok: violations.is_empty(),
        summary: L5AuditSummary {
            total_handoffs: handoffs.len(),
            cross_role_handoffs: cross_role_count,
            violation_count: violations.len(),
        },
        violations,
    })
}

// ── M105 S5 (B-45) — confidence validation tests ────────────────────────────

#[cfg(test)]
mod tests {
    use crate::model::is_valid_confidence;

    /// Pinned by AC-05: `cargo test -p mp --lib reviews::tests::confidence_validation`.
    /// Covers the helper directly so the test doesn't need a `PlanContext`
    /// (which the integration tests do exercise via the CLI for end-to-end).
    #[test]
    fn confidence_validation() {
        // Valid values: low / medium / high / empty (the four documented states).
        assert!(is_valid_confidence("low"));
        assert!(is_valid_confidence("medium"));
        assert!(is_valid_confidence("high"));
        assert!(is_valid_confidence(""));

        // Invalid values: anything else fails. The helper is case-sensitive
        // — "Low" must NOT be accepted (mirrors the severity guard's
        // case-sensitivity at `crates/mp/src/reviews.rs:446-447`).
        assert!(!is_valid_confidence("Low"));
        assert!(!is_valid_confidence("MEDIUM"));
        assert!(!is_valid_confidence(" high"), "leading whitespace rejects");
        assert!(!is_valid_confidence("high "), "trailing whitespace rejects");
        assert!(!is_valid_confidence(" high "), "padded whitespace rejects");
        assert!(!is_valid_confidence("extreme"));
        assert!(!is_valid_confidence("none"));
        assert!(!is_valid_confidence("urgent"));
    }
}
