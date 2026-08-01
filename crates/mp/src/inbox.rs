use serde::Serialize;

use crate::groom;
use crate::paths::{self, PlanContext};
use crate::reviews;
use crate::store;
use crate::track_kind;
use crate::validate;

#[derive(Debug, Serialize)]
pub struct InboxReport {
    pub count: usize,
    /// Discriminator that names what `count` is counting.
    ///   - `"actionable"` — items that need PM or agent decision. Excludes
    ///     ideas/backlog already in `dismissed`/`archived` (M171/TW-16).
    ///     The `spec-review` / `execution-review` / `review` filters are
    ///     slices of the actionable queue filtered by `kind`.
    ///   - `"all"` — same items as actionable today, but tagged so the
    ///     caller can distinguish a request for the unactionable pile
    ///     from a request for the active queue.
    pub count_kind: &'static str,
    pub items: Vec<InboxItem>,
    pub validate_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InboxItem {
    pub kind: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    pub action: String,
}

pub fn build_inbox(ctx: &PlanContext, filter: &str) -> anyhow::Result<InboxReport> {
    // TW-15 (M171 S1): share one live-milestone snapshot across the
    // downstream validate + pending_reviews consumers. Prior shape:
    //   `validate_plan` called `store::load_all_milestones(ctx)`
    //   internally AND the top-level call did it again → 2 directory
    //   scans + 2N parses per inbox render. Now we load once and drive
    //   both downstream consumers (validate, pending_reviews) off the
    //   same snapshot via the `*_with_milestones` / `*_from` pre-loaded
    //   variants.
    //
    // Caveat (external-review F-05): `validate_plan_with_milestones`
    // still does its own follow-up scans of `archive/milestones/` for
    // W43 cross-ref resolution against archived milestone IDs. Those
    // scans are independent of the live-milestone snapshot and remain
    // per-call. The TW-15 win is "1 live-load per inbox render"; the
    // archive-loads are a separate axis and would need a sibling
    // snapshot (with archived milestones) to remove.
    let milestones = store::load_all_milestones(ctx)?;
    let validate_report = validate::validate_plan_with_milestones(ctx, &milestones)?;
    let pending = reviews::pending_reviews_from(ctx, &milestones)?;
    build_inbox_from(
        ctx,
        filter,
        &milestones,
        validate_report.ok,
        Some(validate_report.errors.len()),
        &pending,
    )
}

/// Like [`build_inbox`] but reuses pre-loaded milestones / pending reviews.
/// `validate_error_count` is `Some(n)` when the caller has a full validate
/// report; `None` skips the validate inbox row (status only needs the count).
pub fn build_inbox_from(
    ctx: &PlanContext,
    filter: &str,
    milestones: &[(std::path::PathBuf, crate::model::MilestoneFile)],
    validate_ok: bool,
    validate_error_count: Option<usize>,
    pending: &[reviews::PendingReview],
) -> anyhow::Result<InboxReport> {
    let mut items = Vec::new();

    for pr in pending {
        items.push(InboxItem {
            kind: "execution-review".to_string(),
            id: pr.milestone_id.clone(),
            display: Some(format!("{} — {}", pr.display, pr.title)),
            reason: "execution done — awaiting independent review".to_string(),
            priority: None,
            action: format!(
                "mp reviews pass {} --verdict ok --reviewer <who>",
                pr.milestone_id
            ),
        });
    }

    for (_, m) in milestones {
        let mid = paths::normalize_milestone_id(&m.milestone.id);
        let display = format!(
            "{} — {}",
            paths::display_milestone_id(&m.milestone.id),
            m.milestone.title
        );

        if m.milestone.execution_status == "blocked" {
            items.push(InboxItem {
                kind: "milestone".to_string(),
                id: mid.clone(),
                display: Some(display.clone()),
                reason: format!("blocked: {}", m.milestone.block_reason),
                priority: Some(m.milestone.priority.clone()),
                action: format!("mp show milestone {mid}"),
            });
            continue;
        }
        if m.milestone.spec_status == "review" {
            items.push(InboxItem {
                kind: "spec-review".to_string(),
                id: mid.clone(),
                display: Some(display.clone()),
                reason: "spec_status review — awaiting approval".to_string(),
                priority: Some(m.milestone.priority.clone()),
                action: format!("mp milestone approve {mid}"),
            });
        } else if groom::milestone_matches_filter(m, "grooming", ctx).unwrap_or(false) {
            items.push(InboxItem {
                kind: "milestone".to_string(),
                id: mid.clone(),
                display: Some(display),
                reason: format!("needs grooming (spec: {})", m.milestone.spec_status),
                priority: Some(m.milestone.priority.clone()),
                action: format!("mp milestone groom {mid}"),
            });
        } else if groom::milestone_matches_filter(m, "partial", ctx).unwrap_or(false) {
            items.push(InboxItem {
                kind: "milestone".to_string(),
                id: mid.clone(),
                display: Some(display),
                reason: "partial execution progress".to_string(),
                priority: Some(m.milestone.priority.clone()),
                action: format!("mp show milestone {mid}"),
            });
        }
    }

    for &tk in &track_kind::TrackKind::ALL {
        let kind = tk.as_str();
        if let Ok(track) = store::load_track(ctx, kind) {
            for item in track
                .items
                .iter()
                .filter(|i| i.status == "pending" || i.status == "in-progress")
            {
                items.push(InboxItem {
                    kind: "track".to_string(),
                    id: item.id.clone(),
                    display: Some(item.title.clone()),
                    reason: format!("{} {}", item.status, kind),
                    priority: None,
                    action: format!("mp track show {kind}"),
                });
            }
        }
    }

    if let Ok(ideas) = store::load_ideas(ctx) {
        for idea in ideas
            .ideas
            .iter()
            .filter(|i| i.status == "open" && !i.id.is_empty())
        {
            items.push(InboxItem {
                kind: "idea".to_string(),
                id: idea.id.clone(),
                display: Some(idea.title.clone()),
                reason: "open idea".to_string(),
                priority: None,
                action: format!("mp idea show {}", idea.id),
            });
        }
    }

    if let Ok(backlog) = store::load_backlog(ctx) {
        for item in backlog
            .items
            .iter()
            .filter(|i| i.status == "active" && !i.id.is_empty())
        {
            items.push(InboxItem {
                kind: "backlog".to_string(),
                id: item.id.clone(),
                display: Some(item.description.clone()),
                reason: "active backlog item".to_string(),
                priority: Some(item.priority.clone()),
                action: format!("mp backlog show {}", item.id),
            });
        }
    }

    if let Ok(annotations) = store::load_annotations(ctx) {
        for a in annotations
            .annotations
            .iter()
            .filter(|a| a.status == "open")
        {
            items.push(InboxItem {
                kind: "annotation".to_string(),
                id: a.id.clone(),
                display: Some(format!("{} [{}]", a.kind, a.target)),
                reason: format!("open annotation: {}", a.body),
                priority: None,
                action: format!("mp annotation show {}", a.id),
            });
        }
    }

    if !validate_ok {
        if let Some(n) = validate_error_count {
            items.push(InboxItem {
                kind: "validate".to_string(),
                id: "plan".to_string(),
                display: None,
                reason: format!("{n} validation error(s)"),
                priority: None,
                action: "mp validate --summary".to_string(),
            });
        }
    }

    let filtered = apply_inbox_filter(items, filter)?;
    let count = filtered.len();
    Ok(InboxReport {
        count,
        count_kind: if filter == "all" { "all" } else { "actionable" },
        items: filtered,
        validate_ok,
        filter: if filter == "actionable" {
            None
        } else {
            Some(filter.to_string())
        },
    })
}

fn apply_inbox_filter(items: Vec<InboxItem>, filter: &str) -> anyhow::Result<Vec<InboxItem>> {
    Ok(match filter {
        "all" | "actionable" => items,
        "spec-review" => items
            .into_iter()
            .filter(|i| i.kind == "spec-review")
            .collect(),
        "execution-review" => items
            .into_iter()
            .filter(|i| i.kind == "execution-review")
            .collect(),
        "review" => items
            .into_iter()
            .filter(|i| i.kind == "spec-review" || i.kind == "execution-review")
            .collect(),
        other => anyhow::bail!(
            "unknown inbox filter '{other}' (expected actionable, all, spec-review, execution-review, or review)"
        ),
    })
}

pub fn status_blockers(ctx: &PlanContext) -> anyhow::Result<Vec<serde_json::Value>> {
    let milestones = store::load_all_milestones(ctx)?;
    Ok(status_blockers_from(&milestones))
}

pub fn status_blockers_from(
    milestones: &[(std::path::PathBuf, crate::model::MilestoneFile)],
) -> Vec<serde_json::Value> {
    let mut blockers = Vec::new();
    for (_, m) in milestones {
        if m.milestone.execution_status == "blocked" {
            blockers.push(serde_json::json!({
                "milestone": paths::normalize_milestone_id(&m.milestone.id),
                "reason": m.milestone.block_reason,
                "since": m.milestone.blocked_at,
            }));
        }
    }
    blockers
}
