use anyhow::Result;
use serde_json::Value;

use crate::assets;
use crate::model::{MilestoneFile, PlanFile, TrackItem};
use crate::paths::PlanContext;
use crate::store;

#[derive(Debug, serde::Serialize)]
pub struct ChecklistResult {
    pub r#type: String,
    pub milestone_id: Option<String>,
    pub track_kind: Option<String>,
    pub missing: Vec<String>,
    pub suggested_questions: Vec<String>,
    pub ready_for_review: bool,
}

pub fn interview_checklist(
    ctx: &PlanContext,
    checklist_type: &str,
    milestone_id: Option<&str>,
    track_kind: Option<&str>,
    draft: bool,
) -> Result<ChecklistResult> {
    if draft {
        return draft_checklist(checklist_type);
    }
    let raw = assets::read_embedded("schemas/interview-checklist.json")?;
    let doc: Value = serde_json::from_str(&raw)?;

    match checklist_type {
        "charter" => charter_checklist(&doc),
        "milestone" => {
            let id = milestone_id.unwrap_or("");
            if id.is_empty() {
                return Ok(ChecklistResult {
                    r#type: "milestone".to_string(),
                    milestone_id: None,
                    track_kind: None,
                    missing: vec!["milestone id required (--id)".to_string()],
                    suggested_questions: vec![],
                    ready_for_review: false,
                });
            }
            milestone_checklist(ctx, &doc, id)
        }
        "track-item" => {
            let kind = track_kind.unwrap_or("bugfix");
            track_item_checklist(ctx, &doc, kind)
        }
        "brief" => brief_checklist(ctx),
        "implementation-plan" => impl_plan_checklist(ctx, milestone_id),
        _ => anyhow::bail!("unknown checklist type: {checklist_type}"),
    }
}

fn charter_checklist(doc: &Value) -> Result<ChecklistResult> {
    let mut missing = Vec::new();
    let mut questions = Vec::new();
    if let Some(charter) = doc.get("charter") {
        if let Some(rounds) = charter.get("rounds").and_then(|r| r.as_array()) {
            for round in rounds {
                if let Some(qs) = round.get("questions").and_then(|q| q.as_array()) {
                    for q in qs {
                        questions.push(q.as_str().unwrap_or("").to_string());
                    }
                }
            }
        }
        for field in [
            "project.name",
            "project.description",
            "charter.goals",
            "charter.non_goals",
        ] {
            missing.push(field.to_string());
        }
    }
    Ok(ChecklistResult {
        r#type: "charter".to_string(),
        milestone_id: None,
        track_kind: None,
        missing,
        suggested_questions: questions,
        ready_for_review: false,
    })
}

fn milestone_checklist(ctx: &PlanContext, doc: &Value, id: &str) -> Result<ChecklistResult> {
    let norm = crate::paths::normalize_milestone_id(id);
    let path = crate::paths::find_milestone_file(&ctx.milestones_dir(), &norm);
    let mut missing = Vec::new();
    let mut questions = Vec::new();

    if let Some(p) = path {
        let m: MilestoneFile = store::load_milestone(&p)?;
        if m.intent.outcome.is_empty() {
            missing.push("intent.outcome".to_string());
        }
        if m.problem.description.is_empty() {
            missing.push("problem.description".to_string());
        }
        if m.scope.in_scope.is_empty() {
            missing.push("scope.in_scope".to_string());
        }
        if m.scope.out_of_scope.len() < 2 {
            missing.push("scope.out_of_scope".to_string());
        }
        if m.acceptance_criteria.is_empty() {
            missing.push("acceptance_criteria".to_string());
        }
    } else {
        missing.push(format!("milestones/{norm}.json"));
    }

    if let Some(ms) = doc.get("milestone") {
        if let Some(spec) = ms.get("phase_1_spec") {
            if let Some(rounds) = spec.get("rounds").and_then(|r| r.as_array()) {
                for round in rounds {
                    let fields: Vec<&str> = round
                        .get("fields")
                        .and_then(|f| f.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();
                    let round_missing = fields.iter().any(|f| missing.contains(&f.to_string()));
                    if round_missing {
                        if let Some(qs) = round.get("questions").and_then(|q| q.as_array()) {
                            for q in qs {
                                questions.push(q.as_str().unwrap_or("").to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let ready = missing.is_empty();
    Ok(ChecklistResult {
        r#type: "milestone".to_string(),
        milestone_id: Some(norm),
        track_kind: None,
        missing,
        suggested_questions: questions,
        ready_for_review: ready,
    })
}

fn track_item_checklist(ctx: &PlanContext, doc: &Value, kind: &str) -> Result<ChecklistResult> {
    let mut questions = Vec::new();
    if let Some(ti) = doc.get("track_item") {
        if let Some(rounds) = ti.get("rounds").and_then(|r| r.as_array()) {
            for round in rounds {
                if let Some(qs) = round.get("questions").and_then(|q| q.as_array()) {
                    for q in qs {
                        questions.push(q.as_str().unwrap_or("").to_string());
                    }
                }
            }
        }
    }
    let _ = store::load_track(ctx, kind)?;
    Ok(ChecklistResult {
        r#type: "track-item".to_string(),
        milestone_id: None,
        track_kind: Some(kind.to_string()),
        missing: vec![],
        suggested_questions: questions,
        ready_for_review: true,
    })
}

pub fn interview_gaps_plan(plan: &PlanFile) -> Vec<String> {
    let mut missing = Vec::new();
    if plan.project.name.is_empty() {
        missing.push("project.name".to_string());
    }
    if plan.project.description.is_empty() {
        missing.push("project.description".to_string());
    }
    if plan.charter.goals.is_empty() {
        missing.push("charter.goals".to_string());
    }
    if plan.charter.non_goals.is_empty() {
        missing.push("charter.non_goals".to_string());
    }
    missing
}

pub fn interview_gaps_track_item(item: Option<&TrackItem>) -> Vec<String> {
    let Some(item) = item else {
        return vec![
            "title".to_string(),
            "problem".to_string(),
            "verification".to_string(),
            "steps".to_string(),
        ];
    };
    let mut missing = Vec::new();
    if item.title.is_empty() {
        missing.push("title".to_string());
    }
    if item.problem.is_empty() {
        missing.push("problem".to_string());
    }
    if item.verification.is_empty() && item.done_when.is_empty() {
        missing.push("verification".to_string());
    }
    if item.steps.is_empty() {
        missing.push("steps".to_string());
    }
    missing
}

fn brief_checklist(_ctx: &PlanContext) -> Result<ChecklistResult> {
    let missing = vec![
        "project.name".to_string(),
        "project.description".to_string(),
        "project.stack".to_string(),
        "target users".to_string(),
        "key features".to_string(),
        "constraints".to_string(),
        "success metrics".to_string(),
    ];
    let suggested_questions = vec![
        "What is the project name and one-sentence description?".to_string(),
        "What tech stack are you using (languages, frameworks)?".to_string(),
        "Who are the primary users and what problem do you solve for them?".to_string(),
        "What are the 3 most important features for v1?".to_string(),
        "Are there any hard constraints (deadline, budget, compliance)?".to_string(),
        "How will you measure success (users, revenue, performance)?".to_string(),
    ];
    Ok(ChecklistResult {
        r#type: "brief".to_string(),
        milestone_id: None,
        track_kind: None,
        missing,
        suggested_questions,
        ready_for_review: false,
    })
}

fn impl_plan_checklist(ctx: &PlanContext, milestone_id: Option<&str>) -> Result<ChecklistResult> {
    let mut missing = Vec::new();
    if let Some(id) = milestone_id {
        if let Ok(m) = crate::milestone::load_milestone_by_id(ctx, id) {
            if m.work_packages.is_empty() {
                missing.push("work_packages".to_string());
            }
            if m.steps.is_empty() {
                missing.push("steps".to_string());
            }
            if m.milestone.depends_on.is_empty() {
                missing.push("depends_on".to_string());
            }
            let pending_acs: Vec<String> = m
                .acceptance_criteria
                .iter()
                .filter(|ac| ac.status != "passed")
                .map(|ac| format!("AC {} pending", ac.id))
                .collect();
            missing.extend(pending_acs);
        }
    } else {
        missing.push("milestone id required (--id)".to_string());
    }
    let suggested_questions = vec![
        "What are the work packages? How many? (--work-packages N)".to_string(),
        "What files will each step touch?".to_string(),
        "What tests verify each step?".to_string(),
        "Are all acceptance criteria covered by steps?".to_string(),
        "What is the rollback plan for each work package?".to_string(),
    ];
    let ready = missing.is_empty();
    Ok(ChecklistResult {
        r#type: "implementation-plan".to_string(),
        milestone_id: milestone_id.map(|s| s.to_string()),
        track_kind: None,
        missing,
        suggested_questions,
        ready_for_review: ready,
    })
}

fn draft_checklist(checklist_type: &str) -> Result<ChecklistResult> {
    let questions = match checklist_type {
        "milestone" => vec![
            "What is the outcome? What problem does this solve?".to_string(),
            "What is in scope and out of scope (minimum 2 exclusions)?".to_string(),
            "What acceptance criteria verify this is done?".to_string(),
            "Any design decisions or trade-offs to lock before coding?".to_string(),
            "Any open questions that must be resolved before approval?".to_string(),
        ],
        "brief" => vec![
            "What is the project name and one-sentence description?".to_string(),
            "What tech stack are you using (languages, frameworks)?".to_string(),
            "Who are the primary users and what problem do you solve?".to_string(),
            "What are the most important features for v1?".to_string(),
            "Are there any hard constraints (deadline, budget, compliance)?".to_string(),
            "How will you measure success?".to_string(),
        ],
        "charter" => vec![
            "What are the project goals?".to_string(),
            "What is explicitly out of scope (non-goals)?".to_string(),
            "What principles guide decisions?".to_string(),
        ],
        "track-item" => vec![
            "What is the problem? What's the expected fix?".to_string(),
            "How will you verify the fix works?".to_string(),
            "What steps are needed?".to_string(),
        ],
        "implementation-plan" => vec![
            "What work packages are needed?".to_string(),
            "What files will each step touch?".to_string(),
            "What tests verify each step?".to_string(),
        ],
        _ => vec!["No draft questions available for this type.".to_string()],
    };
    Ok(ChecklistResult {
        r#type: checklist_type.to_string(),
        milestone_id: None,
        track_kind: None,
        missing: vec![],
        suggested_questions: questions,
        ready_for_review: false,
    })
}
