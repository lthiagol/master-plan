use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::cli::{ArchiveCmd, OutputFormat as Fmt, PurgeCmd, RestoreCmd};
use crate::commands::common::{emit, emit_value};
use crate::model::ArchiveEntry;
use crate::paths::PlanContext;
use crate::store;

pub(crate) fn cmd_archive(ctx: &PlanContext, cmd: ArchiveCmd, format: Fmt) -> Result<()> {
    match cmd {
        ArchiveCmd::Milestone { id } => {
            store::archive_milestone(ctx, &id)?;
            emit(
                format,
                &json!({ "ok": true, "archived": "milestone", "id": id }),
            )
        }
        ArchiveCmd::TrackItem { kind, id } => {
            let path = ctx.track_path(&kind);
            let mut track = store::load_track(ctx, &kind)?;
            let item = track
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .with_context(|| format!("item {id} not found"))?;
            item.status = "archived".to_string();
            item.archived_at = store::now_rfc3339();
            let archived_at = item.archived_at.clone();
            store::write_track(ctx, &path, &track)?;
            store::append_archive_meta(
                ctx,
                ArchiveEntry {
                    entity_type: "track-item".to_string(),
                    entity_id: id.clone(),
                    original_path: format!("tracks/{kind}.json"),
                    archived_path: format!("tracks/{kind}.json#{id}"),
                    archived_at,
                },
            )?;
            emit(
                format,
                &json!({ "ok": true, "archived": "track-item", "kind": kind, "id": id }),
            )
        }
    }
}

pub(crate) fn cmd_list_archived(
    ctx: &PlanContext,
    entity_type: Option<&str>,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    let meta = store::load_archive_meta(ctx)?;
    let mut items: Vec<&ArchiveEntry> = meta.entries.iter().collect();
    if let Some(t) = entity_type {
        items.retain(|e| e.entity_type == t);
    }
    let value = json!({ "archived": items });
    emit_value(format, &value, fields)
}

pub(crate) fn cmd_restore(ctx: &PlanContext, cmd: RestoreCmd, format: Fmt) -> Result<()> {
    match cmd {
        RestoreCmd::Archived {
            entity_type,
            id,
            kind,
        } => {
            if entity_type == "milestone" {
                store::restore_archived_milestone(ctx, &id)?;
            } else if entity_type == "track-item" {
                let kind = kind.context("--kind required")?;
                let path = ctx.track_path(&kind);
                let mut track = store::load_track(ctx, &kind)?;
                let item = track
                    .items
                    .iter_mut()
                    .find(|i| i.id == id)
                    .context("track item not found")?;
                item.status = "pending".to_string();
                item.archived_at = String::new();
                store::write_track(ctx, &path, &track)?;
                store::remove_archive_meta_entry(ctx, "track-item", &id)?;
            } else {
                bail!("unknown type {entity_type}");
            }
            emit(
                format,
                &json!({ "ok": true, "restored": entity_type, "id": id }),
            )
        }
    }
}

pub(crate) fn cmd_purge(ctx: &PlanContext, cmd: PurgeCmd, format: Fmt) -> Result<()> {
    match cmd {
        PurgeCmd::Archived {
            entity_type,
            id,
            older_than: _,
            confirm,
        } => {
            if !confirm {
                bail!("purge requires --confirm");
            }
            if let (Some(t), Some(i)) = (entity_type.as_deref(), id.as_deref()) {
                if t == "milestone" {
                    // Match milestone/io::purge_archived_milestone: meta then file.
                    store::remove_archive_meta_entry(
                        ctx,
                        "milestone",
                        &crate::paths::normalize_milestone_id(i),
                    )?;
                    store::purge_archived_milestone(ctx, i)?;
                } else {
                    bail!("purge for type {t} not implemented");
                }
            } else {
                bail!("purge requires --type and id");
            }
            emit(format, &json!({ "ok": true, "purged": true }))
        }
    }
}
