use anyhow::Result;
use serde_json::json;

use crate::backlog;
use crate::cli::{ListTarget, OutputFormat as Fmt};
use crate::commands::archive as cmd_archive_mod;
use crate::commands::common::emit_value;
use crate::commands::track as cmd_track_mod;
use crate::decisions;
use crate::groom;
use crate::path_engine;
use crate::paths::{self, PlanContext};
use crate::store;

pub(crate) fn cmd_list(
    ctx: &PlanContext,
    target: ListTarget,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    match target {
        ListTarget::Milestones {
            filter,
            status,
            spec_status,
            include_archived,
            preset,
            r#where,
            include,
            take,
            select,
            sort,
        } => {
            let milestones = store::load_all_milestones(ctx)?;
            let status_filter: Vec<&str> = status
                .as_deref()
                .map(|s| s.split(',').collect())
                .unwrap_or_default();
            let spec_filter: Vec<&str> = spec_status
                .as_deref()
                .map(|s| s.split(',').collect())
                .unwrap_or_default();

            let preset_filter = preset.as_deref();
            let where_filters: Vec<WhereFilter> = parse_where_filters(&r#where);

            let include_steps = include.iter().any(|i| i == "steps");
            let include_acs = include.iter().any(|i| i == "acceptance_criteria");
            let include_evidence = include.iter().any(|i| i == "evidence");
            let include_findings = include.iter().any(|i| i == "findings");

            let mut items = Vec::new();
            for (_, m) in milestones {
                if let Some(f) = filter.as_deref() {
                    if !groom::milestone_matches_filter(&m, f, ctx)? {
                        continue;
                    }
                }
                if !status_filter.is_empty()
                    && !status_filter.contains(&m.milestone.execution_status.as_str())
                {
                    continue;
                }
                if !spec_filter.is_empty()
                    && !spec_filter.contains(&m.milestone.spec_status.as_str())
                {
                    continue;
                }
                if let Some(preset) = preset_filter {
                    if !milestone_matches_preset(&m, preset) {
                        continue;
                    }
                }
                if !where_filters
                    .iter()
                    .all(|wf| milestone_matches_where(&m, wf))
                {
                    continue;
                }
                items.push(build_milestone_item(
                    &m,
                    include_steps,
                    include_acs,
                    include_evidence,
                    include_findings,
                ));
            }
            if include_archived {
                if let Ok(archived_paths) = store::list_archived_milestones(ctx) {
                    for p in archived_paths {
                        if let Ok(m) = store::load_milestone(&p) {
                            let mut item = build_milestone_item(
                                &m,
                                include_steps,
                                include_acs,
                                include_evidence,
                                include_findings,
                            );
                            if let serde_json::Value::Object(ref mut map) = item {
                                map.insert("archived".to_string(), json!(true));
                            }
                            items.push(item);
                        }
                    }
                }
            }
            // Sort numerically by milestone id. The on-disk ordering from
            // load_all_milestones is lexicographic on the filename, so ids ≥100
            // ("100-...") sort between "10-..." and "11-..." — wrong once a plan
            // crosses 100 milestones (and again at 1000). compare_milestone_ids
            // parses the numeric id; reuse the same helper sync.rs uses.
            items.sort_by(|a, b| {
                let ia = a["id"].as_str().unwrap_or("");
                let ib = b["id"].as_str().unwrap_or("");
                paths::compare_milestone_ids(ia, ib)
            });
            apply_projection_flags(&mut items, take, select.as_deref(), sort.as_deref())?;
            let value = json!({ "milestones": items });
            emit_value(format, &value, fields)
        }
        ListTarget::Tracks { items } => cmd_track_mod::cmd_track_list(ctx, format, fields, items),
        ListTarget::Steps {
            milestone,
            status,
            include_archived,
            take,
            select,
            sort,
        } => cmd_list_steps(ListStepsOptions {
            ctx,
            milestone: milestone.as_deref(),
            status: status.as_deref(),
            include_archived,
            format,
            fields,
            take,
            select: select.as_deref(),
            sort: sort.as_deref(),
        }),
        ListTarget::Archived { entity_type } => {
            cmd_archive_mod::cmd_list_archived(ctx, entity_type.as_deref(), format, fields)
        }
        ListTarget::Backlog { status } => {
            let items = backlog::backlog_list(ctx, status.as_deref())?;
            let value = json!({ "backlog": items });
            emit_value(format, &value, fields)
        }
        ListTarget::Decisions => {
            let items = decisions::decision_list(ctx)?;
            let value = json!({ "decisions": items });
            emit_value(format, &value, fields)
        }
    }
}

pub(crate) struct ListStepsOptions<'a> {
    pub(crate) ctx: &'a PlanContext,
    pub(crate) milestone: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) include_archived: bool,
    pub(crate) format: Fmt,
    pub(crate) fields: &'a [String],
    pub(crate) take: Option<usize>,
    pub(crate) select: Option<&'a str>,
    pub(crate) sort: Option<&'a str>,
}

pub(crate) fn cmd_list_steps(opts: ListStepsOptions) -> Result<()> {
    let milestones = store::load_all_milestones(opts.ctx)?;
    let status_filter: Vec<&str> = opts
        .status
        .map(|s| s.split(',').collect())
        .unwrap_or_default();
    let mut items = Vec::new();
    for (_, m) in milestones {
        let mid = paths::normalize_milestone_id(&m.milestone.id);
        if let Some(filter) = opts.milestone {
            if mid != paths::normalize_milestone_id(filter) {
                continue;
            }
        }
        for step in &m.steps {
            if !status_filter.is_empty() && !status_filter.contains(&step.status.as_str()) {
                continue;
            }
            items.push(json!({
                "milestone": mid,
                "milestone_display": paths::display_milestone_id(&m.milestone.id),
                "step": step,
            }));
        }
    }
    if opts.include_archived {
        if let Ok(archived_paths) = store::list_archived_milestones(opts.ctx) {
            for p in archived_paths {
                if let Ok(m) = store::load_milestone(&p) {
                    let mid = paths::normalize_milestone_id(&m.milestone.id);
                    if let Some(filter) = opts.milestone {
                        if mid != paths::normalize_milestone_id(filter) {
                            continue;
                        }
                    }
                    for step in &m.steps {
                        if !status_filter.is_empty()
                            && !status_filter.contains(&step.status.as_str())
                        {
                            continue;
                        }
                        items.push(json!({
                            "milestone": mid,
                            "milestone_display": paths::display_milestone_id(&m.milestone.id),
                            "step": step,
                            "archived": true,
                        }));
                    }
                }
            }
        }
    }
    items.sort_by(|a, b| {
        let ma = a["milestone"].as_str().unwrap_or("");
        let mb = b["milestone"].as_str().unwrap_or("");
        paths::compare_milestone_ids(ma, mb).then_with(|| {
            let sa = a["step"]["id"].as_str().unwrap_or("");
            let sb = b["step"]["id"].as_str().unwrap_or("");
            path_engine::compare_step_ids(sa, sb)
        })
    });
    apply_projection_flags(&mut items, opts.take, opts.select, opts.sort)?;
    let value = json!({ "steps": items });
    emit_value(opts.format, &value, opts.fields)
}

pub(crate) struct WhereFilter {
    pub(crate) field: String,
    pub(crate) op: String,
    pub(crate) value: String,
}

pub(crate) fn parse_where_filters(raw: &[String]) -> Vec<WhereFilter> {
    let mut filters = Vec::new();
    for entry in raw {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(pos) = trimmed.find("!=") {
            let field = trimmed[..pos].trim().to_string();
            let value = trimmed[pos + 2..].trim().to_string();
            if !field.is_empty() {
                filters.push(WhereFilter {
                    field,
                    op: "!=".to_string(),
                    value,
                });
            }
        } else if let Some(pos) = trimmed.find("==") {
            let field = trimmed[..pos].trim().to_string();
            let value = trimmed[pos + 2..].trim().to_string();
            if !field.is_empty() {
                filters.push(WhereFilter {
                    field,
                    op: "==".to_string(),
                    value,
                });
            }
        } else if trimmed.contains('=') {
            // Single '=' is not valid — hint at ==
            eprintln!(
                "mp: warning: --where '{}' has a single '='; did you mean '=='? (entry ignored)",
                trimmed
            );
        } else {
            eprintln!(
                "mp: warning: --where '{}' has no operator (expected == or !=); entry ignored",
                trimmed
            );
        }
    }
    filters
}

fn milestone_matches_preset(m: &crate::model::MilestoneFile, preset: &str) -> bool {
    match preset {
        "force-bypassed" => {
            m.verification.evidence.contains("[force-bypassed")
                || m.verification
                    .evidence
                    .contains("[step-tests force-bypassed")
        }
        _ => true,
    }
}

pub(crate) fn milestone_matches_where(m: &crate::model::MilestoneFile, wf: &WhereFilter) -> bool {
    // M100: legacy fields may be empty on disk; filter against the
    // effective (derived) value so existing `mp list milestones --where
    // spec_status==ready` calls keep working through the migration window.
    let field_value: String = match wf.field.as_str() {
        "spec_status" => effective_legacy_spec_status(m),
        "execution_status" => effective_legacy_execution_status(m),
        "lifecycle" => m.effective_lifecycle(),
        "id" => m.milestone.id.clone(),
        "title" => m.milestone.title.clone(),
        "risk" => m.milestone.risk.clone(),
        "effort" => m.milestone.effort.clone(),
        "priority" => m.milestone.priority.clone(),
        // M182 S1: add `updated` to the where-filter lookup so scripts
        // can filter milestones by their last-touch date.
        "updated" => m.milestone.updated.clone(),
        _ => return true,
    };
    match wf.op.as_str() {
        "==" => field_value == wf.value,
        "!=" => field_value != wf.value,
        _ => true,
    }
}

/// M100: derive the legacy `spec_status` view from the unified lifecycle so
/// callers using the legacy field name keep getting the same answer during
/// the migration window. Once all on-disk milestones have `lifecycle` set,
/// the legacy fields go away and this helper collapses.
fn effective_legacy_spec_status(m: &crate::model::MilestoneFile) -> String {
    if !m.milestone.spec_status.is_empty() {
        return m.milestone.spec_status.clone();
    }
    match m.effective_lifecycle().as_str() {
        "draft" => "draft".to_string(),
        "groomed" => "review".to_string(),
        "approved" => "ready".to_string(),
        "in-progress" => "ready".to_string(), // execution_status=in-progress implies spec=ready
        "done" => "implemented".to_string(),
        "self-reviewed" => "implemented".to_string(),
        "reviewed" => "implemented".to_string(),
        "complete" => "verified".to_string(),
        "remediation" => "implemented".to_string(),
        other => other.to_string(),
    }
}

/// M100: derive the legacy `execution_status` view from the unified lifecycle.
fn effective_legacy_execution_status(m: &crate::model::MilestoneFile) -> String {
    if !m.milestone.execution_status.is_empty() {
        return m.milestone.execution_status.clone();
    }
    if m.milestone.blocked {
        return "blocked".to_string();
    }
    if m.milestone.deferred {
        return "deferred".to_string();
    }
    if m.milestone.cancelled {
        return "cancelled".to_string();
    }
    match m.effective_lifecycle().as_str() {
        "draft" | "groomed" | "approved" => "planned".to_string(),
        "in-progress" => "in-progress".to_string(),
        "done" | "self-reviewed" | "reviewed" | "complete" | "remediation" => "done".to_string(),
        _ => "planned".to_string(),
    }
}

fn build_milestone_item(
    m: &crate::model::MilestoneFile,
    include_steps: bool,
    include_acs: bool,
    include_evidence: bool,
    include_findings: bool,
) -> serde_json::Value {
    let mut item = json!({
        "id": m.milestone.id,
        "display": paths::display_milestone_id(&m.milestone.id),
        "title": m.milestone.title,
        "lifecycle": m.effective_lifecycle(),
        // M144: emit lifecycle transition timestamp so the TUI can render
        // the "since" column as a relative time. `None` for milestones
        // predating M144 (no recorded transition) — the TUI renders
        // "since updated" in that case.
        "lifecycle_at": m.milestone.lifecycle_at,
        // M100: emit legacy fields as derived values so external scripts keep
        // working during the migration window.
        "spec_status": effective_legacy_spec_status(m),
        "execution_status": effective_legacy_execution_status(m),
        "executed_by": m.milestone.executed_by,
        // M182 S1: additive fields so clients (notably raul's sort-rebind
        // menu) can render + sort by priority and updated client-side
        // without a per-milestone `show` round-trip. The fields are
        // already on `MilestoneMeta`; pre-M182 the list projection
        // dropped them. `updated` is the date the milestone file was
        // last touched (YYYY-MM-DD), and is always populated for new
        // milestones; older milestones may carry an empty string,
        // which sorts to the bottom under ascending order.
        "priority": m.milestone.priority,
        "updated": m.milestone.updated,
    });
    if let serde_json::Value::Object(ref mut map) = item {
        if include_steps {
            map.insert(
                "steps".to_string(),
                serde_json::to_value(&m.steps).unwrap_or(json!([])),
            );
        }
        if include_acs {
            map.insert(
                "acceptance_criteria".to_string(),
                serde_json::to_value(&m.acceptance_criteria).unwrap_or(json!([])),
            );
        }
        if include_evidence {
            map.insert(
                "evidence".to_string(),
                json!({
                    "date": m.verification.date,
                    "verification": m.verification.evidence,
                }),
            );
        }
        if include_findings {
            map.insert(
                "findings".to_string(),
                serde_json::to_value(&m.findings).unwrap_or(json!([])),
            );
        }
    }
    item
}

/// M112 S3: apply `--take N`, `--select 'a.b'`, `--sort 'field'` to an
/// already-built list of items. The flags compose in the order
/// `sort → select → take` (sort first because select projects from the
/// resulting order, take last because it slices the projected array).
/// Unknown fields in `--select`/`--sort` are forwarded as-is to the
/// path-style lookup so the helper surfaces "unknown path" the same way
/// the rest of `--fields` does.
///
/// `--sort` accepts a `-`-prefixed field name for descending order
/// (`--sort -id` = newest first). The default (no prefix) remains
/// ascending — backward-compatible with every existing caller.
pub(crate) fn apply_projection_flags(
    items: &mut Vec<serde_json::Value>,
    take: Option<usize>,
    select: Option<&str>,
    sort: Option<&str>,
) -> Result<()> {
    if let Some(field) = sort {
        sort_items_by_field(items, field)?;
    }
    if let Some(path) = select {
        let mut projected = Vec::with_capacity(items.len());
        for item in items.iter() {
            let mut leaf = None;
            let mut current = item;
            let mut segments = path.split('.');
            for seg in &mut segments {
                let value = match current {
                    serde_json::Value::Object(map) => match map.get(seg) {
                        Some(v) => v,
                        None => break,
                    },
                    _ => break,
                };
                leaf = Some(value);
                current = value;
            }
            if let Some(v) = leaf {
                projected.push(v.clone());
            } else {
                // Missing field — emit null so callers can still align by
                // index. (Strict mode would error here; consumers that want
                // to error can post-process.)
                projected.push(serde_json::Value::Null);
            }
        }
        *items = projected;
    }
    if let Some(n) = take {
        items.truncate(n);
    }
    Ok(())
}

fn sort_items_by_field(items: &mut [serde_json::Value], field: &str) -> Result<()> {
    // Direction is encoded as an optional leading `-` (Unix convention,
    // mirroring `sort -k`, `ls -t` vs `ls -tr`). A bare `-` alone or a
    // bare field both go through the same comparator; only the final
    // `.reverse()` flips the direction. Default (no `-`) is ascending —
    // preserved for every existing caller.
    let (field, descending) = if let Some(stripped) = field.strip_prefix('-') {
        (stripped, true)
    } else {
        (field, false)
    };
    // The default sort path in `cmd_list` already routes the `id` field
    // through `paths::compare_milestone_ids` (numeric compare, split on
    // `.` and parse each segment as u32). Without the same routing here,
    // `--sort id` falls back to lexicographic string compare — which
    // breaks at any plan with mixed-width IDs ("1", "2", "10", "100"
    // come out as "1", "10", "100", "2", "3"). Match the default path
    // so explicit `--sort id` / `--sort -id` give the same order as the
    // implicit default. Dotted ids like "1.2" and "1.10" also sort
    // correctly under numeric compare.
    //
    // Shape dispatch: milestone items carry `id` at the top level, but
    // step items from `mp list steps` nest the step under `"step"`
    // (`{milestone, milestone_display, step: {id, ...}}`). `--sort id`
    // on step items resolves to "" on both sides → trivial no-op (or
    // trivial reverse when descending); `--sort step.id` is the correct
    // user-facing key for the nested shape. We dispatch here so neither
    // case silently misbehaves.
    if field == "id" || field == "milestone.id" || field == "step.id" {
        // Milestone items carry `id` at the top level; step items nest
        // the step under `"step"` (`{milestone, milestone_display, step:
        // {id, ...}}`). `--sort id` on step items resolves to "" on
        // both sides → Ordering::Equal (no-op); `--sort step.id` is the
        // correct user-facing key for the nested shape.
        //
        // Step ids have a separate numeric comparator
        // (`compare_step_ids`) that strips the leading "S" and parses
        // segments. `compare_milestone_ids` would split "S1" / "S2"
        // into empty key vectors (the leading "S" blocks digit parse)
        // and silently produce all-equal comparisons — `path_engine`
        // already uses this comparator for `cmd_list_steps`'s default
        // sort, so we match it here.
        items.sort_by(|a, b| {
            let (va, vb) = if field == "step.id" {
                (
                    a["step"]["id"].as_str().unwrap_or(""),
                    b["step"]["id"].as_str().unwrap_or(""),
                )
            } else {
                (
                    a["id"].as_str().unwrap_or(""),
                    b["id"].as_str().unwrap_or(""),
                )
            };
            if field == "step.id" {
                crate::path_engine::compare_step_ids(va, vb)
            } else {
                crate::paths::compare_milestone_ids(va, vb)
            }
        });
        if descending {
            items.reverse();
        }
        return Ok(());
    }
    // Stable-ish lexicographic sort by the dotted field value. Milestones
    // and steps share the same sort-key shape (string-leaning), so a single
    // string-based comparator is enough.
    items.sort_by(|a, b| {
        // M182 S1: `--sort priority` sorts by priority rank
        // (urgent > high > regular > low > ?), not alphabetical
        // string compare. The rank helper is shared with `path_prefs`;
        // we use the same descending-comparator pattern the path
        // engine uses, so `urgent` is "greater than" `high` (rank 4 >
        // 3), and the row at the top of the list is the
        // highest-priority milestone. This matches `path_prefs.rs`
        // `b_pri.cmp(&a_pri)` — divergent from a strict `.cmp(&)` on
        // numeric rank.
        let ord = if field == "priority" {
            let va = a["priority"].as_str().unwrap_or("");
            let vb = b["priority"].as_str().unwrap_or("");
            crate::path_prefs::priority_rank(vb).cmp(&crate::path_prefs::priority_rank(va))
        } else {
            let va = field_value_as_str(a, field);
            let vb = field_value_as_str(b, field);
            match (va, vb) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        };
        ord
    });
    if descending {
        items.reverse();
    }
    Ok(())
}

fn field_value_as_str(value: &serde_json::Value, field: &str) -> Option<String> {
    let mut current = value;
    for seg in field.split('.') {
        match current {
            serde_json::Value::Object(map) => {
                let v = map.get(seg)?;
                current = v;
            }
            _ => return None,
        }
    }
    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Null => Some(String::new()),
        _ => None,
    }
}
