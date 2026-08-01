use anyhow::{Context, Result};
use serde_json::json;

use crate::bootstrap;
use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::install;
use crate::paths::PlanContext;
use crate::store;

pub(crate) struct InitOptions<'a> {
    pub ctx: &'a PlanContext,
    pub profile: Option<&'a str>,
    pub from_repo: bool,
    pub force: bool,
    pub with_cursor_skill: bool,
    pub with_opencode_skill: bool,
    pub skip_root_agents: bool,
    pub format: Fmt,
}

pub(crate) fn cmd_init(opts: InitOptions) -> Result<()> {
    let txn = crate::plan_io::PlanWriteTxn::acquire_project_root(&opts.ctx.project_root)?;
    txn.run(|_| cmd_init_inner(opts))
}

fn cmd_init_inner(opts: InitOptions) -> Result<()> {
    let created = store::init_plan(opts.ctx, opts.profile, opts.force)?;
    let mut payload = json!({
        "ok": true,
        "plan_dir": opts.ctx.plan_dir,
        "profile": opts.profile.unwrap_or("full"),
        "created": created,
    });
    if opts.from_repo {
        let bootstrap_report = bootstrap::apply_from_repo(opts.ctx, opts.profile)?;
        payload["bootstrap"] = serde_json::to_value(bootstrap_report)?;
    }
    let mut skills_installed = Vec::new();
    if opts.with_cursor_skill {
        let paths = install::install_project_skill(
            &opts.ctx.project_root,
            install::ProjectSkillHarness::Cursor,
        )?;
        for path in paths {
            skills_installed.push(json!({ "harness": "cursor", "path": path }));
        }
    }
    if opts.with_opencode_skill {
        let paths = install::install_project_skill(
            &opts.ctx.project_root,
            install::ProjectSkillHarness::Opencode,
        )?;
        for path in paths {
            skills_installed.push(json!({ "harness": "opencode", "path": path }));
        }
    }
    if !skills_installed.is_empty() {
        payload["skills_installed"] = json!(skills_installed);
    }
    if !opts.skip_root_agents {
        let snippet = crate::assets::read_embedded("templates/ROOT-AGENTS-SNIPPET.md")?;
        let root_path = opts.ctx.project_root.join("AGENTS.md");
        if !root_path.exists() {
            std::fs::write(&root_path, &snippet)
                .with_context(|| format!("write {}", root_path.display()))?;
            payload["root_agents"] = json!(root_path.to_string_lossy());
        }
    }
    emit(opts.format, &payload)
}
