use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::cli::{OutputFormat as Fmt, TrackCmd};
use crate::commands::archive as cmd_archive_mod;
use crate::commands::common::{emit, emit_value};
use crate::milestone::{self, CreateAcceptanceCriterion, CreateMilestoneInput};
use crate::model::{ArchiveEntry, Intent, Problem, Scope, TrackItem};
use crate::paths::PlanContext;
use crate::store;
use crate::track_kind;

// `fields` is threaded for read subcommands (List, Show). Write subcommands
// (Add, Done, etc.) use `emit` which ignores it — that's intentional.
pub(crate) fn cmd_track(
    ctx: &PlanContext,
    cmd: TrackCmd,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    if matches!(&cmd, TrackCmd::List { .. } | TrackCmd::Show { .. }) {
        return cmd_track_inner(ctx, cmd, format, fields);
    }
    let recoverable = matches!(
        &cmd,
        TrackCmd::Cancel { .. }
            | TrackCmd::Promote { .. }
            | TrackCmd::Archive(_)
            | TrackCmd::Restore(_)
            | TrackCmd::Purge(_)
    );
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    if recoverable {
        txn.run_recoverable(|_| cmd_track_inner(ctx, cmd, format, fields))
    } else {
        txn.run(|_| cmd_track_inner(ctx, cmd, format, fields))
    }
}

fn cmd_track_inner(ctx: &PlanContext, cmd: TrackCmd, format: Fmt, fields: &[String]) -> Result<()> {
    match cmd {
        TrackCmd::List { items } => cmd_track_list(ctx, format, fields, items),
        TrackCmd::Show { kind } => {
            let track = store::load_track(ctx, &kind)?;
            match format {
                Fmt::Raw => {
                    println!("{}", std::fs::read_to_string(ctx.track_path(&kind))?);
                    Ok(())
                }
                _ => {
                    let value = serde_json::to_value(&track).unwrap_or(json!({}));
                    emit_value(Fmt::Json, &value, fields)
                }
            }
        }
        TrackCmd::Add {
            kind,
            title,
            problem,
            verification,
            done_when,
            step,
        } => {
            let path = ctx.track_path(&kind);
            let mut track = store::load_track(ctx, &kind)?;
            let id = store::next_track_item_id(&track, &kind)?;
            let item = TrackItem {
                id: id.clone(),
                title: title.unwrap_or_else(|| "Untitled".to_string()),
                status: "pending".to_string(),
                effort: "S".to_string(),
                problem: problem.unwrap_or_default(),
                done_when: done_when.unwrap_or_default(),
                verification: verification.unwrap_or_default(),
                steps: step,
                evidence: String::new(),
                created: store::today(),
                completed: String::new(),
                archived_at: String::new(),
            };
            track.items.push(item.clone());
            store::write_track(ctx, &path, &track)?;
            emit(format, &json!({ "ok": true, "kind": kind, "item": item }))
        }
        TrackCmd::Start { kind, id } => {
            let path = ctx.track_path(&kind);
            let mut track = store::load_track(ctx, &kind)?;
            let item = track
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .with_context(|| format!("item {id} not found"))?;
            item.status = "in-progress".to_string();
            store::write_track(ctx, &path, &track)?;
            emit(
                format,
                &json!({ "ok": true, "id": id, "status": "in-progress" }),
            )
        }
        TrackCmd::Done { kind, id, evidence } => {
            let path = ctx.track_path(&kind);
            let mut track = store::load_track(ctx, &kind)?;
            let item = track
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .with_context(|| format!("item {id} not found"))?;
            item.status = "done".to_string();
            item.completed = store::today();
            if let Some(e) = evidence {
                item.evidence = e;
            }
            store::write_track(ctx, &path, &track)?;
            emit(format, &json!({ "ok": true, "id": id, "status": "done" }))
        }
        TrackCmd::Cancel { kind, id } => {
            let path = ctx.track_path(&kind);
            let cfg = store::load_config(ctx);
            let mut track = store::load_track(ctx, &kind)?;
            if let Some(existing) = track.items.iter().find(|item| item.id == id) {
                if matches!(existing.status.as_str(), "archived" | "cancelled") {
                    return emit(
                        format,
                        &json!({
                            "ok": true,
                            "id": id,
                            "status": existing.status,
                            "idempotent": true,
                        }),
                    );
                }
            }
            let status = {
                let item = track
                    .items
                    .iter_mut()
                    .find(|i| i.id == id)
                    .with_context(|| format!("item {id} not found"))?;
                if cfg.archive_on_track_cancel() {
                    item.status = "archived".to_string();
                    item.archived_at = store::now_rfc3339();
                    item.status.clone()
                } else {
                    item.status = "cancelled".to_string();
                    item.status.clone()
                }
            };
            if cfg.archive_on_track_cancel() {
                let archived_at = track
                    .items
                    .iter()
                    .find(|i| i.id == id)
                    .map(|i| i.archived_at.clone())
                    .unwrap_or_default();
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
            }
            store::write_track(ctx, &path, &track)?;
            emit(format, &json!({ "ok": true, "id": id, "status": status }))
        }
        TrackCmd::Promote {
            kind,
            id,
            to_milestone,
        } => {
            if !to_milestone {
                bail!("specify --to-milestone");
            }
            let payload = track_promote(ctx, &kind, &id)?;
            emit(format, &payload)
        }
        TrackCmd::Archive(cmd) => cmd_archive_mod::cmd_archive(ctx, cmd, format),
        TrackCmd::Restore(cmd) => cmd_archive_mod::cmd_restore(ctx, cmd, format),
        TrackCmd::Purge(cmd) => cmd_archive_mod::cmd_purge(ctx, cmd, format),
    }
}

pub(crate) fn cmd_track_list(
    ctx: &PlanContext,
    format: Fmt,
    fields: &[String],
    include_items: bool,
) -> Result<()> {
    let mut tracks = Vec::new();
    for &tk in &track_kind::TrackKind::ALL {
        let kind = tk.as_str();
        if let Ok(t) = store::load_track(ctx, kind) {
            let pending = t.items.iter().filter(|i| i.status == "pending").count();
            let active = t
                .items
                .iter()
                .filter(|i| i.status != "archived" && i.status != "done")
                .count();
            let total = t.items.iter().filter(|i| i.status != "archived").count();
            let mut entry = json!({
                "kind": kind,
                "title": t.track.title,
                "pending": pending,
                "active": active,
                "total": total,
            });
            if include_items {
                if let serde_json::Value::Object(ref mut map) = entry {
                    let items: Vec<serde_json::Value> = t
                        .items
                        .iter()
                        .filter(|i| i.status != "archived")
                        .map(|i| {
                            json!({
                                "id": i.id,
                                "title": i.title,
                                "status": i.status,
                            })
                        })
                        .collect();
                    map.insert("items".to_string(), json!(items));
                }
            }
            tracks.push(entry);
        }
    }
    let value = json!({ "tracks": tracks });
    emit_value(format, &value, fields)
}

pub fn track_promote(ctx: &PlanContext, kind: &str, id: &str) -> Result<serde_json::Value> {
    if kind.parse::<track_kind::TrackKind>().is_err() {
        bail!("track kind must be bugfix or tweak");
    }

    let path = ctx.track_path(kind);
    let mut track = store::load_track(ctx, kind)?;
    let item = track
        .items
        .iter()
        .find(|i| i.id == id)
        .with_context(|| format!("track item {id} not found in {kind}"))?
        .clone();

    if item.status == "archived" {
        let provenance = format!("Promoted from {kind} track {id}");
        if let Some((_, milestone)) = store::load_all_milestones(ctx)?
            .into_iter()
            .find(|(_, milestone)| milestone.scope.in_scope.iter().any(|s| s == &provenance))
        {
            return Ok(json!({
                "ok": true,
                "kind": kind,
                "track_id": id,
                "promoted_to": format!("milestone:{}", milestone.milestone.id),
                "idempotent": true,
            }));
        }
        bail!("track item {id} already archived");
    }

    let mut acceptance_criteria = Vec::new();
    if !item.done_when.is_empty() {
        acceptance_criteria.push(CreateAcceptanceCriterion {
            description: item.done_when.clone(),
            verification: item.verification.clone(),
            ..Default::default()
        });
    } else if !item.verification.is_empty() {
        acceptance_criteria.push(CreateAcceptanceCriterion {
            description: "Track verification".to_string(),
            verification: item.verification.clone(),
            ..Default::default()
        });
    }

    let outcome = if item.done_when.is_empty() {
        item.title.clone()
    } else {
        item.done_when.clone()
    };

    let m = milestone::create_milestone(
        ctx,
        CreateMilestoneInput {
            title: Some(item.title.clone()),
            intent: Intent { outcome },
            problem: Problem {
                description: item.problem.clone(),
            },
            scope: Scope {
                in_scope: vec![format!("Promoted from {kind} track {id}")],
                out_of_scope: vec!["Out of track scope".to_string(), "TBD".to_string()],
            },
            effort: if item.effort.is_empty() {
                "S".to_string()
            } else {
                item.effort.clone()
            },
            acceptance_criteria,
            ..Default::default()
        },
    )?;

    let promoted_to = format!("milestone:{}", m.milestone.id);
    let item = track
        .items
        .iter_mut()
        .find(|i| i.id == id)
        .context("track item not found")?;
    item.status = "archived".to_string();
    item.archived_at = store::now_rfc3339();
    store::write_track(ctx, &path, &track)?;

    Ok(json!({
        "ok": true,
        "kind": kind,
        "track_id": id,
        "promoted_to": promoted_to,
    }))
}
