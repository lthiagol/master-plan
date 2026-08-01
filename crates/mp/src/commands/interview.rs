use anyhow::Result;
use serde_json::json;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::interview;
use crate::paths::{self, PlanContext};
use crate::store;

pub(crate) fn cmd_interview_checklist(
    ctx: &PlanContext,
    checklist_type: &str,
    id: Option<&str>,
    kind: Option<&str>,
    draft: bool,
    format: Fmt,
) -> Result<()> {
    let result = interview::interview_checklist(ctx, checklist_type, id, kind, draft)?;
    emit(format, &result)
}

pub(crate) fn cmd_interview_gaps(
    ctx: &PlanContext,
    id: Option<&str>,
    kind: Option<&str>,
    format: Fmt,
) -> Result<()> {
    if let Some(kind) = kind {
        let track = store::load_track(ctx, kind)?;
        let item = id.and_then(|item_id| track.items.iter().find(|i| i.id == item_id));
        let missing = interview::interview_gaps_track_item(item);
        return emit(format, &json!({ "missing": missing }));
    }
    if let Some(mid) = id {
        let norm = paths::normalize_milestone_id(mid);
        if let Some(p) = paths::find_milestone_file(&ctx.milestones_dir(), &norm) {
            let m = store::load_milestone(&p)?;
            let mut missing = Vec::new();
            if m.intent.outcome.is_empty() {
                missing.push("intent.outcome");
            }
            if m.problem.description.is_empty() {
                missing.push("problem.description");
            }
            return emit(format, &json!({ "missing": missing }));
        }
    }
    let plan = store::load_plan(ctx)?;
    let missing = interview::interview_gaps_plan(&plan);
    emit(format, &json!({ "missing": missing }))
}
