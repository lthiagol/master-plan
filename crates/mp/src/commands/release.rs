use anyhow::Result;
use serde_json::json;

use crate::cli::{OutputFormat as Fmt, ReleaseCmd};
use crate::commands::common::emit;
use crate::paths::PlanContext;
use crate::store;

pub(crate) fn cmd_release(ctx: &PlanContext, cmd: ReleaseCmd, format: Fmt) -> Result<()> {
    if !matches!(&cmd, ReleaseCmd::Ship { .. }) {
        return cmd_release_inner(ctx, cmd, format);
    }
    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
    txn.run(|_| cmd_release_inner(ctx, cmd, format))
}

fn cmd_release_inner(ctx: &PlanContext, cmd: ReleaseCmd, format: Fmt) -> Result<()> {
    match cmd {
        ReleaseCmd::List => {
            let plan = store::load_plan(ctx)?;
            let versions: Vec<serde_json::Value> = plan
                .releases
                .iter()
                .map(|r| {
                    json!({
                        "version": r.version,
                        "status": r.status,
                        "date": r.date,
                        "milestones": r.milestones,
                    })
                })
                .collect();
            emit(format, &json!({ "ok": true, "releases": versions }))
        }
        ReleaseCmd::Map => {
            let plan = store::load_plan(ctx)?;
            let mut planned = Vec::new();
            let mut shipped = Vec::new();
            for r in &plan.releases {
                let entry = json!({
                    "version": r.version,
                    "date": r.date,
                    "milestones": r.milestones,
                });
                match r.status.as_str() {
                    "shipped" => shipped.push(entry),
                    _ => planned.push(entry),
                }
            }
            emit(
                format,
                &json!({ "ok": true, "planned": planned, "shipped": shipped }),
            )
        }
        ReleaseCmd::Show { version } => {
            let plan = store::load_plan(ctx)?;
            let release = plan
                .releases
                .iter()
                .find(|r| r.version == version)
                .ok_or_else(|| anyhow::anyhow!("release {version} not found"))?;
            emit(
                format,
                &json!({
                    "ok": true,
                    "release": {
                        "version": release.version,
                        "status": release.status,
                        "date": release.date,
                        "milestones": release.milestones,
                    }
                }),
            )
        }
        ReleaseCmd::Ship { version, force } => {
            let mut plan = store::load_plan(ctx)?;
            let release_idx = plan
                .releases
                .iter()
                .position(|r| r.version == version)
                .ok_or_else(|| anyhow::anyhow!("release {version} not found"))?;

            let milestone_ids = plan.releases[release_idx].milestones.clone();

            if !force {
                let all_milestones = store::load_all_milestones(ctx)?;
                for m_id in &milestone_ids {
                    let milestone = all_milestones
                        .iter()
                        .find(|(_, m)| m.milestone.id == *m_id)
                        .map(|(_, m)| m);
                    if let Some(m) = milestone {
                        if m.milestone.execution_status != "done" {
                            anyhow::bail!(
                                "milestone {m_id} is not done (status: {}); use --force to override",
                                m.milestone.execution_status
                            );
                        }
                    }
                }
            }

            let today = store::today();
            plan.releases[release_idx].status = "shipped".to_string();
            plan.releases[release_idx].date = today.clone();
            store::write_plan(ctx, &plan)?;

            emit(
                format,
                &json!({
                    "ok": true,
                    "version": version,
                    "status": "shipped",
                    "date": today,
                }),
            )
        }
    }
}
