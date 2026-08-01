use anyhow::{bail, Result};

use crate::model::DecisionEntry;
use crate::paths::PlanContext;
use crate::store;

pub fn decision_add(
    ctx: &PlanContext,
    summary: &str,
    context_text: Option<&str>,
    milestone: Option<&str>,
) -> Result<DecisionEntry> {
    if summary.is_empty() {
        bail!("--summary is required");
    }
    let mut decisions = store::load_decisions(ctx)?;
    let id = store::next_decision_id(&decisions);
    let entry = DecisionEntry {
        id: id.clone(),
        date: store::today(),
        summary: summary.to_string(),
        context: context_text.unwrap_or("").to_string(),
        milestone: milestone.unwrap_or("").to_string(),
    };
    decisions.decisions.push(entry.clone());
    store::write_decisions(ctx, &decisions)?;
    Ok(entry)
}

pub fn decision_list(ctx: &PlanContext) -> Result<Vec<DecisionEntry>> {
    Ok(store::load_decisions(ctx)?.decisions)
}

pub fn decision_remove(ctx: &PlanContext, id: &str) -> Result<()> {
    let mut decisions = store::load_decisions(ctx)?;
    let len_before = decisions.decisions.len();
    decisions.decisions.retain(|d| d.id != id);
    if decisions.decisions.len() == len_before {
        bail!("decision {id} not found");
    }
    store::write_decisions(ctx, &decisions)?;
    Ok(())
}
