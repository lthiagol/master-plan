use anyhow::Result;

use crate::cli::{resolve_format, ActivityCmd, Cli, Commands, OverviewCmd, ReviewCmd};
use crate::commands::activity as cmd_activity_mod;
use crate::commands::annotation as cmd_annotation_mod;
use crate::commands::autopilot as cmd_autopilot_mod;
use crate::commands::backlog as cmd_backlog_mod;
use crate::commands::breaking_release as cmd_breaking_release_mod;
use crate::commands::brief as cmd_brief_mod;
use crate::commands::brownfield as cmd_brownfield_mod;
use crate::commands::changelog as cmd_changelog_mod;
use crate::commands::common::{emit, emit_fields};
use crate::commands::config as cmd_config_mod;
use crate::commands::decision as cmd_decision_mod;
use crate::commands::digest as cmd_digest_mod;
use crate::commands::docgen as cmd_docgen_mod;
use crate::commands::doctor as cmd_doctor_mod;
use crate::commands::edit as cmd_edit_mod;
use crate::commands::execution as cmd_execution_mod;
use crate::commands::git as cmd_git_mod;
use crate::commands::graph as cmd_graph_mod;
use crate::commands::idea as cmd_idea_mod;
use crate::commands::init as cmd_init_mod;
use crate::commands::install as cmd_install_mod;
use crate::commands::list as cmd_list_mod;
use crate::commands::milestone as cmd_milestone_mod;
use crate::commands::note as cmd_note_mod;
use crate::commands::overview as cmd_overview_mod;
use crate::commands::path as cmd_path_mod;
use crate::commands::plan as cmd_plan_mod;
use crate::commands::release as cmd_release_mod;
use crate::commands::reviews as cmd_reviews_mod;
use crate::commands::scratch as cmd_scratch_mod;
use crate::commands::search as cmd_search_mod;
use crate::commands::session as cmd_session_mod;
use crate::commands::show as cmd_show_mod;
use crate::commands::skill as cmd_skill_mod;
use crate::commands::specs as cmd_specs_mod;
use crate::commands::status as cmd_status_mod;
use crate::commands::sync as cmd_sync_mod;
use crate::commands::track as cmd_track_mod;
use crate::commands::validate as cmd_validate_mod;
use crate::commands::watch as cmd_watch_mod;
use crate::digest::DigestOptions;
use crate::hygiene;
use crate::inbox;
use crate::migrate;
use crate::paths::PlanContext;

pub(super) fn run(cli: Cli) -> Result<()> {
    let ctx = PlanContext::discover(cli.plan_dir.clone(), cli.project_root.clone())?;
    let format = resolve_format(cli.format);
    let fields: &[String] = &cli.fields;

    match cli.command {
        Commands::Init {
            profile,
            from_repo,
            force,
            merge_root_agents,
            with_cursor_skill,
            with_opencode_skill,
            skip_root_agents,
            refresh,
            yes,
        } => cmd_init_mod::cmd_init(cmd_init_mod::InitOptions {
            ctx: &ctx,
            profile: profile.as_deref(),
            from_repo,
            force,
            merge_root_agents,
            with_cursor_skill,
            with_opencode_skill,
            skip_root_agents,
            refresh,
            yes,
            format,
        }),
        Commands::Install {
            harness,
            global,
            dev,
            source,
            print_paths,
            toolkit_only,
            skills,
            agents,
            check,
            list_skills,
        } => cmd_install_mod::cmd_install(cmd_install_mod::InstallOptions {
            harness,
            global,
            dev,
            source: source.as_deref(),
            print_paths,
            toolkit_only,
            skills,
            agents,
            check,
            list_skills,
            format,
        }),
        Commands::Uninstall {
            harness,
            global,
            purge,
        } => cmd_install_mod::cmd_uninstall(harness, global, purge, format),
        Commands::Doctor { project } => cmd_doctor_mod::cmd_doctor(&ctx, project, format),
        Commands::Specs { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_specs_mod::cmd_specs(&ctx, cmd, format)
        }
        Commands::Spec { cmd } => super::spec::run(&ctx, cmd, format, fields),
        Commands::Annotation { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_annotation_mod::cmd_annotation(&ctx, cmd, format)
        }
        Commands::Brownfield { cmd } => cmd_brownfield_mod::cmd_brownfield(&ctx, cmd, format),
        Commands::Brief { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_brief_mod::cmd_brief(&ctx, cmd, format)
        }
        Commands::Idea { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_idea_mod::cmd_idea(&ctx, cmd, format)
        }
        Commands::Session { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_session_mod::cmd_session(&ctx, cmd, format)
        }
        Commands::Autopilot { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_autopilot_mod::cmd_autopilot(&ctx, cmd, format, fields)
        }
        Commands::Agent { cmd } => super::agent::run(&ctx, cmd, format),
        Commands::Skill { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_skill_mod::cmd_skill(&ctx, cmd, format)
        }
        Commands::Graph {
            milestone,
            with_steps,
            with_ac,
            cmd,
        } => {
            ctx.ensure_plan_exists()?;
            cmd_graph_mod::cmd_graph(&ctx, milestone.as_deref(), with_steps, with_ac, cmd, format)
        }
        Commands::Inbox { filter } => {
            ctx.ensure_plan_exists()?;
            let report = inbox::build_inbox(&ctx, &filter)?;
            emit_fields(format, &report, fields)
        }
        Commands::Hygiene { stale_days } => {
            ctx.ensure_plan_exists()?;
            let report = hygiene::run_hygiene(&ctx, stale_days)?;
            emit(format, &report)
        }
        Commands::Migrate {
            cmd,
            kinds,
            dry_run,
            yes,
        } => {
            ctx.ensure_plan_exists()?;
            match cmd {
                Some(crate::cli::MigrateCmd::ManualPrefixBackfill {
                    dry_run: sub_dry,
                    yes: sub_yes,
                }) => {
                    // Subcommand flags win; top-level --dry-run/--yes also apply
                    // so `mp migrate manual-prefix-backfill --yes` works either
                    // way. Dry-run wins over yes (never write when dry_run).
                    let dry = dry_run || sub_dry;
                    let apply_yes = yes || sub_yes;
                    let txn = crate::plan_io::PlanWriteTxn::acquire(&ctx.plan_dir)?;
                    let report = if dry {
                        txn.run(|_| {
                            crate::migrations::run_manual_prefix_backfill(&ctx, dry, apply_yes)
                        })?
                    } else {
                        txn.run_recoverable(|_| {
                            crate::migrations::run_manual_prefix_backfill(&ctx, dry, apply_yes)
                        })?
                    };
                    emit(format, &report)
                }
                None if kinds => {
                    let plan_dir = &ctx.plan_dir;
                    let txn = crate::plan_io::PlanWriteTxn::acquire(plan_dir)?;
                    let report = if dry_run {
                        txn.run(|_| migrate::migrate_kinds(plan_dir, true))?
                    } else {
                        txn.run_recoverable(|_| migrate::migrate_kinds(plan_dir, false))?
                    };
                    emit(format, &report)
                }
                None => {
                    anyhow::bail!(
                        "mp migrate requires --kinds or a subcommand \
                         (e.g. `mp migrate manual-prefix-backfill --dry-run`)"
                    );
                }
            }
        }
        Commands::Backlog { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_backlog_mod::cmd_backlog(&ctx, cmd, format)
        }
        Commands::Decision { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_decision_mod::cmd_decision(&ctx, cmd, format)
        }
        Commands::Config { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_config_mod::cmd_config(&ctx, cmd, format)
        }
        Commands::Validate { summary } => {
            cmd_validate_mod::cmd_validate(&ctx, format, fields, summary)
        }
        Commands::Sync => {
            ctx.ensure_plan_exists()?;
            cmd_sync_mod::cmd_sync(&ctx, format)
        }
        Commands::Status { summary } => {
            ctx.ensure_plan_exists()?;
            cmd_status_mod::cmd_status(&ctx, format, fields, summary)
        }
        Commands::Activity(cmd) => {
            ctx.ensure_plan_exists()?;
            let ActivityCmd { limit } = cmd;
            cmd_activity_mod::cmd_activity(&ctx, format, fields, limit)
        }
        Commands::Overview(cmd) => {
            ctx.ensure_plan_exists()?;
            let OverviewCmd { summary } = cmd;
            cmd_overview_mod::cmd_overview(&ctx, format, fields, summary)
        }
        Commands::Next { lane, summary } => {
            ctx.ensure_plan_exists()?;
            cmd_status_mod::cmd_next_step(&ctx, format, fields, lane, summary)
        }
        Commands::Path {
            horizon,
            include_grooming,
            prioritize_coverage,
            include_coverage_gaps,
            lane,
            all,
            no_ideas,
            summary,
            cmd,
        } => {
            ctx.ensure_plan_exists()?;
            cmd_path_mod::cmd_path(cmd_path_mod::PathOptions {
                ctx: &ctx,
                horizon,
                include_grooming,
                prioritize_coverage,
                include_coverage_gaps,
                cmd,
                format,
                fields,
                lane: lane.map(|l| match l {
                    crate::cli::path::LaneArg::Blocked => "blocked".to_string(),
                    crate::cli::path::LaneArg::Execution => "execution".to_string(),
                    crate::cli::path::LaneArg::Review => "review".to_string(),
                    crate::cli::path::LaneArg::Grooming => "grooming".to_string(),
                    crate::cli::path::LaneArg::Backlog => "backlog".to_string(),
                }),
                all_lanes: all,
                no_ideas,
                path_summary: summary,
            })
        }
        Commands::Plan { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_plan_mod::cmd_plan(&ctx, cmd, format)
        }
        Commands::Execution { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_execution_mod::cmd_execution(&ctx, cmd, format)
        }
        Commands::Reviews { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_reviews_mod::cmd_reviews(&ctx, cmd, format, fields)
        }
        Commands::List { target } => {
            ctx.ensure_plan_exists()?;
            cmd_list_mod::cmd_list(&ctx, target, format, fields)
        }
        Commands::Show { target } => cmd_show_mod::cmd_show(&ctx, target, format, fields),
        Commands::Interview { cmd } => super::interview::run(&ctx, cmd, format),
        Commands::Track { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_track_mod::cmd_track(&ctx, cmd, format, fields)
        }
        Commands::Release { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_release_mod::cmd_release(&ctx, cmd, format)
        }
        Commands::Milestone { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_milestone_mod::cmd_milestone(&ctx, cmd, format)
        }
        Commands::Changelog { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_changelog_mod::cmd_changelog(&ctx, cmd, format)
        }
        Commands::Digest {
            since_handoff,
            since,
            days,
            markdown,
            out,
        } => {
            ctx.ensure_plan_exists()?;
            cmd_digest_mod::cmd_digest(
                &ctx,
                DigestOptions {
                    since_handoff,
                    since,
                    days,
                    markdown,
                    out,
                },
                format,
            )
        }
        Commands::Note { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_note_mod::cmd_note(&ctx, cmd, format)
        }
        Commands::Edit { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_edit_mod::cmd_edit(&ctx, cmd, format)
        }
        Commands::Search {
            query,
            filter_type,
            limit,
            include,
            group_by,
        } => {
            ctx.ensure_plan_exists()?;
            cmd_search_mod::cmd_search(
                &ctx,
                &query,
                filter_type.as_deref(),
                format,
                limit,
                &include,
                group_by.as_deref(),
            )
        }
        Commands::Git { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_git_mod::cmd_git(&ctx, cmd, format)
        }
        Commands::Scratch { cmd } => cmd_scratch_mod::cmd_scratch(&ctx, cmd, format, fields),
        Commands::Watch {
            ids,
            dry_run,
            log_file,
            stall_timeout_ms,
            poll_interval_ms,
            resume,
            force,
            detach,
        } => {
            // M219: emit the canonical deprecation notice on stderr
            // before any other work, so the warning fires for every
            // legacy `mp watch` invocation regardless of plan state.
            // The legacy `mp watch` walks the same code path as
            // `mp autopilot start`, so exit codes and stdout remain
            // identical; the only difference is the single deprecation
            // line below. Wording is pinned by AC-03 byte-for-byte.
            eprintln!("mp watch is deprecated; use 'mp autopilot' instead.");
            ctx.ensure_plan_exists()?;
            cmd_watch_mod::cmd_watch(
                &ctx,
                ids,
                dry_run,
                log_file,
                stall_timeout_ms,
                poll_interval_ms,
                resume,
                force,
                detach,
                format,
            )
        }
        Commands::Docgen { out, group } => {
            // docgen doesn't need a plan; it's a CLI-shape tool.
            cmd_docgen_mod::cmd_docgen(&ctx, out.as_deref(), group.as_deref(), format, fields)
        }
        Commands::Review { cmd } => {
            ctx.ensure_plan_exists()?;
            match cmd {
                ReviewCmd::Sidecar {
                    milestone,
                    finding,
                    output,
                } => cmd_reviews_mod::cmd_review_sidecar(
                    &ctx,
                    &milestone,
                    finding.as_deref(),
                    &output,
                    format,
                    fields,
                ),
            }
        }
        Commands::WatchControl { cmd } => super::watch_control::run(&ctx, cmd, format, fields),
        Commands::BreakingRelease { cmd } => {
            ctx.ensure_plan_exists()?;
            cmd_breaking_release_mod::cmd_breaking_release(&ctx, cmd, format)
        }
    }
}
