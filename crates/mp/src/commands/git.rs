use anyhow::Result;

use crate::cli::{GitCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::git;
use crate::paths::PlanContext;

pub(crate) fn cmd_git(ctx: &PlanContext, cmd: GitCmd, format: Fmt) -> Result<()> {
    match cmd {
        GitCmd::Status => {
            let report = git::git_status(ctx)?;
            emit(format, &report)?;
        }
        GitCmd::SuggestMessage => {
            let report = git::git_suggest_message(ctx)?;
            emit(format, &report)?;
        }
        GitCmd::Commit { message } => {
            let report = git::git_commit(ctx, message.as_deref())?;
            emit(format, &report)?;
        }
    }
    Ok(())
}
