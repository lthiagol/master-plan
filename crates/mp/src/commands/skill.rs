use anyhow::Result;

use crate::cli::{OutputFormat as Fmt, SkillCmd};
use crate::commands::common::emit;
use crate::paths::PlanContext;
use crate::skill;

pub(crate) fn cmd_skill(ctx: &PlanContext, cmd: SkillCmd, format: Fmt) -> Result<()> {
    match cmd {
        SkillCmd::Context => {
            let report = skill::skill_context(ctx)?;
            emit(format, &report)
        }
    }
}
