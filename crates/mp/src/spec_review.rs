//! Spec-review surface (M80) — condensed, review-oriented reads of a milestone
//! spec for humans (via raul) and agents (via `mp spec`).
//!
//! `spec_review` projects a lean review view: outcome, problem, scope, ACs with
//! per-AC coverage + evidence + force-bypass flags, open questions, and an
//! AC-to-step traceability matrix with coverage gaps. `spec_diff` reports what
//! spec fields changed since the milestone's last approval (review record),
//! anchored on git history of the milestone file.

use anyhow::Result;
use serde_json::{json, Value};

use crate::milestone;
use crate::milestone_health;
use crate::model::MilestoneFile;
use crate::paths::{self, PlanContext};
use crate::reviews;

/// Condensed review-oriented projection of a milestone spec.
///
/// Reuses the on-disk milestone plus the M80 health summary (force-bypass /
/// review_state). Returned by `mp spec review <id>`.
pub fn spec_review(ctx: &PlanContext, id: &str) -> Result<Value> {
    let m = milestone::load_milestone_by_id(ctx, id)?;
    let health = milestone_health::build_milestone_health_summary(ctx, id).ok();
    let force_bypassed = health
        .as_ref()
        .map(|h| h.verification.force_bypassed)
        .unwrap_or(false);
    let review_state = health
        .as_ref()
        .map(|h| h.review_state.clone())
        .unwrap_or_default();

    // Build per-AC coverage: which steps cover each AC (via covers_ac), and
    // collect the set of ACs with no covering step (coverage gaps).
    let acs = spec_review_acceptance_criteria(&m, &m.steps, force_bypassed);
    let coverage_gaps: Vec<String> = acs
        .iter()
        .filter(|ac| {
            ac["covered_by_steps"]
                .as_array()
                .map(|a| a.is_empty())
                .unwrap_or(true)
        })
        .filter_map(|ac| ac["id"].as_str().map(String::from))
        .collect();

    let value = json!({
        "milestone": {
            "id": m.milestone.id,
            "display": paths::display_milestone_id(&m.milestone.id),
            "title": m.milestone.title,
            // M100 ER-8 follow-up: intentionally raw read of the
            // legacy fields. spec_review is a display projection; the
            // surface mirrors the on-disk shape for the reviewer to
            // see what the milestone file actually carries
            // (including the empty post-migration legacy fields).
            // Routing through `effective_*` would mask the very
            // shape the reviewer is auditing. ER-8 routed
            // decision-making sites only.
            "spec_status": m.milestone.spec_status,
            "execution_status": m.milestone.execution_status,
            "updated": m.milestone.updated,
            "priority": m.milestone.priority,
            "effort": m.milestone.effort,
            "risk": m.milestone.risk,
            "depends_on": m.milestone.depends_on,
        },
        "outcome": m.intent.outcome,
        "problem": m.problem.description,
        "scope": {
            "in_scope": m.scope.in_scope,
            "out_of_scope": m.scope.out_of_scope,
        },
        "open_questions": m.open_questions,
        "design_decisions": m.design_decisions,
        "review_state": review_state,
        "force_bypassed": force_bypassed,
        "acceptance_criteria": acs,
        "coverage_gaps": coverage_gaps,
    });
    Ok(value)
}

/// Build the per-AC review rows: status, verification, evidence, force-bypass
/// flag, and the list of steps covering it (`covered_by_steps` with id/status).
fn spec_review_acceptance_criteria(
    m: &MilestoneFile,
    steps: &[crate::model::Step],
    milestone_force_bypassed: bool,
) -> Vec<Value> {
    m.acceptance_criteria
        .iter()
        .map(|ac| {
            let covering: Vec<Value> = steps
                .iter()
                .filter(|s| s.covers_ac.iter().any(|c| c == &ac.id))
                .map(|s| {
                    json!({
                        "id": s.id,
                        "status": s.status,
                        "tests": s.tests,
                    })
                })
                .collect();
            let ac_force_bypassed = milestone_force_bypassed
                || milestone_health::evidence_marks_force_bypass(&ac.evidence);
            json!({
                "id": ac.id,
                "description": ac.description,
                "verification": ac.verification,
                "status": ac.status,
                "evidence": ac.evidence,
                "force_bypassed": ac_force_bypassed,
                "covered_by_steps": covering,
            })
        })
        .collect()
}

/// Spec-field diff since the milestone's last review (approval).
///
/// Anchors on the latest review record's `reviewed_at` date: resolves the git
/// ref at or just before that date, loads the milestone file at that ref, and
/// diffs the review-relevant spec fields (outcome, problem, scope, ACs, open
/// questions, design decisions) against the current on-disk version.
///
/// Returns a structured change list; raul renders it. When there is no prior
/// review or git cannot resolve a baseline, the report is returned with a
/// `baseline_status` explaining why (rather than erroring) so the review view
/// degrades gracefully.
pub fn spec_diff(ctx: &PlanContext, id: &str) -> Result<Value> {
    let m = milestone::load_milestone_by_id(ctx, id)?;
    let last_review = reviews::latest_review(ctx, &m.milestone.id)?;

    let baseline = match &last_review {
        Some(r) => load_milestone_at_review(ctx, &m, &r.reviewed_at),
        None => Baseline::None("no prior review record — nothing to diff".to_string()),
    };

    let (baseline_ref, baseline_status) = match &baseline {
        Baseline::At(refstr) => (Some(refstr.clone()), "resolved"),
        Baseline::None(reason) => (None, reason.as_str()),
    };

    let changes = match &baseline {
        Baseline::At(refstr) => match load_milestone_at_git(ctx, &m, refstr) {
            Some(base) => diff_spec_fields(&base, &m),
            None => Vec::new(),
        },
        Baseline::None(_) => Vec::new(),
    };

    Ok(json!({
        "milestone": {
            "id": m.milestone.id,
            "display": paths::display_milestone_id(&m.milestone.id),
            "title": m.milestone.title,
        },
        "last_review": last_review,
        "baseline_ref": baseline_ref,
        "baseline_status": baseline_status,
        "changes": changes,
    }))
}

enum Baseline {
    At(String),
    None(String),
}

/// Resolve a git ref for the milestone file at or just before `reviewed_at`.
///
/// Tries the current slug path first; if the slug changed between the review
/// date and now (so the current path has no commits at the old date), falls
/// back to matching any `milestones/<id>-*.json` path in history.
fn load_milestone_at_review(
    ctx: &PlanContext,
    current: &MilestoneFile,
    reviewed_at: &str,
) -> Baseline {
    let plan_rel = match ctx.plan_dir.strip_prefix(&ctx.project_root) {
        Ok(p) => p.to_path_buf(),
        Err(_) => return Baseline::None("plan dir not under project root".to_string()),
    };
    let current_rel = format!(
        "{}/milestones/{}-{}.json",
        plan_rel.display(),
        current.milestone.id,
        current.milestone.slug
    );
    // `--before=<date>` (date-only) means midnight, which excludes same-day
    // commits; pad to end-of-day so a review recorded earlier today still
    // anchors on today's commits.
    let date = if reviewed_at.contains('T') {
        reviewed_at.to_string()
    } else {
        format!("{reviewed_at}T23:59:59")
    };

    // 1. Current slug path.
    if let Some(hash) = git_revlist_before(&ctx.project_root, &date, &current_rel) {
        return Baseline::At(hash);
    }
    // 2. Slug-changed fallback: any historical path matching <id>-*. The
    //    `.json` suffix is intentionally omitted — git pathspec wildcard
    //    matching is unreliable with a trailing literal after `*`, and
    //    `load_milestone_at_git` already filters to `.json` when resolving.
    let glob = format!(
        "{}/milestones/{}-*",
        plan_rel.display(),
        current.milestone.id
    );
    if let Some(hash) = git_revlist_before(&ctx.project_root, &date, &glob) {
        return Baseline::At(hash);
    }
    Baseline::None(format!(
        "no commit touching the milestone file at or before {reviewed_at}"
    ))
}

/// `git rev-list -1 --before=<date> HEAD -- <path>` → first matching hash.
fn git_revlist_before(root: &std::path::Path, date: &str, path_spec: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .current_dir(root)
        .args([
            "rev-list",
            "-1",
            &format!("--before={date}"),
            "HEAD",
            "--",
            path_spec,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}

/// Load a milestone file at a git ref. Resolves the file path by slug against
/// the passed-in milestone (no reload), then falls back to scanning the
/// milestones dir at that ref in case the slug changed. Reuses the shared
/// `plan_diff` git-show helper.
fn load_milestone_at_git(
    ctx: &PlanContext,
    current: &MilestoneFile,
    git_ref: &str,
) -> Option<MilestoneFile> {
    let plan_rel = ctx.plan_dir.strip_prefix(&ctx.project_root).ok()?;
    let rel = format!(
        "{}/milestones/{}-{}.json",
        plan_rel.display(),
        current.milestone.id,
        current.milestone.slug
    );
    if let Some(m) = crate::plan_diff::git_show_milestone_json(&ctx.project_root, git_ref, &rel) {
        return Some(m);
    }
    // Slug may have changed between the baseline ref and now — list the
    // milestones dir at that ref and match by id prefix.
    let dir = format!("{}/milestones", plan_rel.display());
    let listing = std::process::Command::new("git")
        .current_dir(&ctx.project_root)
        .args(["show", &format!("{git_ref}:{dir}/")])
        .output()
        .ok()?;
    if !listing.status.success() {
        return None;
    }
    let prefix = format!("{}-", current.milestone.id);
    for line in String::from_utf8_lossy(&listing.stdout).lines() {
        let name = line.trim();
        if name.starts_with(&prefix) && name.ends_with(".json") {
            let rel = format!("{dir}/{name}");
            if let Some(m) =
                crate::plan_diff::git_show_milestone_json(&ctx.project_root, git_ref, &rel)
            {
                return Some(m);
            }
        }
    }
    None
}

/// Field-level diff of the review-relevant spec content between two milestone
/// versions. Status/title/execution fields are intentionally excluded — this
/// is a *spec* diff, not a state diff.
fn diff_spec_fields(base: &MilestoneFile, current: &MilestoneFile) -> Vec<Value> {
    let mut changes = Vec::new();
    push_str_change(
        &mut changes,
        "intent.outcome",
        &base.intent.outcome,
        &current.intent.outcome,
    );
    push_str_change(
        &mut changes,
        "problem.description",
        &base.problem.description,
        &current.problem.description,
    );
    push_list_change(
        &mut changes,
        "scope.in_scope",
        &base.scope.in_scope,
        &current.scope.in_scope,
    );
    push_list_change(
        &mut changes,
        "scope.out_of_scope",
        &base.scope.out_of_scope,
        &current.scope.out_of_scope,
    );
    diff_open_questions(&mut changes, &base.open_questions, &current.open_questions);
    diff_design_decisions(
        &mut changes,
        &base.design_decisions,
        &current.design_decisions,
    );
    // ACs: diff by id — added/removed ids plus description/verification edits.
    let base_acs: std::collections::HashMap<&str, &crate::model::AcceptanceCriterion> = base
        .acceptance_criteria
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect();
    let curr_acs: std::collections::HashMap<&str, &crate::model::AcceptanceCriterion> = current
        .acceptance_criteria
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect();
    for id in base_acs.keys() {
        if !curr_acs.contains_key(id) {
            changes.push(json!({ "field": format!("acceptance_criteria.{id}"), "from": "present", "to": null, "summary": "acceptance criterion removed" }));
        }
    }
    for (id, curr) in &curr_acs {
        match base_acs.get(*id) {
            None => changes.push(json!({ "field": format!("acceptance_criteria.{id}"), "from": null, "to": "present", "summary": "acceptance criterion added" })),
            Some(base) => {
                push_str_change(&mut changes, &format!("acceptance_criteria.{id}.description"), &base.description, &curr.description);
                push_str_change(&mut changes, &format!("acceptance_criteria.{id}.verification"), &base.verification, &curr.verification);
            }
        }
    }
    changes
}

fn push_str_change(out: &mut Vec<Value>, field: &str, from: &str, to: &str) {
    if from.trim() == to.trim() {
        return;
    }
    out.push(json!({
        "field": field,
        "from": from,
        "to": to,
        "summary": summarize_text_delta(from, to),
    }));
}

fn push_list_change(out: &mut Vec<Value>, field: &str, from: &[String], to: &[String]) {
    let from_set: std::collections::BTreeSet<&str> = from.iter().map(|s| s.as_str()).collect();
    let to_set: std::collections::BTreeSet<&str> = to.iter().map(|s| s.as_str()).collect();
    if from_set == to_set {
        return;
    }
    let added: Vec<&str> = to_set.difference(&from_set).copied().collect();
    let removed: Vec<&str> = from_set.difference(&to_set).copied().collect();
    out.push(json!({
        "field": field,
        "from": from,
        "to": to,
        "summary": format!("+{} added, -{} removed", added.len(), removed.len()),
    }));
}

fn summarize_text_delta(from: &str, to: &str) -> String {
    if from.is_empty() {
        return "field populated".to_string();
    }
    if to.is_empty() {
        return "field cleared".to_string();
    }
    format!("{} -> {} chars", from.len(), to.len())
}

/// Id-keyed diff of open questions: detects added/removed questions plus
/// edits to `question`/`status`/`answer`. Resolving a question (status change)
/// or adding an answer is a spec-relevant change a reviewer cares about.
fn diff_open_questions(
    out: &mut Vec<Value>,
    base: &[crate::model::OpenQuestion],
    current: &[crate::model::OpenQuestion],
) {
    let base_map: std::collections::HashMap<&str, &crate::model::OpenQuestion> =
        base.iter().map(|q| (q.id.as_str(), q)).collect();
    let curr_map: std::collections::HashMap<&str, &crate::model::OpenQuestion> =
        current.iter().map(|q| (q.id.as_str(), q)).collect();
    for id in base_map.keys() {
        if !curr_map.contains_key(id) {
            out.push(json!({ "field": format!("open_questions.{id}"), "from": "present", "to": null, "summary": "open question removed" }));
        }
    }
    for (id, curr) in &curr_map {
        match base_map.get(*id) {
            None => out.push(json!({ "field": format!("open_questions.{id}"), "from": null, "to": "present", "summary": "open question added" })),
            Some(b) => {
                push_str_change(out, &format!("open_questions.{id}.question"), &b.question, &curr.question);
                push_str_change(out, &format!("open_questions.{id}.status"), &b.status, &curr.status);
                push_str_change(out, &format!("open_questions.{id}.answer"), &b.answer, &curr.answer);
            }
        }
    }
}

/// Diff of design decisions (F-01: previously omitted from the spec diff
/// entirely). `DesignDecision` has no stable id and `area` is not guaranteed
/// unique (the `design-decision add` CLI even leaves it empty), so we compare
/// the two lists as sets of full `(area, choice, rationale)` tuples. A
/// reviewer sees "decision added/removed" plus a count — enough to know the
/// spec moved, without inventing a stable key that doesn't exist.
fn diff_design_decisions(
    out: &mut Vec<Value>,
    base: &[crate::model::DesignDecision],
    current: &[crate::model::DesignDecision],
) {
    let key = |d: &crate::model::DesignDecision| format!("{}|{}|{}", d.area, d.choice, d.rationale);
    let base_set: std::collections::BTreeSet<String> = base.iter().map(key).collect();
    let curr_set: std::collections::BTreeSet<String> = current.iter().map(key).collect();
    if base_set == curr_set {
        return;
    }
    let added = curr_set.difference(&base_set).count();
    let removed = base_set.difference(&curr_set).count();
    out.push(json!({
        "field": "design_decisions",
        "from": base.iter().map(|d| json!({"area": d.area, "choice": d.choice, "rationale": d.rationale})).collect::<Vec<_>>(),
        "to": current.iter().map(|d| json!({"area": d.area, "choice": d.choice, "rationale": d.rationale})).collect::<Vec<_>>(),
        "summary": format!("design decisions changed (+{added} added, -{removed} removed)"),
    }));
}
