use anyhow::Result;
use serde_json::json;

use crate::cli::{OutputFormat as Fmt, WpCmd};
use crate::commands::common::emit;
use crate::paths::PlanContext;
use crate::wp;

pub(crate) fn cmd_wp(ctx: &PlanContext, cmd: WpCmd, format: Fmt) -> Result<()> {
    match cmd {
        WpCmd::Add {
            milestone,
            name,
            goal,
            rollback,
            id,
        } => {
            let wp = wp::add_work_package(
                ctx,
                &milestone,
                wp::AddWpInput {
                    id,
                    name,
                    goal: goal.unwrap_or_default(),
                    rollback: rollback.unwrap_or_default(),
                },
            )?;
            emit(format, &json!({ "ok": true, "work_package": wp }))
        }
        WpCmd::Update {
            milestone,
            wp: wp_id,
            name,
            goal,
            rollback,
        } => {
            let wp = wp::wp_update(ctx, &milestone, &wp_id, name, goal, rollback)?;
            emit(format, &json!({ "ok": true, "work_package": wp }))
        }
        WpCmd::Remove { milestone, wp } => {
            let result = wp::remove_work_package(ctx, &milestone, &wp)?;
            emit(format, &result)
        }
    }
}
