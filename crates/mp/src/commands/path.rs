use anyhow::Result;
use serde_json::json;

use crate::cli::{OutputFormat as Fmt, PathSubCmd};
use crate::commands::common::{emit, emit_fields};
use crate::path_engine;
use crate::path_prefs;
use crate::paths::PlanContext;

pub(crate) struct PathOptions<'a> {
    pub ctx: &'a PlanContext,
    pub horizon: usize,
    pub include_grooming: bool,
    pub prioritize_coverage: bool,
    pub include_coverage_gaps: bool,
    pub cmd: Option<PathSubCmd>,
    pub format: Fmt,
    pub fields: &'a [String],
    pub lane: Option<String>,
    pub all_lanes: bool,
    pub no_ideas: bool,
    pub path_summary: bool,
}

pub(crate) fn cmd_path(opts: PathOptions) -> Result<()> {
    if opts.cmd.is_none() {
        return cmd_path_inner(opts);
    }
    let txn = crate::plan_io::PlanWriteTxn::acquire(&opts.ctx.plan_dir)?;
    txn.run(|_| cmd_path_inner(opts))
}

fn cmd_path_inner(opts: PathOptions) -> Result<()> {
    match opts.cmd {
        None => {
            // M102 AC-06/07: --lane, --all, --summary switch to the multi-lane
            // view; otherwise fall back to the legacy execution-only report.
            if opts.lane.is_some() || opts.all_lanes || opts.path_summary {
                let lane_opts = path_engine::LaneOptions {
                    no_ideas: opts.no_ideas,
                };
                let report = path_engine::build_lanes(opts.ctx, opts.horizon, lane_opts)?;
                if let Some(name) = opts.lane {
                    let only: Vec<_> = report
                        .lanes
                        .into_iter()
                        .filter(|l| l.name == name)
                        .collect();
                    if only.is_empty() {
                        anyhow::bail!("unknown lane: {name} (known: blocked, execution, review, grooming, backlog)");
                    }
                    return emit_fields(opts.format, &only[0], opts.fields);
                }
                if opts.path_summary {
                    return emit_fields(
                        opts.format,
                        &json!({
                            "lanes": report.lanes.iter().map(|l| json!({
                                "name": l.name,
                                "item_count": l.item_count,
                                "head": l.head,
                            })).collect::<Vec<_>>(),
                            "summary": report.summary,
                        }),
                        opts.fields,
                    );
                }
                return emit_fields(opts.format, &report, opts.fields);
            }

            let mut report = path_engine::build_path(opts.ctx, opts.horizon)?;

            if opts.include_grooming {
                let grooming_ms = path_engine::find_grooming_milestones(opts.ctx)?;
                report.actions.extend(grooming_ms);
            }
            if opts.include_coverage_gaps {
                let gaps = path_engine::find_coverage_gaps(opts.ctx)?;
                report.actions.extend(gaps);
            }
            if opts.prioritize_coverage {
                report.sort_by_coverage_priority();
            }
            emit_fields(opts.format, &report, opts.fields)
        }
        Some(PathSubCmd::Pin {
            milestone,
            before,
            rank,
            reason,
        }) => {
            let plan = path_prefs::pin_milestone(
                opts.ctx,
                &milestone,
                before.as_deref(),
                rank,
                reason.as_deref(),
            )?;
            emit(
                opts.format,
                &json!({
                    "ok": true,
                    "adoption_order": plan.execution.adoption_order,
                }),
            )
        }
        Some(PathSubCmd::Unpin { milestone }) => {
            let plan = path_prefs::unpin_milestone(opts.ctx, &milestone)?;
            emit(
                opts.format,
                &json!({
                    "ok": true,
                    "adoption_order": plan.execution.adoption_order,
                }),
            )
        }
        Some(PathSubCmd::ListPins { milestone }) => {
            let pins = path_prefs::list_pins(opts.ctx)?;
            let filtered = match milestone {
                Some(ref m) => {
                    let norm = crate::paths::normalize_milestone_id(m);
                    pins.into_iter()
                        .filter(|p| p.milestone == norm)
                        .collect::<Vec<_>>()
                }
                None => pins,
            };
            emit_fields(opts.format, &json!({ "pins": filtered }), opts.fields)
        }
        Some(PathSubCmd::Focus { milestone, through }) => {
            let plan = path_prefs::focus_milestone(opts.ctx, &milestone, through.as_deref())?;
            emit(
                opts.format,
                &json!({
                    "ok": true,
                    "focus_milestone": plan.execution.focus_milestone,
                    "focus_through_step": plan.execution.focus_through_step,
                }),
            )
        }
        Some(PathSubCmd::ClearFocus) => {
            let plan = path_prefs::clear_focus(opts.ctx)?;
            emit(
                opts.format,
                &json!({
                    "ok": true,
                    "focus_milestone": plan.execution.focus_milestone,
                }),
            )
        }
        Some(PathSubCmd::Suggest) => {
            let report = path_engine::suggest_path(opts.ctx)?;
            emit_fields(opts.format, &report, opts.fields)
        }
    }
}
