use anyhow::Result;

use crate::cli::{NoteCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::note;
use crate::paths::PlanContext;

pub(crate) fn cmd_note(ctx: &PlanContext, cmd: NoteCmd, format: Fmt) -> Result<()> {
    match cmd {
        NoteCmd::Add {
            title,
            body,
            body_file,
            to,
        } => {
            let report = note::note_add(
                ctx,
                &title,
                body.as_deref(),
                body_file.as_deref(),
                to.as_deref(),
            )?;
            emit(format, &report)
        }
    }
}
