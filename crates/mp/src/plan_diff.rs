use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::json;

use crate::model::{
    HandoffBaseline, IndexSnapshot, MilestoneFile, MilestoneSnapshot, PlanFile, StepStatusSnapshot,
};
use crate::paths::{self, PlanContext};
use crate::store;

#[derive(Debug, Clone, Default)]
pub struct PlanDiffOptions {
    pub since_handoff: bool,
    pub since: Option<String>,
    pub git_ref: Option<String>,
    pub markdown: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanDiffReport {
    pub ok: bool,
    pub clean: bool,
    pub since: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plan_changes: Vec<FieldChange>,
    pub changed_milestones: Vec<MilestoneDiff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneDiff {
    pub id: String,
    pub display: String,
    pub title: String,
    pub changes: Vec<FieldChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldChange {
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub summary: String,
}

pub fn plan_diff(ctx: &PlanContext, opts: PlanDiffOptions) -> Result<PlanDiffReport> {
    let plan = store::load_plan(ctx)?;
    let since_label = resolve_since_label(&plan, &opts)?;

    let (plan_changes, changed_milestones) = if let Some(git_ref) = opts.git_ref.as_deref() {
        (Vec::new(), diff_since_git(ctx, git_ref)?)
    } else if opts.since_handoff {
        if plan.execution.handoff_baseline.milestones.is_empty() {
            bail!(
                "no handoff baseline snapshot; run mp execution handoff to establish a diff baseline"
            );
        }
        diff_against_baseline(ctx, &plan, &plan.execution.handoff_baseline)?
    } else {
        let cutoff = parse_cutoff(&since_label)?;
        (Vec::new(), diff_since_mtime(ctx, cutoff)?)
    };

    let clean = plan_changes.is_empty() && changed_milestones.is_empty();
    let mut report = PlanDiffReport {
        ok: true,
        clean,
        since: since_label,
        plan_changes,
        changed_milestones,
        markdown: None,
    };
    if opts.markdown {
        report.markdown = Some(render_markdown(&report));
    }
    Ok(report)
}

pub fn capture_handoff_baseline(ctx: &PlanContext, plan: &PlanFile) -> Result<HandoffBaseline> {
    let milestones = store::load_all_milestones(ctx)?;
    Ok(build_baseline(plan, &milestones))
}

pub fn changed_milestone_ids_between(
    previous: &HandoffBaseline,
    current: &HandoffBaseline,
) -> Vec<String> {
    if previous.milestones.is_empty() {
        return Vec::new();
    }
    let mut ids = HashSet::new();
    let prev: HashMap<_, _> = previous
        .milestones
        .iter()
        .map(|m| (m.id.clone(), m))
        .collect();
    let curr: HashMap<_, _> = current
        .milestones
        .iter()
        .map(|m| (m.id.clone(), m))
        .collect();

    for (id, snap) in &curr {
        match prev.get(id) {
            Some(old) if milestone_snapshots_equal(old, snap) => {}
            _ => {
                ids.insert(id.clone());
            }
        }
    }
    for id in prev.keys() {
        if !curr.contains_key(id) {
            ids.insert(id.clone());
        }
    }

    let mut out: Vec<_> = ids.into_iter().collect();
    out.sort();
    out
}

fn resolve_since_label(plan: &PlanFile, opts: &PlanDiffOptions) -> Result<String> {
    if let Some(git_ref) = &opts.git_ref {
        return Ok(format!("git:{git_ref}"));
    }
    if opts.since_handoff {
        if plan.execution.handoff_at.is_empty() {
            bail!("no handoff recorded; use --since <iso> or --git <ref>");
        }
        return Ok(plan.execution.handoff_at.clone());
    }
    if let Some(since) = &opts.since {
        return Ok(since.clone());
    }
    bail!("specify --since-handoff, --since <iso>, or --git <ref>");
}

fn parse_cutoff(since: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(since) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(since, "%Y-%m-%d") {
        return Ok(date.and_hms_opt(0, 0, 0).context("invalid date")?.and_utc());
    }
    bail!("invalid --since value: {since} (expected RFC3339 or YYYY-MM-DD)");
}

fn build_baseline(plan: &PlanFile, milestones: &[(PathBuf, MilestoneFile)]) -> HandoffBaseline {
    let mut milestone_index: Vec<IndexSnapshot> = plan
        .milestones
        .iter()
        .map(|e| IndexSnapshot {
            id: e.id.clone(),
            title: e.title.clone(),
            spec_status: e.spec_status.clone(),
            execution_status: e.execution_status.clone(),
        })
        .collect();
    milestone_index.sort_by(|a, b| paths::compare_milestone_ids(&a.id, &b.id));

    let mut snaps: Vec<MilestoneSnapshot> = milestones
        .iter()
        .map(|(_, m)| milestone_snapshot(m))
        .collect();
    snaps.sort_by(|a, b| paths::compare_milestone_ids(&a.id, &b.id));

    HandoffBaseline {
        planning_status: plan.project.planning_status.clone(),
        execution_mode: plan.execution.mode.clone(),
        milestone_index,
        milestones: snaps,
    }
}

fn milestone_snapshot(m: &MilestoneFile) -> MilestoneSnapshot {
    let mut steps: Vec<StepStatusSnapshot> = m
        .steps
        .iter()
        .map(|s| StepStatusSnapshot {
            id: s.id.clone(),
            status: s.status.clone(),
        })
        .collect();
    steps.sort_by(|a, b| crate::path_engine::compare_step_ids(&a.id, &b.id));
    // M100: derive legacy `spec_status` / `execution_status` so the baseline
    // snapshot uses the same vocabulary the diff consumer expects, even when
    // the milestone file has been migrated to the new shape.
    let (spec_status, execution_status) = derive_legacy_status_for_diff(m);
    MilestoneSnapshot {
        id: m.milestone.id.clone(),
        title: m.milestone.title.clone(),
        spec_status,
        execution_status,
        updated: m.milestone.updated.clone(),
        steps,
    }
}

fn milestone_snapshots_equal(a: &MilestoneSnapshot, b: &MilestoneSnapshot) -> bool {
    a.title == b.title
        && a.spec_status == b.spec_status
        && a.execution_status == b.execution_status
        && a.updated == b.updated
        && a.steps == b.steps
}

fn diff_against_baseline(
    ctx: &PlanContext,
    plan: &PlanFile,
    baseline: &HandoffBaseline,
) -> Result<(Vec<FieldChange>, Vec<MilestoneDiff>)> {
    let milestones = store::load_all_milestones(ctx)?;
    let current = build_baseline(plan, &milestones);
    Ok((
        diff_plan_fields(baseline, &current),
        diff_milestone_snapshots(baseline, &current),
    ))
}

fn diff_plan_fields(baseline: &HandoffBaseline, current: &HandoffBaseline) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    push_if_changed(
        &mut changes,
        "plan.json:project.planning_status",
        &baseline.planning_status,
        &current.planning_status,
    );
    push_if_changed(
        &mut changes,
        "plan.json:execution.mode",
        &baseline.execution_mode,
        &current.execution_mode,
    );

    let prev_index: HashMap<_, _> = baseline
        .milestone_index
        .iter()
        .map(|e| (e.id.clone(), e))
        .collect();
    let curr_index: HashMap<_, _> = current
        .milestone_index
        .iter()
        .map(|e| (e.id.clone(), e))
        .collect();

    for (id, entry) in &curr_index {
        match prev_index.get(id) {
            None => changes.push(field_change(
                &format!("plan.json:milestones.{id}"),
                None,
                Some(entry.title.clone()),
                format!("index added milestone {id}"),
            )),
            Some(old) => {
                push_if_changed(
                    &mut changes,
                    &format!("plan.json:milestones.{id}.spec_status"),
                    &old.spec_status,
                    &entry.spec_status,
                );
                push_if_changed(
                    &mut changes,
                    &format!("plan.json:milestones.{id}.execution_status"),
                    &old.execution_status,
                    &entry.execution_status,
                );
                push_if_changed(
                    &mut changes,
                    &format!("plan.json:milestones.{id}.title"),
                    &old.title,
                    &entry.title,
                );
            }
        }
    }
    for id in prev_index.keys() {
        if !curr_index.contains_key(id) {
            changes.push(field_change(
                &format!("plan.json:milestones.{id}"),
                Some("present".into()),
                None,
                format!("index removed milestone {id}"),
            ));
        }
    }
    changes
}

fn diff_milestone_snapshots(
    baseline: &HandoffBaseline,
    current: &HandoffBaseline,
) -> Vec<MilestoneDiff> {
    let prev: HashMap<_, _> = baseline
        .milestones
        .iter()
        .map(|m| (m.id.clone(), m))
        .collect();
    let curr: HashMap<_, _> = current
        .milestones
        .iter()
        .map(|m| (m.id.clone(), m))
        .collect();

    let mut ids: HashSet<String> = prev.keys().chain(curr.keys()).cloned().collect();
    let mut out = Vec::new();
    for id in ids.drain() {
        let changes = match (prev.get(&id), curr.get(&id)) {
            (None, Some(snap)) => vec![field_change(
                "milestone",
                None,
                Some("added".into()),
                format!("new milestone {}", snap.title),
            )],
            (Some(_), None) => vec![field_change(
                "milestone",
                Some("present".into()),
                None,
                format!("removed milestone {id}"),
            )],
            (Some(old), Some(new)) => diff_snapshot_pair(old, new),
            (None, None) => continue,
        };
        if changes.is_empty() {
            continue;
        }
        let title = curr
            .get(&id)
            .map(|s| s.title.clone())
            .or_else(|| prev.get(&id).map(|s| s.title.clone()))
            .unwrap_or_default();
        out.push(MilestoneDiff {
            id: id.clone(),
            display: paths::display_milestone_id(&id),
            title,
            changes,
        });
    }
    out.sort_by(|a, b| paths::compare_milestone_ids(&a.id, &b.id));
    out
}

fn diff_snapshot_pair(old: &MilestoneSnapshot, new: &MilestoneSnapshot) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    push_if_changed(
        &mut changes,
        "milestone.spec_status",
        &old.spec_status,
        &new.spec_status,
    );
    push_if_changed(
        &mut changes,
        "milestone.execution_status",
        &old.execution_status,
        &new.execution_status,
    );
    push_if_changed(&mut changes, "milestone.title", &old.title, &new.title);
    push_if_changed(
        &mut changes,
        "milestone.updated",
        &old.updated,
        &new.updated,
    );

    let old_steps: HashMap<_, _> = old
        .steps
        .iter()
        .map(|s| (s.id.clone(), &s.status))
        .collect();
    let new_steps: HashMap<_, _> = new
        .steps
        .iter()
        .map(|s| (s.id.clone(), &s.status))
        .collect();
    for (step_id, status) in &new_steps {
        match old_steps.get(step_id) {
            Some(old_status) if old_status.as_str() == status.as_str() => {}
            Some(old_status) => changes.push(field_change(
                &format!("steps.{step_id}.status"),
                Some((*old_status).clone()),
                Some((*status).clone()),
                format!("{step_id}: {old_status} → {status}"),
            )),
            None => changes.push(field_change(
                &format!("steps.{step_id}"),
                None,
                Some((*status).clone()),
                format!("added step {step_id} ({status})"),
            )),
        }
    }
    for step_id in old_steps.keys() {
        if !new_steps.contains_key(step_id) {
            changes.push(field_change(
                &format!("steps.{step_id}"),
                Some("present".into()),
                None,
                format!("removed step {step_id}"),
            ));
        }
    }
    changes
}

fn diff_since_mtime(ctx: &PlanContext, cutoff: DateTime<Utc>) -> Result<Vec<MilestoneDiff>> {
    let milestones = store::load_all_milestones(ctx)?;
    let mut out = Vec::new();
    for (path, m) in milestones {
        if milestone_file_changed_since(&path, cutoff) {
            out.push(milestone_diff_added_touch(&m));
        }
    }
    out.sort_by(|a, b| paths::compare_milestone_ids(&a.id, &b.id));
    Ok(out)
}

fn milestone_file_changed_since(path: &Path, cutoff: DateTime<Utc>) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let modified: DateTime<Utc> = modified.into();
    modified >= cutoff
}

fn milestone_diff_added_touch(m: &MilestoneFile) -> MilestoneDiff {
    MilestoneDiff {
        id: m.milestone.id.clone(),
        display: paths::display_milestone_id(&m.milestone.id),
        title: m.milestone.title.clone(),
        changes: vec![field_change(
            "milestone.updated",
            None,
            Some(m.milestone.updated.clone()),
            format!("file touched ({})", m.milestone.updated),
        )],
    }
}

fn diff_since_git(ctx: &PlanContext, git_ref: &str) -> Result<Vec<MilestoneDiff>> {
    let plan_rel = plan_rel_path(ctx)?;
    let changed_files = git_diff_names(&ctx.project_root, git_ref, &plan_rel)?;
    let mut out = Vec::new();
    for file in changed_files {
        if milestone_id_from_plan_path(&file).is_some() {
            let current_path = ctx.plan_dir.join(format!("milestones/{file}"));
            let current = store::load_milestone(&current_path)?;
            let baseline = load_milestone_at_git(&ctx.project_root, git_ref, &plan_rel, &file);
            out.push(diff_milestone_files(&current, baseline.as_ref()));
        }
    }
    out.sort_by(|a, b| paths::compare_milestone_ids(&a.id, &b.id));
    Ok(out)
}

fn plan_rel_path(ctx: &PlanContext) -> Result<PathBuf> {
    ctx.plan_dir
        .strip_prefix(&ctx.project_root)
        .map(|p| p.to_path_buf())
        .with_context(|| format!("plan dir {} not under project root", ctx.plan_dir.display()))
}

fn git_diff_names(root: &Path, git_ref: &str, plan_rel: &Path) -> Result<Vec<String>> {
    let spec = format!("{}/", plan_rel.display());
    let out = Command::new("git")
        .current_dir(root)
        .args(["diff", "--name-only", git_ref, "--", &spec])
        .output()
        .context("failed to run git diff")?;
    if !out.status.success() {
        bail!("git diff failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let mut files = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix(&format!("{}/milestones/", plan_rel.display())) {
            if rest.ends_with(".json") {
                files.push(rest.to_string());
            }
        }
    }
    Ok(files)
}

fn milestone_id_from_plan_path(file: &str) -> Option<String> {
    let stem = file.strip_suffix(".json")?;
    stem.split('-').next().map(|id| id.to_string())
}

fn load_milestone_at_git(
    root: &Path,
    git_ref: &str,
    plan_rel: &Path,
    file: &str,
) -> Option<MilestoneFile> {
    let git_path = format!("{}/milestones/{file}", plan_rel.display());
    if let Some(m) = git_show_milestone_json(root, git_ref, &git_path) {
        return Some(m);
    }
    // Pre-M92 refs stored milestones as `.toml`; fall back when JSON is absent.
    let toml_file = file
        .strip_suffix(".json")
        .map(|stem| format!("{stem}.toml"));
    let toml_file = toml_file?;
    let toml_path = format!("{}/milestones/{toml_file}", plan_rel.display());
    git_show_milestone_toml(root, git_ref, &toml_path)
}

pub fn git_show_milestone_json(
    root: &Path,
    git_ref: &str,
    git_path: &str,
) -> Option<MilestoneFile> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["show", &format!("{git_ref}:{git_path}")])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_str(std::str::from_utf8(&out.stdout).ok()?).ok()
}

fn git_show_milestone_toml(root: &Path, git_ref: &str, git_path: &str) -> Option<MilestoneFile> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["show", &format!("{git_ref}:{git_path}")])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = std::str::from_utf8(&out.stdout).ok()?;
    let toml_value: toml::Value = toml::from_str(raw).ok()?;
    toml_value.try_into().ok()
}

fn diff_milestone_files(
    current: &MilestoneFile,
    baseline: Option<&MilestoneFile>,
) -> MilestoneDiff {
    let mut changes = Vec::new();
    match baseline {
        None => {
            changes.push(field_change(
                "milestone",
                None,
                Some("added".into()),
                "new milestone file".into(),
            ));
        }
        Some(base) => {
            push_if_changed(
                &mut changes,
                "milestone.spec_status",
                &base.milestone.spec_status,
                &current.milestone.spec_status,
            );
            push_if_changed(
                &mut changes,
                "milestone.execution_status",
                &base.milestone.execution_status,
                &current.milestone.execution_status,
            );
            // M100 ER-8 follow-up (deliberate deferral): diffing
            // `spec_status` / `execution_status` here is correct for
            // legacy-shape milestones but does not surface a
            // `lifecycle`-field transition on a migrated milestone
            // whose raw legacy fields are empty. Adding a
            // `lifecycle` push-if-changed block requires a new field
            // in `FieldChange` and would shift the existing diff
            // shape that downstream consumers (changelog emit,
            // review-lane filters) key on. Tracked as a separate
            // follow-up alongside the ER-8 reader sweep in
            // `crates/mp/src/{reviews,graph,digest,groom,
            // plan_gaps,wp,execution,skill}.rs`.
            push_if_changed(
                &mut changes,
                "milestone.title",
                &base.milestone.title,
                &current.milestone.title,
            );
            diff_step_statuses_files(&mut changes, base, current);
        }
    }
    MilestoneDiff {
        id: current.milestone.id.clone(),
        display: paths::display_milestone_id(&current.milestone.id),
        title: current.milestone.title.clone(),
        changes,
    }
}

fn diff_step_statuses_files(
    changes: &mut Vec<FieldChange>,
    base: &MilestoneFile,
    current: &MilestoneFile,
) {
    for step in &current.steps {
        if let Some(old) = base.steps.iter().find(|s| s.id == step.id) {
            if old.status != step.status {
                changes.push(field_change(
                    &format!("steps.{}.status", step.id),
                    Some(old.status.clone()),
                    Some(step.status.clone()),
                    format!("{}: {} → {}", step.id, old.status, step.status),
                ));
            }
        } else {
            changes.push(field_change(
                &format!("steps.{}", step.id),
                None,
                Some(step.status.clone()),
                format!("added step {} ({})", step.id, step.status),
            ));
        }
    }
}

fn push_if_changed(changes: &mut Vec<FieldChange>, field: &str, from: &str, to: &str) {
    if from != to {
        changes.push(field_change(
            field,
            Some(from.to_string()),
            Some(to.to_string()),
            format!("{from} → {to}"),
        ));
    }
}

fn field_change(
    field: &str,
    from: Option<String>,
    to: Option<String>,
    summary: String,
) -> FieldChange {
    FieldChange {
        field: field.to_string(),
        from,
        to,
        summary,
    }
}

fn render_markdown(report: &PlanDiffReport) -> String {
    if report.clean {
        return format!("No plan changes since {}.", report.since);
    }
    let mut lines = vec![format!("# Plan diff since {}", report.since), String::new()];
    if !report.plan_changes.is_empty() {
        lines.push("## plan.json".to_string());
        for c in &report.plan_changes {
            lines.push(format!("- **{}**: {}", c.field, c.summary));
        }
        lines.push(String::new());
    }
    for m in &report.changed_milestones {
        lines.push(format!("## {} — {}", m.display, m.title));
        for c in &m.changes {
            lines.push(format!("- **{}**: {}", c.field, c.summary));
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

pub fn handoff_show(ctx: &PlanContext) -> Result<serde_json::Value> {
    let plan = store::load_plan(ctx)?;
    Ok(json!({
        "ok": true,
        "handoff_at": if plan.execution.handoff_at.is_empty() { serde_json::Value::Null } else { json!(plan.execution.handoff_at) },
        "handoff_by": plan.execution.handoff_by,
        "changed_milestone_ids": plan.execution.handoff_changed_milestones,
        "changed_milestones": plan.execution.handoff_changed_milestones.iter().map(|id| paths::display_milestone_id(id)).collect::<Vec<_>>(),
        "baseline_milestones": plan.execution.handoff_baseline.milestones.len(),
    }))
}

/// M100: derive legacy `spec_status` / `execution_status` from the unified
/// lifecycle so baseline snapshots and current snapshots use the same
/// vocabulary the diff consumer expects. Mirrors `derive_index_status` in
/// sync.rs; kept as a separate function to avoid a crate-internal dep.
fn derive_legacy_status_for_diff(m: &MilestoneFile) -> (String, String) {
    let spec = if !m.milestone.spec_status.is_empty() {
        m.milestone.spec_status.clone()
    } else {
        match m.effective_lifecycle().as_str() {
            "draft" => "draft".to_string(),
            "groomed" => "review".to_string(),
            "approved" => "ready".to_string(),
            "in-progress" => "ready".to_string(),
            "done" => "implemented".to_string(),
            "self-reviewed" => "implemented".to_string(),
            "reviewed" => "implemented".to_string(),
            "complete" => "verified".to_string(),
            "remediation" => "implemented".to_string(),
            other => other.to_string(),
        }
    };
    let exec = if !m.milestone.execution_status.is_empty() {
        m.milestone.execution_status.clone()
    } else if m.milestone.blocked {
        "blocked".to_string()
    } else if m.milestone.deferred {
        "deferred".to_string()
    } else if m.milestone.cancelled {
        "cancelled".to_string()
    } else {
        match m.effective_lifecycle().as_str() {
            "draft" | "groomed" | "approved" => "planned".to_string(),
            "in-progress" => "in-progress".to_string(),
            "done" | "self-reviewed" | "reviewed" | "complete" | "remediation" => {
                "done".to_string()
            }
            _ => "planned".to_string(),
        }
    };
    (spec, exec)
}
