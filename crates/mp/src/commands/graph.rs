use anyhow::Result;

use crate::cli::{GraphCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::graph;
use crate::paths::PlanContext;

pub(crate) fn cmd_graph(
    ctx: &PlanContext,
    milestone: Option<&str>,
    with_steps: bool,
    with_ac: bool,
    cmd: Option<GraphCmd>,
    format: Fmt,
) -> Result<()> {
    match cmd {
        Some(GraphCmd::Explain { milestone }) => {
            let report = graph::graph_explain(ctx, &milestone)?;
            emit(format, &report)
        }
        None => {
            let report = graph::build_graph(ctx, milestone, with_steps, with_ac)?;
            // --format raw on graph emits GraphViz DOT (debug export).
            if matches!(format, Fmt::Raw) {
                println!("{}", graph::graph_dot(&report));
                return Ok(());
            }
            emit(format, &report)
        }
    }
}
