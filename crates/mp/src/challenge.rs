use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;

use crate::assets;
use crate::backlog;
use crate::model::{ChallengeFile, ChallengeFinding, ChallengeMeta, MilestoneFile};
use crate::paths::{self, PlanContext};
use crate::plan_gaps;
use crate::step::{self, UpdateStepInput};
use crate::store;
use crate::validate;

const SCOPES: &[&str] = &["spec", "plan", "full", "sequence"];

#[derive(Debug, Serialize)]
pub struct ChallengeListReport {
    pub challenges: Vec<ChallengeSummary>,
    pub findings: Vec<ChallengeFinding>,
}

#[derive(Debug, Serialize)]
pub struct ChallengeSummary {
    pub id: String,
    pub milestone_id: String,
    pub scope: String,
    pub status: String,
    pub created: String,
    pub open_findings: usize,
    pub path: String,
}

pub fn challenge_start(
    ctx: &PlanContext,
    milestone_id: Option<&str>,
    scope: &str,
) -> Result<ChallengeFile> {
    if !SCOPES.contains(&scope) {
        bail!("invalid scope {scope}; use spec|plan|full|sequence");
    }
    let milestone_id = if scope == "sequence" {
        String::new()
    } else {
        let id = milestone_id.context("milestone id required for non-sequence scope")?;
        paths::normalize_milestone_id(id)
    };

    if scope != "sequence" {
        let _ = crate::milestone::load_milestone_path(ctx, &milestone_id)?;
    }

    if let Some(open) = find_open_challenge(ctx, &milestone_id)? {
        bail!("open challenge already exists: {}", open.challenge.id);
    }

    let (path, challenge_id) = next_challenge_path(ctx, &milestone_id, scope)?;
    let file = ChallengeFile {
        challenge: ChallengeMeta {
            id: challenge_id,
            milestone_id,
            scope: scope.to_string(),
            status: "open".to_string(),
            created: store::today(),
            closed: String::new(),
            summary: String::new(),
        },
        findings: vec![],
    };
    store::write_challenge(ctx, &path, &file)?;
    Ok(file)
}

pub fn challenge_audit(
    ctx: &PlanContext,
    milestone_id: &str,
    scope: Option<&str>,
) -> Result<ChallengeFile> {
    let mut challenge = open_challenge_for_milestone(ctx, milestone_id)?;
    let audit_scope = scope
        .map(str::to_string)
        .unwrap_or_else(|| challenge.challenge.scope.clone());
    let mid = paths::normalize_milestone_id(milestone_id);

    if audit_scope == "sequence" {
        audit_sequence(ctx, &mut challenge)?;
    } else {
        let path = crate::milestone::load_milestone_path(ctx, &mid)?;
        let m = store::load_milestone(&path)?;
        if matches!(audit_scope.as_str(), "spec" | "full") {
            let min_out_of_scope = store::try_load_config(ctx)
                .map(|c| c.min_out_of_scope())
                .unwrap_or(2);
            audit_spec(&m, min_out_of_scope, &mut challenge);
        }
        if matches!(audit_scope.as_str(), "plan" | "full") {
            audit_plan(ctx, &mid, &mut challenge)?;
        }
    }
    persist_challenge(ctx, &challenge)?;
    Ok(challenge)
}

pub fn challenge_list(
    ctx: &PlanContext,
    milestone_id: Option<&str>,
    status: Option<&str>,
) -> Result<ChallengeListReport> {
    let mut challenges = Vec::new();
    let mut findings = Vec::new();

    for path in store::list_challenge_paths(ctx)? {
        let file = store::load_challenge(&path)?;
        if let Some(mid) = milestone_id {
            if paths::normalize_milestone_id(&file.challenge.milestone_id)
                != paths::normalize_milestone_id(mid)
            {
                continue;
            }
        }
        let open_findings = file.findings.iter().filter(|f| f.status == "open").count();
        if milestone_id.is_some() {
            for f in &file.findings {
                if status.is_none_or(|s| f.status == s) {
                    findings.push(f.clone());
                }
            }
        }
        if status.is_none() || file.challenge.status == status.unwrap() {
            challenges.push(ChallengeSummary {
                id: file.challenge.id.clone(),
                milestone_id: file.challenge.milestone_id.clone(),
                scope: file.challenge.scope.clone(),
                status: file.challenge.status.clone(),
                created: file.challenge.created.clone(),
                open_findings,
                path: path
                    .strip_prefix(&ctx.plan_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string(),
            });
        }
    }
    challenges.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(ChallengeListReport {
        challenges,
        findings,
    })
}

pub fn challenge_add(
    ctx: &PlanContext,
    milestone_id: &str,
    title: &str,
    severity: &str,
    category: &str,
    target: Option<&str>,
    description: Option<&str>,
) -> Result<ChallengeFinding> {
    let mut challenge = open_challenge_for_milestone(ctx, milestone_id)?;
    let finding = ChallengeFinding {
        id: next_finding_id(&challenge.findings),
        severity: severity.to_string(),
        category: category.to_string(),
        title: title.to_string(),
        description: description.unwrap_or("").to_string(),
        target: target.unwrap_or("").to_string(),
        status: "open".to_string(),
        resolution: String::new(),
        action: String::new(),
        action_ref: String::new(),
    };
    challenge.findings.push(finding.clone());
    persist_challenge(ctx, &challenge)?;
    Ok(finding)
}

pub fn challenge_resolve(
    ctx: &PlanContext,
    milestone_id: &str,
    finding_id: &str,
    action: &str,
    payload: Option<&str>,
    resolution: Option<&str>,
    dry_run: bool,
) -> Result<ChallengeFinding> {
    let mut challenge = open_challenge_for_milestone(ctx, milestone_id)?;
    let finding = challenge
        .findings
        .iter_mut()
        .find(|f| f.id == finding_id)
        .with_context(|| format!("finding {finding_id} not found"))?;
    if finding.status != "open" {
        bail!("finding {finding_id} is not open");
    }

    let resolution_text = resolution.unwrap_or("").to_string();
    if dry_run {
        return Ok(ChallengeFinding {
            status: "open".to_string(),
            action: action.to_string(),
            resolution: resolution_text,
            ..finding.clone()
        });
    }

    let action_ref = apply_resolution_action(
        ctx,
        milestone_id,
        finding,
        action,
        payload,
        &resolution_text,
    )?;
    finding.status = "resolved".to_string();
    finding.action = action.to_string();
    finding.resolution = resolution_text;
    if let Some(r) = action_ref {
        finding.action_ref = r;
    }
    let out = finding.clone();
    persist_challenge(ctx, &challenge)?;
    Ok(out)
}

pub fn challenge_dismiss(
    ctx: &PlanContext,
    milestone_id: &str,
    finding_id: &str,
    reason: &str,
) -> Result<ChallengeFinding> {
    let mut challenge = open_challenge_for_milestone(ctx, milestone_id)?;
    let finding = challenge
        .findings
        .iter_mut()
        .find(|f| f.id == finding_id)
        .with_context(|| format!("finding {finding_id} not found"))?;
    finding.status = "dismissed".to_string();
    finding.resolution = reason.to_string();
    finding.action = "no-change".to_string();
    let out = finding.clone();
    persist_challenge(ctx, &challenge)?;
    Ok(out)
}

pub fn challenge_done(ctx: &PlanContext, milestone_id: &str) -> Result<ChallengeFile> {
    let mut challenge = open_challenge_for_milestone(ctx, milestone_id)?;
    if challenge.findings.iter().any(|f| f.status == "open") {
        bail!("cannot close challenge with open findings");
    }
    challenge.challenge.status = "closed".to_string();
    challenge.challenge.closed = store::today();
    persist_challenge(ctx, &challenge)?;
    Ok(challenge)
}

fn apply_resolution_action(
    ctx: &PlanContext,
    milestone_id: &str,
    finding: &ChallengeFinding,
    action: &str,
    payload: Option<&str>,
    resolution: &str,
) -> Result<Option<String>> {
    match action {
        "no-change" => Ok(None),
        "defer-backlog" => {
            let item = backlog::backlog_add(
                ctx,
                if resolution.is_empty() {
                    &finding.title
                } else {
                    resolution
                },
                Some("challenge"),
                None,
                Some("low"),
            )?;
            Ok(Some(item.id))
        }
        "update-step" => {
            let step_id = parse_step_target(&finding.target)?;
            let input = parse_step_payload(payload)?;
            let updated = step::update_step(ctx, milestone_id, step_id, input)?;
            Ok(Some(updated.id))
        }
        "split-step" => {
            let step_id = parse_step_target(&finding.target)?;
            let steps = step::split_step(ctx, milestone_id, step_id)?;
            Ok(steps.first().map(|s| s.id.clone()))
        }
        _ => bail!("unsupported action {action}"),
    }
}

fn parse_step_target(target: &str) -> Result<&str> {
    target
        .strip_prefix("step:")
        .filter(|s| !s.is_empty())
        .context("target must be step:<id>")
}

fn parse_step_payload(payload: Option<&str>) -> Result<UpdateStepInput> {
    let raw = payload.context("--payload required for update-step")?;
    let v: Value = serde_json::from_str(raw).context("payload must be JSON")?;
    Ok(UpdateStepInput {
        action: v.get("action").and_then(|x| x.as_str()).map(str::to_string),
        files: v.get("files").and_then(|x| x.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        }),
        tests: v.get("tests").and_then(|x| x.as_str()).map(str::to_string),
        done_when: v
            .get("done_when")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        covers_ac: v.get("covers_ac").and_then(|x| x.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        }),
        work_package: v
            .get("work_package")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        depends_on_steps: None,
        evidence: v
            .get("evidence")
            .and_then(|x| x.as_str())
            .map(str::to_string),
    })
}

fn audit_spec(m: &MilestoneFile, min_out_of_scope: usize, challenge: &mut ChallengeFile) {
    for err in validate::validate_milestone_ready(m, min_out_of_scope) {
        push_finding(
            challenge,
            "major",
            "gap",
            &err.message,
            &format!("milestone:{}", m.milestone.id),
            &err.message,
        );
    }
    if m.acceptance_criteria.is_empty() {
        push_finding(
            challenge,
            "blocker",
            "coverage",
            "No acceptance criteria",
            "milestone",
            "acceptance criteria required",
        );
    }
    for q in &m.open_questions {
        if q.status == "open" {
            push_finding(
                challenge,
                "major",
                "gap",
                &format!("Open question {}", q.id),
                &format!("question:{}", q.id),
                &q.question,
            );
        }
    }
}

fn audit_plan(ctx: &PlanContext, milestone_id: &str, challenge: &mut ChallengeFile) -> Result<()> {
    let gaps = plan_gaps::plan_gaps(ctx, milestone_id)?;
    for gap in gaps.missing {
        push_finding(
            challenge,
            &gap.severity,
            "gap",
            &gap.message,
            &gap.field,
            &gap.message,
        );
    }
    for ac in gaps.coverage.acceptance_criteria {
        if ac.covered_by.is_empty() {
            push_finding(
                challenge,
                "major",
                "coverage",
                &format!("AC {} has no covering step", ac.id),
                &format!("ac:{}", ac.id),
                "add step with covers_ac",
            );
        }
    }
    Ok(())
}

fn audit_sequence(ctx: &PlanContext, challenge: &mut ChallengeFile) -> Result<()> {
    let milestones = store::load_all_milestones(ctx)?;
    let ids: std::collections::HashSet<String> = milestones
        .iter()
        .map(|(_, m)| paths::normalize_milestone_id(&m.milestone.id))
        .collect();
    for (_, m) in &milestones {
        for dep in &m.milestone.depends_on {
            if dep.is_empty() || dep == "none" {
                continue;
            }
            let dep_id = paths::normalize_milestone_id(dep);
            if !ids.contains(&dep_id) {
                push_finding(
                    challenge,
                    "blocker",
                    "sequencing",
                    &format!("M{} depends on missing {dep_id}", m.milestone.id),
                    &format!("milestone:{}", m.milestone.id),
                    &format!("dependency {dep_id} not found"),
                );
            }
        }
    }
    Ok(())
}

fn push_finding(
    challenge: &mut ChallengeFile,
    severity: &str,
    category: &str,
    title: &str,
    target: &str,
    description: &str,
) {
    if challenge
        .findings
        .iter()
        .any(|f| f.status == "open" && f.target == target && f.title == title)
    {
        return;
    }
    challenge.findings.push(ChallengeFinding {
        id: next_finding_id(&challenge.findings),
        severity: severity.to_string(),
        category: category.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        target: target.to_string(),
        status: "open".to_string(),
        resolution: String::new(),
        action: String::new(),
        action_ref: String::new(),
    });
}

fn next_finding_id(findings: &[ChallengeFinding]) -> String {
    let max = findings
        .iter()
        .filter_map(|f| f.id.strip_prefix("F-"))
        .filter_map(|s| s.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("F-{:02}", max + 1)
}

fn next_challenge_path(
    ctx: &PlanContext,
    milestone_id: &str,
    scope: &str,
) -> Result<(std::path::PathBuf, String)> {
    let prefix = if scope == "sequence" {
        "roadmap".to_string()
    } else {
        format!("{milestone_id}-{scope}")
    };
    let mut seq = 1u32;
    for path in store::list_challenge_paths(ctx)? {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if let Some(rest) = stem.strip_prefix(&format!("{prefix}-")) {
            if let Ok(n) = rest.parse::<u32>() {
                seq = seq.max(n + 1);
            }
        }
    }
    let file_name = format!("{prefix}-{seq:02}.json");
    let challenge_id = if scope == "sequence" {
        format!("CH-ROADMAP-{seq:02}")
    } else {
        format!("CH-{milestone_id}-{seq:02}")
    };
    Ok((ctx.challenges_dir().join(file_name), challenge_id))
}

fn find_open_challenge(ctx: &PlanContext, milestone_id: &str) -> Result<Option<ChallengeFile>> {
    for path in store::list_challenge_paths(ctx)? {
        let file = store::load_challenge(&path)?;
        if file.challenge.status == "open" && file.challenge.milestone_id == milestone_id {
            return Ok(Some(file));
        }
    }
    Ok(None)
}

fn open_challenge_for_milestone(ctx: &PlanContext, milestone_id: &str) -> Result<ChallengeFile> {
    let mid = paths::normalize_milestone_id(milestone_id);
    find_open_challenge(ctx, &mid)?
        .context("no open challenge for milestone; run challenge start first")
}

fn persist_challenge(ctx: &PlanContext, challenge: &ChallengeFile) -> Result<()> {
    let path = challenge_file_path(ctx, &challenge.challenge.id)?;
    store::write_challenge(ctx, &path, challenge)
}

fn challenge_file_path(ctx: &PlanContext, challenge_id: &str) -> Result<std::path::PathBuf> {
    for path in store::list_challenge_paths(ctx)? {
        let file = store::load_challenge(&path)?;
        if file.challenge.id == challenge_id {
            return Ok(path);
        }
    }
    bail!("challenge file not found for {challenge_id}")
}

pub fn challenge_template() -> Result<String> {
    assets::read_embedded("templates/defaults/challenge.json")
}
