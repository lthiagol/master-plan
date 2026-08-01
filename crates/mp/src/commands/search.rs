use anyhow::Result;
use serde_json::json;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::paths::PlanContext;
use crate::search::{self, VALID_SEARCH_TYPES};

/// Recognized values for `--include`. Any other value is an error so a typo
/// doesn't silently degrade to snippet-only.
const INCLUDE_SNIPPET: &str = "snippet";
const INCLUDE_OBJECT: &str = "object";

/// Recognized values for `--group-by`. Currently only "milestone" is
/// supported; passing any other value errors out.
const GROUP_BY_MILESTONE: &str = "milestone";

pub(crate) fn cmd_search(
    ctx: &PlanContext,
    query: &str,
    filter_type: Option<&str>,
    format: Fmt,
    limit: usize,
    include: &str,
    group_by: Option<&str>,
) -> Result<()> {
    if include != INCLUDE_SNIPPET && include != INCLUDE_OBJECT {
        anyhow::bail!(
            "invalid --include value: {include} (expected: {INCLUDE_SNIPPET} or {INCLUDE_OBJECT})"
        );
    }
    if let Some(g) = group_by {
        if g != GROUP_BY_MILESTONE {
            anyhow::bail!("invalid --group-by value: {g} (supported: {GROUP_BY_MILESTONE})");
        }
    }
    // M2 remediation: the lib's `VALID_SEARCH_TYPES` is the single
    // source of truth for which `--type` values mp search accepts. The
    // CLI dispatch validates against this list so adding a new
    // artifact type to `search.rs` automatically extends the CLI
    // surface without requiring a parallel edit here.
    let filter_type = match filter_type {
        None => None,
        Some("all") => None,
        Some(t) if VALID_SEARCH_TYPES.contains(&t) => Some(t),
        Some(bad) => {
            anyhow::bail!(
                "invalid --type value: {bad} (expected one of: {} or 'all')",
                VALID_SEARCH_TYPES.join(", ")
            );
        }
    };
    let include_object = include == INCLUDE_OBJECT;
    let results = search::search_plan(ctx, query, filter_type, limit, include_object)?;
    if group_by == Some(GROUP_BY_MILESTONE) {
        let grouped = search::group_by_milestone(results);
        emit(format, &grouped)
    } else {
        emit(format, &json!({ "results": results }))
    }
}
