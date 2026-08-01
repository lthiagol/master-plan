use anyhow::Result;
use serde_json::json;

use crate::challenge;
use crate::cli::{ChallengeCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::paths::PlanContext;

pub(crate) fn cmd_challenge(ctx: &PlanContext, cmd: ChallengeCmd, format: Fmt) -> Result<()> {
    match cmd {
        ChallengeCmd::Start { id, scope } => {
            let file = challenge::challenge_start(ctx, id.as_deref(), &scope)?;
            emit(format, &json!({ "ok": true, "challenge": file.challenge }))
        }
        ChallengeCmd::Audit { id, scope } => {
            let file = challenge::challenge_audit(ctx, &id, scope.as_deref())?;
            emit(
                format,
                &json!({ "ok": true, "challenge": file.challenge, "findings": file.findings }),
            )
        }
        ChallengeCmd::List { id, status } => {
            let report = challenge::challenge_list(ctx, id.as_deref(), status.as_deref())?;
            emit(format, &report)
        }
        ChallengeCmd::Add {
            id,
            title,
            severity,
            category,
            target,
            description,
        } => {
            let finding = challenge::challenge_add(
                ctx,
                &id,
                &title,
                &severity,
                &category,
                target.as_deref(),
                description.as_deref(),
            )?;
            emit(format, &json!({ "ok": true, "finding": finding }))
        }
        ChallengeCmd::Resolve {
            id,
            finding_id,
            action,
            payload,
            resolution,
            dry_run,
        } => {
            let finding = challenge::challenge_resolve(
                ctx,
                &id,
                &finding_id,
                &action,
                payload.as_deref(),
                resolution.as_deref(),
                dry_run,
            )?;
            emit(format, &json!({ "ok": true, "finding": finding }))
        }
        ChallengeCmd::Dismiss {
            id,
            finding_id,
            reason,
        } => {
            let finding = challenge::challenge_dismiss(ctx, &id, &finding_id, &reason)?;
            emit(format, &json!({ "ok": true, "finding": finding }))
        }
        ChallengeCmd::Done { id } => {
            let file = challenge::challenge_done(ctx, &id)?;
            emit(format, &json!({ "ok": true, "challenge": file.challenge }))
        }
    }
}
