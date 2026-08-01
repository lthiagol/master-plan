use anyhow::Result;
use serde_json::json;

use crate::annotation;
use crate::cli::{AnnotationCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::paths::PlanContext;

pub(crate) fn cmd_annotation(ctx: &PlanContext, cmd: AnnotationCmd, format: Fmt) -> Result<()> {
    if matches!(
        &cmd,
        AnnotationCmd::List { .. } | AnnotationCmd::Show { .. }
    ) {
        return cmd_annotation_inner(ctx, cmd, format);
    }
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    txn.run(|_| cmd_annotation_inner(ctx, cmd, format))
}

fn cmd_annotation_inner(ctx: &PlanContext, cmd: AnnotationCmd, format: Fmt) -> Result<()> {
    match cmd {
        AnnotationCmd::Create {
            target,
            kind,
            body,
            author,
        } => {
            let item = annotation::annotation_create(ctx, &target, &kind, &body, &author)?;
            emit(format, &json!({ "ok": true, "annotation": item }))
        }
        AnnotationCmd::List {
            open,
            target,
            kind,
            author,
        } => {
            let items = annotation::annotation_list(
                ctx,
                open,
                target.as_deref(),
                kind.as_deref(),
                author.as_deref(),
            )?;
            emit(format, &json!({ "ok": true, "annotations": items }))
        }
        AnnotationCmd::Show { id } => {
            let item = annotation::annotation_show(ctx, &id)?;
            emit(format, &json!({ "ok": true, "annotation": item }))
        }
        AnnotationCmd::Update {
            id,
            body,
            kind,
            author,
        } => {
            let item = annotation::annotation_update(
                ctx,
                &id,
                body.as_deref(),
                kind.as_deref(),
                author.as_deref(),
            )?;
            emit(format, &json!({ "ok": true, "annotation": item }))
        }
        AnnotationCmd::Resolve { id } => {
            let item = annotation::annotation_resolve(ctx, &id)?;
            emit(format, &json!({ "ok": true, "annotation": item }))
        }
        AnnotationCmd::Reopen { id } => {
            let item = annotation::annotation_reopen(ctx, &id)?;
            emit(format, &json!({ "ok": true, "annotation": item }))
        }
        AnnotationCmd::Remove { id } => {
            annotation::annotation_remove(ctx, &id)?;
            emit(format, &json!({ "ok": true, "removed": id }))
        }
        AnnotationCmd::Addressed { id } => {
            let item = annotation::annotation_mark_addressed(ctx, &id)?;
            emit(format, &json!({ "ok": true, "annotation": item }))
        }
    }
}
