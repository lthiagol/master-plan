use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::json_input;
use crate::model::{
    AcceptanceCriterion, Intent, MilestoneFile, MilestoneMeta, Problem, Scope, Step, Verification,
    WorkPackage,
};
use crate::paths::{self, PlanContext};
use crate::{store, validate};

use super::io::milestone_path;
use super::{
    load_milestone_by_id, load_milestone_path, next_fragment_id, with_milestone_mut_unlocked,
    write_milestone_synced,
};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct CreateAcceptanceCriterion {
    pub id: Option<String>,
    pub description: String,
    pub verification: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub evidence: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct CreateMilestoneInput {
    pub id: Option<String>,
    pub title: Option<String>,
    pub slug: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default = "default_effort")]
    pub effort: String,
    #[serde(default = "default_risk")]
    pub risk: String,
    #[serde(default)]
    pub change_kind: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub intent: Intent,
    #[serde(default)]
    pub problem: Problem,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub acceptance_criteria: Vec<CreateAcceptanceCriterion>,
    #[serde(default)]
    pub open_questions: Vec<crate::model::OpenQuestion>,
    #[serde(default)]
    pub design_decisions: Vec<crate::model::DesignDecision>,
}

fn default_effort() -> String {
    "S".to_string()
}

fn default_risk() -> String {
    "low".to_string()
}

pub fn read_create_input(
    title: Option<&str>,
    file: Option<&std::path::Path>,
    json: Option<&str>,
) -> Result<CreateMilestoneInput> {
    if let Some(path) = file {
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            anyhow::bail!(
                "milestone create --file expects a .json document; {} is not JSON (2.0 dropped TOML input)",
                path.display()
            );
        }
    }
    if file.is_some() || json.is_some() {
        let raw = json_input::read_json_payload(file, json)?;
        let value: serde_json::Value = serde_json::from_str(&raw)?;
        if let Some(obj) = value.as_object() {
            validate_create_milestone_keys(obj)?;
        }
        return Ok(serde_json::from_value(value)?);
    }
    let title = title.context("provide --title, --json, or --file")?;
    Ok(CreateMilestoneInput {
        title: Some(title.to_string()),
        ..Default::default()
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateMilestoneInput {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub depends_on: Option<Vec<String>>,
    pub effort: Option<String>,
    pub risk: Option<String>,
    pub change_kind: Option<String>,
    pub intent: Option<Intent>,
    pub problem: Option<Problem>,
    pub scope: Option<Scope>,
    pub acceptance_criteria: Option<Vec<CreateAcceptanceCriterion>>,
    pub steps: Option<Vec<Step>>,
    pub open_questions: Option<Vec<crate::model::OpenQuestion>>,
    pub work_packages: Option<Vec<WorkPackage>>,
    /// M165: optional `verification` write — when `Some`, replaces the
    /// milestone-level `verification` block (date / branch / evidence).
    /// `None` preserves the existing block. Reachable on lifecycle=complete
    /// milestones to amend the evidence (e.g. flip a `[force-bypassed`
    /// marker after a follow-up milestone closes the debt); the canonical
    /// completion-time write remains `mp milestone complete --evidence`.
    #[serde(default)]
    pub verification: Option<Verification>,
}

pub fn read_update_input(
    file: Option<&std::path::Path>,
    json: Option<&str>,
    replace_arrays: bool,
    accept_extra_fields: bool,
) -> Result<UpdateMilestoneInput> {
    if let Some(path) = file {
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            anyhow::bail!(
                "milestone update --file expects a .json document; {} is not JSON (2.0 dropped TOML input)",
                path.display()
            );
        }
    }
    let raw = json_input::read_json_payload(file, json)?;
    let value: serde_json::Value = serde_json::from_str(&raw)?;
    if let Some(obj) = value.as_object() {
        validate_update_milestone_keys(obj, replace_arrays, accept_extra_fields)?;
    }
    Ok(serde_json::from_value(value)?)
}

/// 2.0 lean spec model (M82): keys that were scaffolded in the 1.x shape
/// but removed when the spec model shed ceremony. `mp milestone create` /
/// `mp milestone update` reject them; `mp edit strip-dropped-keys` (M105 /
/// B-41) is the bulk removal utility for the 85 historical files that
/// still carry them.
///
/// Exposed `pub(crate)` so `commands::edit` (B-41) can iterate the same
/// list. Internal callers should keep using the symbolic constants
/// (`update_field_hint`, `validate_create_milestone_keys`,
/// `validate_update_milestone_keys`).
pub const DROPPED_CEREMONY_KEYS: &[&str] = &[
    "behavior",
    "context",
    "requirements",
    "success_criteria",
    "assumptions",
    "interface",
    "risks",
    "technical_context",
    "follow_ups",
];

fn create_field_hint(key: &str) -> String {
    if DROPPED_CEREMONY_KEYS.contains(&key) {
        return format!(
            "'{key}' — dropped in 2.0 lean spec model (M82); use load-bearing fields instead"
        );
    }
    update_field_hint(key)
}

fn validate_create_milestone_keys(obj: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let dropped: Vec<String> = obj
        .keys()
        .filter(|k| DROPPED_CEREMONY_KEYS.contains(&k.as_str()))
        .map(|k| create_field_hint(k))
        .collect();
    if !dropped.is_empty() {
        anyhow::bail!(
            "milestone create JSON contains dropped ceremony field(s): {}",
            dropped.join("; ")
        );
    }
    const CREATE_MILESTONE_KEYS: &[&str] = &[
        "id",
        "title",
        "slug",
        "depends_on",
        "effort",
        "risk",
        "change_kind",
        "priority",
        "intent",
        "problem",
        "scope",
        "acceptance_criteria",
        "open_questions",
        "design_decisions",
        // Accepted compatibility hints. Creation still owns initial lifecycle
        // defaults; these keys are recognized so older callers are not treated
        // as malformed merely because they include the resulting state.
        "spec_status",
        "execution_status",
        "lifecycle",
    ];
    let unknown = obj
        .keys()
        .filter(|key| !CREATE_MILESTONE_KEYS.contains(&key.as_str()))
        .map(|key| format!("'{key}'"))
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        anyhow::bail!(
            "milestone create JSON contains unsupported field(s): {}",
            unknown.join("; ")
        );
    }
    Ok(())
}

/// Top-level JSON keys accepted by `mp milestone update --json` by default.
/// Document arrays (`acceptance_criteria`, `steps`) are intentionally excluded —
/// agents must use fragment commands (`mp milestone ac …`, `mp milestone step …`)
/// rather than rebuild whole arrays. M93 AC-08.
///
/// `verification` is allowed (M165) so post-completion evidence amends can
/// land via the same `mp milestone update` surface. `mp milestone complete
/// --evidence` remains the canonical write at completion time.
const UPDATE_MILESTONE_KEYS: &[&str] = &[
    "title",
    "slug",
    "depends_on",
    "effort",
    "risk",
    "change_kind",
    "intent",
    "problem",
    "scope",
    "open_questions",
    "work_packages",
    "verification",
];

/// Document arrays that are rejected by default and only allowed via
/// `--replace-arrays` (migration / one-off repair only).
const GUARDED_UPDATE_ARRAYS: &[&str] = &["acceptance_criteria", "steps"];

fn update_field_hint(key: &str) -> String {
    if DROPPED_CEREMONY_KEYS.contains(&key) {
        return format!(
            "'{key}' — dropped in 2.0 lean spec model (M82); use load-bearing fields instead"
        );
    }
    match key {
        "design_decisions" => format!("'{key}' — use mp milestone design-decision add"),
        "findings" => format!("'{key}' — use mp reviews finding add/resolve/list"),
        "verification" => format!(
            "'{key}' — use mp milestone update --verification (M165; --verification-file / --verification-date / --verification-branch also accepted)"
        ),
        "steps" => format!("'{key}' — use mp milestone step add/update/set-status/done"),
        "acceptance_criteria" => format!(
            "'{key}' — use mp milestone ac add/update/remove (use --replace-arrays for migration scripts)"
        ),
        "milestone" => format!("'{key}' — use mp milestone set-status/set-spec-status"),
        other => format!("'{other}' — not a supported milestone update field"),
    }
}

fn validate_update_milestone_keys(
    obj: &serde_json::Map<String, serde_json::Value>,
    replace_arrays: bool,
    accept_extra_fields: bool,
) -> Result<()> {
    let allowed: std::collections::HashSet<&str> = UPDATE_MILESTONE_KEYS.iter().copied().collect();
    let guarded: std::collections::HashSet<&str> = GUARDED_UPDATE_ARRAYS.iter().copied().collect();

    let mut errors: Vec<String> = Vec::new();
    for key in obj.keys() {
        if allowed.contains(key.as_str()) {
            continue;
        }
        if guarded.contains(key.as_str()) && !replace_arrays {
            errors.push(format!(
                "'{key}' is a guarded document array (M93 AC-08); use mp milestone {ns} … commands, or pass --replace-arrays to opt into whole-array replacement (migration only)",
                ns = if key == "acceptance_criteria" { "ac" } else { "step" },
            ));
            continue;
        }
        if replace_arrays && guarded.contains(key.as_str()) {
            // --replace-arrays was passed and the key is a guarded array: allow
            // silently. Migration scripts take responsibility.
            continue;
        }
        if accept_extra_fields {
            // M111 S5 escape hatch: ignore unknown fields silently so that
            // `mp show --format raw → mp milestone update --json --accept-extra-fields`
            // round-trips without manual `jq del(...)` stripping.
            continue;
        }
        errors.push(update_field_hint(key));
    }

    if errors.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "milestone update JSON contains unsupported field(s): {}",
        errors.join("; ")
    );
}
fn map_acceptance_criteria(items: Vec<CreateAcceptanceCriterion>) -> Vec<AcceptanceCriterion> {
    items
        .into_iter()
        .enumerate()
        .map(|(i, ac)| AcceptanceCriterion {
            id: ac
                .id
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| format!("AC-{:02}", i + 1)),
            description: ac.description,
            verification: ac.verification,
            status: if ac.status.is_empty() {
                "pending".to_string()
            } else {
                ac.status
            },
            evidence: ac.evidence,
        })
        .collect()
}

pub fn create_milestone(ctx: &PlanContext, input: CreateMilestoneInput) -> Result<MilestoneFile> {
    let cfg = store::try_load_config(ctx)?;

    let title = input
        .title
        .filter(|t| !t.is_empty())
        .context("title is required")?;
    let id = match input.id {
        Some(id) => id,
        None => store::next_milestone_id(ctx)?,
    };
    let norm = paths::normalize_milestone_id(&id);
    let slug = input
        .slug
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| store::slugify(&title));
    let today = store::today();

    let acceptance_criteria: Vec<AcceptanceCriterion> =
        map_acceptance_criteria(input.acceptance_criteria);

    let milestone = MilestoneFile {
        milestone: MilestoneMeta {
            id: norm.clone(),
            title: title.clone(),
            slug: slug.clone(),
            lifecycle: "draft".to_string(),
            // M144: track lifecycle transition timestamp.
            lifecycle_at: Some(crate::store::now_rfc3339()),
            spec_status: String::new(),
            execution_status: String::new(),
            blocked: false,
            needs_regrooming: false,
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            deferred: false,
            deferred_reason: String::new(),
            depends_on: input.depends_on,
            effort: input.effort,
            risk: input.risk,
            change_kind: input.change_kind,
            priority: input.priority.unwrap_or_else(|| "normal".to_string()),
            created: today.clone(),
            updated: today,
            blocked_at: String::new(),
            block_reason: String::new(),
            blocked_by: String::new(),
            target_version: String::new(),
            executed_by: String::new(),
            remediation_pre_state: None,
            flow_stages: BTreeMap::new(),
        },
        intent: input.intent,
        problem: input.problem,
        scope: input.scope,
        acceptance_criteria,
        open_questions: input.open_questions,
        design_decisions: input.design_decisions,
        ..Default::default()
    };

    fs::create_dir_all(ctx.milestones_dir())?;
    let path = milestone_path(ctx, &norm, &slug);
    crate::schema::enforce_milestone_file(&cfg, &milestone)?;
    write_milestone_synced(ctx, &path, &milestone)?;
    // M180 S3: record one milestone-created event. Called from
    // inside the `cmd_milestone` dispatcher's plan-write lock, so
    // use the in-lock best-effort variant (AC-04 / F-02: re-acquiring
    // the lock would deadlock, and a journal failure must not bubble
    // up as a command failure).
    let _ = crate::activity::append_event_best_effort_unlocked(
        ctx,
        crate::activity::milestone_created_event(&norm),
    )?;
    Ok(milestone)
}

pub fn update_milestone(
    ctx: &PlanContext,
    id: &str,
    input: UpdateMilestoneInput,
    if_updated: Option<&str>,
) -> Result<MilestoneFile> {
    let path = load_milestone_path(ctx, id)?;
    let mut m = store::load_milestone(&path)?;

    if let Some(expected) = if_updated {
        if m.milestone.updated != expected {
            bail!(
                "milestone {id} write conflict: expected updated={expected}, found {}",
                m.milestone.updated
            );
        }
    }

    if let Some(title) = input.title.filter(|t| !t.is_empty()) {
        m.milestone.title = title;
    }
    if let Some(slug) = input.slug.filter(|s| !s.is_empty()) {
        m.milestone.slug = slug;
    }
    if let Some(depends_on) = input.depends_on {
        m.milestone.depends_on = depends_on;
    }
    if let Some(effort) = input.effort {
        m.milestone.effort = effort;
    }
    if let Some(risk) = input.risk {
        m.milestone.risk = risk;
    }
    if let Some(change_kind) = input.change_kind {
        m.milestone.change_kind = change_kind;
    }
    if let Some(intent) = input.intent {
        m.intent = intent;
    }
    if let Some(problem) = input.problem {
        m.problem = problem;
    }
    if let Some(scope) = input.scope {
        m.scope = scope;
    }
    if let Some(acs) = input.acceptance_criteria {
        m.acceptance_criteria = map_acceptance_criteria(acs);
    }
    if let Some(steps) = input.steps {
        m.steps = steps;
    }
    if let Some(questions) = input.open_questions {
        m.open_questions = questions;
    }
    if let Some(work_packages) = input.work_packages {
        m.work_packages = work_packages;
    }

    // M165: post-completion evidence amend. `Some(v)` replaces the milestone's
    // `verification` block in full (the caller sends the full shape via
    // `mp milestone update --json` / `--verification-file`). Sub-fields set to
    // empty strings here are the caller's responsibility to populate — the
    // `mp milestone update --verification` CLI path fills in sensible defaults
    // (date = today() when empty) before reaching this apply. `None` preserves
    // the existing block.
    if let Some(v) = input.verification {
        m.verification = v;
    }

    m.milestone.updated = store::today();
    write_milestone_synced(ctx, &path, &m)?;
    Ok(m)
}

pub fn create_from_handoff(ctx: &PlanContext, handoff_path: &str) -> Result<serde_json::Value> {
    let content = std::fs::read_to_string(handoff_path)
        .with_context(|| format!("failed to read handoff file: {handoff_path}"))?;

    let sections = crate::brief::parse_handoff_markdown(&content)?;

    if sections.is_empty() {
        anyhow::bail!(
            "no markdown headings found in {handoff_path}; handoff files need # or ## sections"
        );
    }

    let mut created = Vec::new();

    for (heading, body) in &sections {
        let input = CreateMilestoneInput {
            title: Some(heading.clone()),
            intent: Intent {
                outcome: body.clone(),
            },
            scope: Scope {
                in_scope: vec![heading.clone()],
                out_of_scope: vec![
                    "Other project concerns".to_string(),
                    "Out of scope items".to_string(),
                ],
            },
            ..Default::default()
        };
        let m = create_milestone(ctx, input)?;
        created.push(serde_json::json!({
            "id": m.milestone.id,
            "title": m.milestone.title,
            "slug": m.milestone.slug,
        }));
    }

    Ok(serde_json::json!({
        "ok": true,
        "milestones_created": created.len(),
        "milestones": created,
    }))
}

pub const SPEC_STATUSES: &[&str] = &[
    "draft",
    "interview",
    "review",
    "ready",
    "implemented",
    "verified",
];

/// Apply the shared pure state-machine result to an in-memory milestone.
/// This is the only non-migration assignment site used by public mp writers.
///
/// M202: every MilestoneEvent that flows through here also writes the
/// corresponding mp-flow stage mutations via `apply_flow_stages_for_event`,
/// so a milestone's 12-stage timeline stays in sync with its lifecycle
/// without each call site having to remember to do it. Hand-off is
/// intentionally never auto-advanced (AC-11); explicit
/// `mp milestone stage set <id> hand-off done` is the only path.
pub(crate) fn apply_transition(
    m: &mut MilestoneFile,
    event: crate::model::MilestoneEvent,
) -> Result<crate::model::TransitionEffects> {
    let current =
        crate::model::MilestoneState::from_meta(&m.milestone).map_err(anyhow::Error::msg)?;
    let effects =
        crate::model::transition(&current, event, crate::model::TransitionContext::default())
            .map_err(anyhow::Error::msg)?;
    m.milestone.lifecycle = effects.phase.as_str().to_string();
    m.milestone.blocked = effects.overlays.blocked;
    m.milestone.deferred = effects.overlays.deferred;
    m.milestone.cancelled = effects.overlays.cancelled;
    m.milestone.needs_regrooming = effects.overlays.needs_regrooming;
    m.milestone.remediation_pre_state = effects
        .remediation_pre_state
        .map(|phase| phase.as_str().to_string());
    m.milestone.spec_status = effects.spec_status.to_string();
    m.milestone.execution_status = effects.execution_status.to_string();
    if effects.phase_changed {
        m.milestone.lifecycle_at = Some(crate::store::now_rfc3339());
    }
    // M202: mirror the lifecycle transition into the 12-stage mp-flow
    // timeline. The pure-state function returns the (slug, new_status)
    // pairs it wrote; we ignore the return value here because the durable
    // writer only needs to know the side effect happened. Previews (the
    // apply_spec_status_with_gates dry-run path) ALSO run this so the
    // dry-run envelope mirrors what would land on disk.
    crate::model::apply_flow_stages_for_event(
        &mut m.milestone.flow_stages,
        event,
        &crate::store::now_rfc3339(),
    );
    Ok(effects)
}

fn event_for_spec_status(m: &MilestoneFile, status: &str) -> Result<crate::model::MilestoneEvent> {
    Ok(match status {
        "draft" if m.milestone.lifecycle == "draft" => crate::model::MilestoneEvent::Sync,
        "draft" => anyhow::bail!(
            "legacy set-spec-status cannot regress lifecycle {} to draft",
            m.milestone.lifecycle
        ),
        "interview" | "review" => crate::model::MilestoneEvent::Groom,
        "ready" => crate::model::MilestoneEvent::Approve,
        "implemented" => crate::model::MilestoneEvent::FinishExecution,
        // Complete requires the complete_milestone ceremony (AC verify,
        // open-findings gates, evidence). The legacy setter must not
        // jump Approved→Complete by assigning verified.
        "verified" => anyhow::bail!(
            "legacy set-spec-status cannot jump to verified/complete; \
             use `mp milestone complete` (complete ceremony gates)"
        ),
        _ => anyhow::bail!("invalid spec_status: {status}"),
    })
}

pub fn apply_spec_status(ctx: &PlanContext, id: &str, status: &str) -> Result<MilestoneFile> {
    if !SPEC_STATUSES.contains(&status) {
        anyhow::bail!(
            "invalid spec_status: {status} (expected one of: {})",
            SPEC_STATUSES.join(", ")
        );
    }
    let updated = with_milestone_mut_unlocked(ctx, id, |m| {
        let lc_was = m.milestone.lifecycle.clone();
        let event = event_for_spec_status(m, status)?;
        apply_transition(m, event)?;
        Ok((m.clone(), lc_was, m.milestone.lifecycle.clone()))
    })?;
    // M180 S3: emit lifecycle-transition event when the mapped
    // lifecycle actually changed. Same-state writes (which the
    // idempotency check at S11 / no-op deduplication passes through
    // without rewriting) emit nothing.
    let (m, lc_was, lc_now) = updated;
    crate::activity::record_lifecycle_transition(ctx, &m.milestone.id, &lc_was, &lc_now)?;
    Ok(m)
}

/// Apply `spec_status` and run the same gates the single-id path enforces
/// (G2/G3/G4/G14). Returns the gate errors when blocked; callers can surface
/// them per-id in a bulk fan-out.
///
/// `commit=false` runs the gate checks but skips the actual write. Used by
/// bulk dry-run so callers can preview which milestones would be blocked
/// without mutating any of them.
pub fn apply_spec_status_with_gates(
    ctx: &PlanContext,
    id: &str,
    status: &str,
    commit: bool,
) -> Result<ApplySpecStatusResult> {
    if !SPEC_STATUSES.contains(&status) {
        anyhow::bail!(
            "invalid spec_status: {status} (expected one of: {})",
            SPEC_STATUSES.join(", ")
        );
    }
    let path = load_milestone_path(ctx, id)?;
    let current = store::load_milestone(&path)?;
    let cfg = store::try_load_config(ctx)?;
    let mut errors = gate_errors_for_spec_status(&current, status, cfg.min_out_of_scope());
    if status == "ready" {
        errors.extend(validate::check_g14_approval_requests(ctx, id));
    }
    if !errors.is_empty() {
        return Ok(ApplySpecStatusResult::Blocked {
            gate_errors: errors,
            current: Box::new(current),
        });
    }
    if !commit {
        let mut preview = current.clone();
        let event = event_for_spec_status(&preview, status)?;
        apply_transition(&mut preview, event)?;
        return Ok(ApplySpecStatusResult::Applied(Box::new(preview)));
    }
    let m = apply_spec_status(ctx, id, status)?;
    Ok(ApplySpecStatusResult::Applied(Box::new(m)))
}

/// Result of `apply_spec_status_with_gates` — either the updated milestone
/// or the gate errors that blocked the transition.
pub enum ApplySpecStatusResult {
    Applied(Box<MilestoneFile>),
    Blocked {
        gate_errors: Vec<crate::validate::ValidationIssue>,
        current: Box<MilestoneFile>,
    },
}

pub const VALID_PRIORITIES: &[&str] = &["urgent", "high", "normal", "low"];

pub fn set_priority(ctx: &PlanContext, id: &str, priority: &str) -> Result<MilestoneFile> {
    if !VALID_PRIORITIES.contains(&priority) {
        anyhow::bail!(
            "invalid priority: {priority} (expected one of: {})",
            VALID_PRIORITIES.join(", ")
        );
    }
    with_milestone_mut_unlocked(ctx, id, |m| {
        m.milestone.priority = priority.to_string();
        Ok(m.clone())
    })
}

/// Dry-run variant: returns the milestone with priority mutated in-memory
/// but does not write to disk. Used by bulk dry-run previews.
pub fn set_priority_preview(ctx: &PlanContext, id: &str, priority: &str) -> Result<MilestoneFile> {
    if !VALID_PRIORITIES.contains(&priority) {
        anyhow::bail!(
            "invalid priority: {priority} (expected one of: {})",
            VALID_PRIORITIES.join(", ")
        );
    }
    let path = load_milestone_path(ctx, id)?;
    let mut m = store::load_milestone(&path)?;
    m.milestone.priority = priority.to_string();
    Ok(m)
}

pub fn gate_errors_for_spec_status(
    m: &MilestoneFile,
    status: &str,
    min_out_of_scope: usize,
) -> Vec<validate::ValidationIssue> {
    match status {
        "review" => validate::validate_milestone_review(m, min_out_of_scope),
        "ready" => validate::validate_milestone_ready(m, min_out_of_scope),
        _ => vec![],
    }
}

pub fn approve_milestone(ctx: &PlanContext, id: &str) -> Result<MilestoneFile> {
    apply_spec_status(ctx, id, "ready")
}

/// Print a warning to stderr if any depends_on entry references a non-existent milestone.
pub fn warn_dangling_deps(ctx: &crate::paths::PlanContext, depends_on: &[String]) {
    let milestones = crate::store::load_all_milestones(ctx).unwrap_or_default();
    let existing: std::collections::HashSet<String> = milestones
        .iter()
        .map(|(_, m)| crate::paths::normalize_milestone_id(&m.milestone.id))
        .collect();
    for dep in depends_on {
        if dep.is_empty() || dep == "none" {
            continue;
        }
        let norm = crate::paths::normalize_milestone_id(dep);
        if !existing.contains(&norm) {
            let _ = writeln!(
                std::io::stderr(),
                "warning: depends_on entry \"{dep}\" (normalized: \"{norm}\") references a non-existent milestone"
            );
        }
    }
}

pub fn spec_status_allows_steps(spec_status: &str) -> bool {
    matches!(spec_status, "ready" | "implemented" | "verified")
}

pub fn set_target_version(ctx: &PlanContext, id: &str, version: &str) -> Result<MilestoneFile> {
    let m = with_milestone_mut_unlocked(ctx, id, |m| {
        m.milestone.target_version = version.to_string();
        Ok(m.clone())
    })?;

    // Update releases registry in plan.json
    let mut plan = store::load_plan(ctx)?;
    let norm_id = crate::paths::normalize_milestone_id(id);
    let release = plan.releases.iter_mut().find(|r| r.version == version);
    if let Some(release) = release {
        if !release.milestones.contains(&norm_id) {
            release.milestones.push(norm_id);
        }
    } else {
        plan.releases.push(crate::model::ReleaseEntry {
            version: version.to_string(),
            status: "planned".to_string(),
            date: String::new(),
            milestones: vec![norm_id],
        });
    }
    store::write_plan(ctx, &plan)?;

    Ok(m)
}
/// Raw lifecycle assignment is migration-only. Public CLI dispatch still
/// reaches this function so old scripts get a precise error instead of an
/// arbitrary jump. Migration code must call `apply_migrate_raw` (which
/// routes through `MilestoneEvent::MigrateRaw`) — never assign
/// `milestone.lifecycle` as a string.
pub fn set_lifecycle(
    _ctx: &PlanContext,
    _id: &str,
    lifecycle: &str,
    _commit: bool,
) -> Result<MilestoneFile> {
    if !lifecycle.is_empty() && !crate::model::LIFECYCLE_STATES.contains(&lifecycle) {
        anyhow::bail!(
            "invalid lifecycle: {lifecycle:?} (expected one of: {} or \"\" to reset)",
            crate::model::LIFECYCLE_STATES.join(", ")
        );
    }
    anyhow::bail!(
        "set-lifecycle is migration-only; use approve, set-status, complete, reopen, block, or defer"
    )
}

/// Migration-only escape hatch: apply `MilestoneEvent::MigrateRaw` to an
/// in-memory milestone. Used by `migrate_milestone_to_lifecycle` so the
/// shared transition table remains the sole lifecycle writer.
pub(crate) fn apply_migrate_raw(m: &mut MilestoneFile, lifecycle: &str) -> Result<()> {
    let phase = if lifecycle.is_empty() {
        crate::model::MilestonePhase::Draft
    } else {
        crate::model::MilestonePhase::from_lifecycle(lifecycle).map_err(anyhow::Error::msg)?
    };
    apply_transition(m, crate::model::MilestoneEvent::MigrateRaw(phase))?;
    Ok(())
}

/// Dry-run preview for `set_lifecycle`. Mirrors `set_priority_preview`'s
/// pattern (in-memory mutate, no write) so callers — bulk-update
/// flows, the TUI's preflight checks — get the post-update file
/// without touching disk.
pub fn set_lifecycle_preview(
    ctx: &PlanContext,
    id: &str,
    lifecycle: &str,
) -> Result<MilestoneFile> {
    set_lifecycle(ctx, id, lifecycle, /* commit */ false)
}

/// Add `depends_on` to a milestone's dependency list (no-op if already present).
/// Returns an error if appending would introduce a cycle in the milestone graph.
///
/// This is the single-id entry point. It builds a fresh `depends_on` graph
/// (one full plan load) and delegates to [`add_depends_on_with_graph`]. Bulk
/// callers should call [`add_depends_on_with_graph`] directly with a
/// pre-built graph so they pay the full-plan load once per batch, not once
/// per target.
///
/// `commit=false` runs the cycle check but skips the write — used by
/// dry-run previews.
pub fn add_depends_on(
    ctx: &PlanContext,
    id: &str,
    depends_on: &str,
    commit: bool,
) -> Result<MilestoneFile> {
    let dep_norm = paths::normalize_milestone_id(depends_on);
    let graph = build_depends_on_graph(ctx)?;
    let id_norm = paths::normalize_milestone_id(id);
    let mut prospective = graph.get(&id_norm).cloned().unwrap_or_default();
    if !prospective.contains(&dep_norm) {
        prospective.push(dep_norm.clone());
    }
    if depends_on_creates_cycle_in_graph(&graph, id, &prospective) {
        anyhow::bail!("adding depends_on={dep_norm} on {id_norm} would create a cycle");
    }
    add_depends_on_with_graph(ctx, id, depends_on, commit)
}

/// Graph-aware variant of [`add_depends_on`]: the caller is responsible
/// for running the cycle check via [`depends_on_creates_cycle_in_graph`]
/// on a pre-built graph (see [`build_depends_on_graph`]) beforehand. This
/// avoids the per-target full-plan reload that the single-id path pays,
/// which matters for bulk fan-out.
///
/// `commit=false` returns a preview with the prospective dep applied but
/// does not write — used by bulk dry-run previews.
pub fn add_depends_on_with_graph(
    ctx: &PlanContext,
    id: &str,
    depends_on: &str,
    commit: bool,
) -> Result<MilestoneFile> {
    let dep_norm = paths::normalize_milestone_id(depends_on);
    with_milestone_mut_unlocked(ctx, id, |m| {
        if m.milestone
            .depends_on
            .iter()
            .any(|d| paths::normalize_milestone_id(d) == dep_norm)
        {
            // Already present — no-op but report success.
            return Ok(m.clone());
        }
        let mut prospective = m.milestone.depends_on.clone();
        prospective.push(dep_norm.clone());
        if !commit {
            // Caller asked us to only check, not write. Return the milestone
            // with the prospective deps applied so before/after previews work.
            let mut preview = m.clone();
            preview.milestone.depends_on = prospective;
            return Ok(preview);
        }
        m.milestone.depends_on = prospective;
        Ok(m.clone())
    })
}

/// Remove `depends_on` from a milestone's dependency list (no-op if absent).
/// `commit=false` runs a no-op read so dry-run can still report before/after.
pub fn remove_depends_on(
    ctx: &PlanContext,
    id: &str,
    depends_on: &str,
    commit: bool,
) -> Result<MilestoneFile> {
    let dep_norm = paths::normalize_milestone_id(depends_on);
    with_milestone_mut_unlocked(ctx, id, |m| {
        if !commit {
            let mut preview = m.clone();
            preview
                .milestone
                .depends_on
                .retain(|d| paths::normalize_milestone_id(d) != dep_norm);
            return Ok(preview);
        }
        m.milestone
            .depends_on
            .retain(|d| paths::normalize_milestone_id(d) != dep_norm);
        Ok(m.clone())
    })
}

/// Build an in-memory `id -> normalized depends_on` map for the whole plan.
/// Used by bulk depends-on ops so we only pay one full load per batch.
pub fn build_depends_on_graph(
    ctx: &PlanContext,
) -> Result<std::collections::HashMap<String, Vec<String>>> {
    let milestones = store::load_all_milestones(ctx)?;
    let mut by_id: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (_, m) in &milestones {
        let mid = paths::normalize_milestone_id(&m.milestone.id);
        let deps: Vec<String> = m
            .milestone
            .depends_on
            .iter()
            .map(|d| paths::normalize_milestone_id(d))
            .collect();
        by_id.insert(mid, deps);
    }
    Ok(by_id)
}

/// Cycle check that uses a pre-built graph from `build_depends_on_graph`,
/// avoiding the per-call full plan load in bulk fan-out scenarios.
///
/// The cycle walk is O(N) per target: it overlays prospective dependencies on
/// a read-only graph without cloning it. Callers must hold the plan write lock
/// so the snapshot remains valid for the whole batch.
pub fn depends_on_creates_cycle_in_graph(
    by_id: &std::collections::HashMap<String, Vec<String>>,
    id: &str,
    prospective_deps: &[String],
) -> bool {
    let id_norm = paths::normalize_milestone_id(id);

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack: Vec<String> = prospective_deps.to_vec();
    while let Some(cur) = stack.pop() {
        if cur == id_norm {
            return true;
        }
        if !visited.insert(cur.clone()) {
            continue;
        }
        // The prospective dep entry for `id` overlays the snapshot
        // graph read-only — we look it up via a small chained match
        // rather than cloning `by_id`.
        let deps: Option<&[String]> = if cur == id_norm {
            Some(prospective_deps)
        } else {
            by_id.get(&cur).map(|v| v.as_slice())
        };
        if let Some(deps) = deps {
            for d in deps {
                if !visited.contains(d) {
                    stack.push(d.clone());
                }
            }
        }
    }
    false
}

pub fn design_decision_add(
    ctx: &PlanContext,
    id: &str,
    area: &str,
    decision: &str,
    rationale: &str,
) -> Result<crate::model::DesignDecision> {
    with_milestone_mut_unlocked(ctx, id, |m| {
        let dd = crate::model::DesignDecision {
            area: area.to_string(),
            choice: decision.to_string(),
            rationale: rationale.to_string(),
        };
        m.design_decisions.push(dd.clone());
        Ok(dd)
    })
}

/// Update one design decision in place. `target_index` (0-based) selects by
/// position; `target_area` selects by matching area text (first hit). At least
/// one of `--index` / `--area` must be supplied; any combination of
/// `new_area`/`decision`/`rationale` is applied. Returns the mutated DD.
/// M111 S4: makes DDs mutable instead of append-only.
pub fn design_decision_update(
    ctx: &PlanContext,
    id: &str,
    target_index: Option<usize>,
    target_area: Option<String>,
    new_area: Option<String>,
    decision: Option<String>,
    rationale: Option<String>,
) -> Result<crate::model::DesignDecision> {
    if target_index.is_none() && target_area.is_none() {
        anyhow::bail!("design-decision update requires --index or --area");
    }
    if new_area.is_none() && decision.is_none() && rationale.is_none() {
        anyhow::bail!(
            "design-decision update is a no-op; pass at least one of --new-area/--decision/--rationale"
        );
    }
    with_milestone_mut_unlocked(ctx, id, |m| {
        let pos = resolve_dd_target(&m.design_decisions, target_index, target_area.as_deref())?;
        let dd = &mut m.design_decisions[pos];
        if let Some(a) = new_area {
            dd.area = a;
        }
        if let Some(c) = decision {
            dd.choice = c;
        }
        if let Some(r) = rationale {
            dd.rationale = r;
        }
        Ok(dd.clone())
    })
}

/// Remove one design decision. `target_index` (0-based) or `target_area`
/// selects the target (first match). Returns the removed DD's id (index as
/// string) for the caller's confirmation payload.
pub fn design_decision_remove(
    ctx: &PlanContext,
    id: &str,
    target_index: Option<usize>,
    target_area: Option<String>,
) -> Result<usize> {
    if target_index.is_none() && target_area.is_none() {
        anyhow::bail!("design-decision remove requires --index or --area");
    }
    let path = load_milestone_path(ctx, id)?;
    let mut m = store::load_milestone(&path)?;
    let pos = resolve_dd_target(&m.design_decisions, target_index, target_area.as_deref())?;
    m.design_decisions.remove(pos);
    m.milestone.updated = store::today();
    write_milestone_synced(ctx, &path, &m)?;
    Ok(pos)
}

fn resolve_dd_target(
    dds: &[crate::model::DesignDecision],
    target_index: Option<usize>,
    target_area: Option<&str>,
) -> Result<usize> {
    let pos = if let Some(idx) = target_index {
        if idx >= dds.len() {
            anyhow::bail!(
                "design-decision index {idx} out of range (0..{})",
                dds.len()
            );
        }
        idx
    } else if let Some(area) = target_area {
        dds.iter()
            .position(|d| d.area == area)
            .with_context(|| format!("no design-decision matches --area={area:?}"))?
    } else {
        unreachable!("resolve_dd_target called with no selector");
    };
    Ok(pos)
}

pub fn criterion_add(
    ctx: &PlanContext,
    id: &str,
    description: &str,
    verification: &str,
) -> Result<crate::model::AcceptanceCriterion> {
    let ac_id = {
        let m = load_milestone_by_id(ctx, id)?;
        next_fragment_id(&m.acceptance_criteria, |ac| ac.id.as_str(), "AC")
    };
    with_milestone_mut_unlocked(ctx, id, |m| {
        let ac = crate::model::AcceptanceCriterion {
            id: ac_id.clone(),
            description: description.to_string(),
            verification: verification.to_string(),
            status: "pending".to_string(),
            evidence: String::new(),
        };
        m.acceptance_criteria.push(ac.clone());
        Ok(ac)
    })
}

pub fn question_add(ctx: &PlanContext, id: &str, text: &str) -> Result<serde_json::Value> {
    let qid = {
        let m = load_milestone_by_id(ctx, id)?;
        next_fragment_id(&m.open_questions, |q| q.id.as_str(), "Q")
    };
    with_milestone_mut_unlocked(ctx, id, |m| {
        let q = crate::model::OpenQuestion {
            id: qid.clone(),
            question: text.to_string(),
            status: "open".to_string(),
            answer: String::new(),
        };
        m.open_questions.push(q.clone());
        Ok(serde_json::json!({ "id": qid, "question": q.question }))
    })
}

pub fn question_resolve(
    ctx: &PlanContext,
    id: &str,
    qid: &str,
    resolution: &str,
) -> Result<serde_json::Value> {
    use anyhow::Context;
    with_milestone_mut_unlocked(ctx, id, |m| {
        let q = m
            .open_questions
            .iter_mut()
            .find(|q| q.id == qid)
            .with_context(|| format!("question {qid} not found"))?;
        q.status = "resolved".to_string();
        q.answer = resolution.to_string();
        Ok(serde_json::json!({ "id": qid, "resolved": true }))
    })
}

/// Read-only: return a single acceptance criterion as a JSON value (id, description,
/// verification, status, evidence). No milestone write — agents should never need
/// to load the whole document to inspect one AC. (M93 AC-01.)
pub fn criterion_show(ctx: &PlanContext, id: &str, ac_id: &str) -> Result<serde_json::Value> {
    let m = load_milestone_by_id(ctx, id)?;
    let ac = m
        .acceptance_criteria
        .iter()
        .find(|ac| ac.id == ac_id)
        .with_context(|| format!("acceptance criterion {ac_id} not found in milestone {id}"))?;
    Ok(serde_json::to_value(ac)?)
}

/// Read-only: return all acceptance criteria for a milestone as a JSON array of
/// single-fragment objects. Used by `mp milestone ac list`. (M93 AC-01.)
pub fn criterion_list(ctx: &PlanContext, id: &str) -> Result<serde_json::Value> {
    let m = load_milestone_by_id(ctx, id)?;
    Ok(serde_json::to_value(&m.acceptance_criteria)?)
}

/// Mutator: update one acceptance criterion in place. Returns only the changed
/// fragment — `{ id, description?, verification?, evidence? }` containing just the
/// fields the caller asked to change (plus `id` for unambiguous identification).
/// M93 AC-04 fragment-only stdout contract: every mutator returns only the
/// changed fragment, never the whole document.
///
/// M111 S1: accept `--evidence` so per-AC run evidence can be stamped without
/// the `--replace-arrays` escape hatch.
pub fn criterion_update(
    ctx: &PlanContext,
    id: &str,
    ac_id: &str,
    description: Option<String>,
    verification: Option<String>,
    evidence: Option<String>,
) -> Result<serde_json::Value> {
    if description.is_none() && verification.is_none() && evidence.is_none() {
        bail!("criterion update requires --description, --verification, and/or --evidence");
    }
    with_milestone_mut_unlocked(ctx, id, |m| {
        let ac = m
            .acceptance_criteria
            .iter_mut()
            .find(|ac| ac.id == ac_id)
            .with_context(|| format!("acceptance criterion {ac_id} not found in milestone {id}"))?;
        if let Some(d) = &description {
            ac.description = d.clone();
        }
        if let Some(v) = &verification {
            ac.verification = v.clone();
        }
        if let Some(e) = &evidence {
            ac.evidence = e.clone();
        }
        // Fragment-only contract: build an object with just the changed fields
        // (plus id so callers can confirm which AC was touched).
        let mut fragment = serde_json::Map::new();
        fragment.insert("id".to_string(), serde_json::Value::String(ac.id.clone()));
        if let Some(d) = description {
            fragment.insert("description".to_string(), serde_json::Value::String(d));
        }
        if let Some(v) = verification {
            fragment.insert("verification".to_string(), serde_json::Value::String(v));
        }
        if let Some(e) = evidence {
            fragment.insert("evidence".to_string(), serde_json::Value::String(e));
        }
        Ok(serde_json::Value::Object(fragment))
    })
}

/// Bulk AC update from a JSON array file. Each element applies
/// through the same per-AC update flow as [`criterion_update`]: same
/// shell-parse preflight (`sh -n`), same evidence preflight, same
/// fragment-only stdout contract. Empty array is a no-op (returns
/// `{ok, applied: 0}`). A missing `id` field or unknown AC id fails
/// fast with a per-element error so a typo in one entry doesn't
/// half-apply the rest.
///
/// Path-mode only (defer stdin-mode per design_decisions; matches
/// `mp milestone update --json @file` precedent).
pub fn criterion_bulk_update(
    ctx: &PlanContext,
    id: &str,
    bulk_path: &Path,
) -> Result<serde_json::Value> {
    // M118 findings follow-up (B-59): load the milestone up-front so a
    // missing milestone short-circuits with one clean error rather
    // than N noisy "AC-X not found in milestone Y" errors per element.
    // We don't read the file contents here (the per-AC loop does
    // its own load through `criterion_update` / `with_milestone_mut_unlocked`),
    // only the existence check. The lookup itself goes through the
    // canonical path so any "milestone does not exist" error matches
    // what `mp milestone show <id>` would emit on the same id.
    match load_milestone_path(ctx, id) {
        Ok(_) => {}
        Err(e) => {
            bail!("cannot bulk-update acceptance criteria: {e}");
        }
    }

    // Read & validate the JSON file.
    let raw = fs::read_to_string(bulk_path)
        .with_context(|| format!("read bulk file {}", bulk_path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("parse {} as JSON", bulk_path.display()))?;
    let arr = match value.as_array() {
        Some(a) => a,
        None => bail!(
            "bulk update payload must be a JSON array; got {} at root",
            value_kind(&value)
        ),
    };
    if arr.is_empty() {
        return Ok(json!({ "ok": true, "applied": 0, "results": [] }));
    }

    // Validate every row before the first write.
    let mut prepared: Vec<BulkUpdateRow> = Vec::with_capacity(arr.len());
    for (idx, el) in arr.iter().enumerate() {
        let obj = el
            .as_object()
            .with_context(|| format!("bulk[{idx}] must be an object; got {}", value_kind(el)))?;
        let ac_id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .with_context(|| format!("bulk[{idx}] missing required `id` field"))?
            .to_string();
        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let verification = obj
            .get("verification")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let evidence = obj
            .get("evidence")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if description.is_none() && verification.is_none() && evidence.is_none() {
            bail!(
                "bulk[{idx}] (id={ac_id}): at least one of description, verification, evidence must be supplied"
            );
        }
        prepared.push(BulkUpdateRow {
            ac_id,
            description,
            verification,
            evidence,
        });
    }

    // Every AC id must exist before any write, preventing a late unknown-id
    // error from leaving the milestone partially updated.
    //
    // The update phase uses unlocked mutation primitives and therefore
    // requires the caller to hold the plan-write lock. The CLI dispatcher
    // does so; direct callers must establish the same lock discipline.
    {
        let path = load_milestone_path(ctx, id).with_context(|| format!("milestone {id}"))?;
        let m = store::load_milestone(&path).with_context(|| format!("load milestone {id}"))?;
        let known: std::collections::BTreeSet<&str> = m
            .acceptance_criteria
            .iter()
            .map(|ac| ac.id.as_str())
            .collect();
        let unknown: Vec<&str> = prepared
            .iter()
            .map(|r| r.ac_id.as_str())
            .filter(|id| !known.contains(id))
            .collect();
        if !unknown.is_empty() {
            bail!(
                "cannot bulk-update acceptance criteria: milestone {id} has no AC(s) {} \
                 (known: {}); fix the `id` field and retry",
                unknown
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                known
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }

    // Apply through the single-AC path. Shell parsing remains a warning, and
    // bulk responses preserve the same preflight diagnostics.
    let mut results = Vec::with_capacity(prepared.len());
    let mut preflight_warnings: Vec<serde_json::Value> = Vec::new();
    for row in &prepared {
        let ac = criterion_update(
            ctx,
            id,
            &row.ac_id,
            row.description.clone(),
            row.verification.clone(),
            row.evidence.clone(),
        )?;
        if let Some(verification) = row.verification.as_deref() {
            if let Some(warning) = crate::commands::common::shell_parse_preflight(verification) {
                preflight_warnings.push(serde_json::json!({
                    "id": row.ac_id,
                    "warning": warning,
                }));
            }
        }
        // Surface the AC fragment WITH the id field preserved so
        // callers can match results back to inputs without relying on
        // array ordering (M118 CR F-5).
        results.push(ac);
    }
    let applied = results.len();
    let mut payload = json!({
        "ok": true,
        "applied": applied,
        "milestone_id": id,
        "results": results,
    });
    if !preflight_warnings.is_empty() {
        payload["preflight_warnings"] = serde_json::Value::Array(preflight_warnings);
    }
    Ok(payload)
}

/// Structured row for the bulk-update apply phase. Lifted out of the
/// inline 4-tuple so the field names appear at the call site (M118 CR
/// F-7 — primitive 4-tuple made the loop body opaque).
struct BulkUpdateRow {
    ac_id: String,
    description: Option<String>,
    verification: Option<String>,
    evidence: Option<String>,
}

/// Tiny JSON-value-kind label for richer bulk-element errors. Lifted
/// to module scope so both `criterion_bulk_update` and callers can
/// surface useful context in failures.
fn value_kind(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}
/// Mutator: remove one acceptance criterion. Fails when any step covers this AC.
/// (M93 AC-05.)
pub fn criterion_remove(ctx: &PlanContext, id: &str, ac_id: &str) -> Result<serde_json::Value> {
    // Coverage guard: refuse if any step `covers_ac` includes this id.
    let path = load_milestone_path(ctx, id)?;
    let m = store::load_milestone(&path)?;
    let covering_steps: Vec<String> = m
        .steps
        .iter()
        .filter(|s| s.covers_ac.iter().any(|c| c == ac_id))
        .map(|s| s.id.clone())
        .collect();
    if !covering_steps.is_empty() {
        bail!(
            "cannot remove acceptance criterion {ac_id} from milestone {id}: covered by step(s) {}",
            covering_steps.join(", ")
        );
    }

    let mut m = m;
    let removed = {
        let pos = m
            .acceptance_criteria
            .iter()
            .position(|ac| ac.id == ac_id)
            .with_context(|| format!("acceptance criterion {ac_id} not found in milestone {id}"))?;
        m.acceptance_criteria.remove(pos);
        ac_id.to_string()
    };
    m.milestone.updated = store::today();
    write_milestone_synced(ctx, &path, &m)?;
    Ok(serde_json::json!({ "ok": true, "removed": removed }))
}
pub fn split_milestone(
    ctx: &PlanContext,
    id: &str,
    into: u32,
    titles: Option<Vec<String>>,
) -> Result<serde_json::Value> {
    if into < 2 {
        anyhow::bail!("--into must be at least 2");
    }

    let parent_path = load_milestone_path(ctx, id)?;
    let parent = store::load_milestone(&parent_path)?;
    let parent_id = paths::normalize_milestone_id(&parent.milestone.id);

    if parent_id.contains('.') {
        anyhow::bail!("cannot split child milestone {parent_id}");
    }

    let title_list = titles.unwrap_or_else(|| {
        (1..=into)
            .map(|part| format!("{} (part {part})", parent.milestone.title))
            .collect()
    });
    if title_list.len() != into as usize {
        anyhow::bail!(
            "expected {} title(s) with --titles, got {}",
            into,
            title_list.len()
        );
    }

    let mut all = store::load_all_milestones(ctx)?;
    let mut children = Vec::new();

    for title in title_list {
        let child_id = next_milestone_child_id(&parent_id, &all);
        let child = create_milestone(
            ctx,
            CreateMilestoneInput {
                id: Some(child_id.clone()),
                title: Some(title),
                depends_on: vec![parent_id.clone()],
                intent: parent.intent.clone(),
                problem: parent.problem.clone(),
                scope: parent.scope.clone(),
                effort: parent.milestone.effort.clone(),
                risk: parent.milestone.risk.clone(),
                change_kind: parent.milestone.change_kind.clone(),
                acceptance_criteria: parent
                    .acceptance_criteria
                    .iter()
                    .map(|ac| CreateAcceptanceCriterion {
                        id: Some(ac.id.clone()),
                        description: ac.description.clone(),
                        verification: ac.verification.clone(),
                        status: "pending".to_string(),
                        evidence: String::new(),
                    })
                    .collect(),
                open_questions: parent.open_questions.clone(),
                ..Default::default()
            },
        )?;
        children.push(child.milestone.id.clone());
        all = store::load_all_milestones(ctx)?;
    }

    let mut parent = store::load_milestone(&parent_path)?;
    parent.milestone.updated = store::today();
    write_milestone_synced(ctx, &parent_path, &parent)?;

    Ok(serde_json::json!({
        "ok": true,
        "parent_id": parent_id,
        "children": children,
    }))
}

fn next_milestone_child_id(
    parent_id: &str,
    milestones: &[(std::path::PathBuf, MilestoneFile)],
) -> String {
    let prefix = format!("{parent_id}.");
    let mut max = 0u32;
    for (_, m) in milestones {
        let mid = paths::normalize_milestone_id(&m.milestone.id);
        if let Some(rest) = mid.strip_prefix(&prefix) {
            if let Some(n) = rest.split('.').next().and_then(|p| p.parse().ok()) {
                max = max.max(n);
            }
        }
    }
    format!("{parent_id}.{}", max + 1)
}
