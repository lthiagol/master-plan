use anyhow::Result;

use crate::cli::{InterviewCmd, OutputFormat};
use crate::commands::interview;
use crate::paths::PlanContext;

pub(super) fn run(ctx: &PlanContext, cmd: InterviewCmd, format: OutputFormat) -> Result<()> {
    match cmd {
        InterviewCmd::Checklist {
            checklist_type,
            id,
            kind,
            draft,
        } => {
            ctx.ensure_plan_exists()?;
            if std::env::args().any(|arg| arg == "--type") {
                eprintln!("mp: warning: --type is deprecated, use --checklist-type instead");
            }
            interview::cmd_interview_checklist(
                ctx,
                &checklist_type,
                id.as_deref(),
                kind.as_deref(),
                draft,
                format,
            )
        }
        InterviewCmd::Gaps { id, kind } => {
            ctx.ensure_plan_exists()?;
            interview::cmd_interview_gaps(ctx, id.as_deref(), kind.as_deref(), format)
        }
    }
}
