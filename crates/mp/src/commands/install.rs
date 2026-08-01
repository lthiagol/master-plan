use anyhow::Result;
use serde_json::json;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::harness;
use crate::install;

pub(crate) struct InstallOptions<'a> {
    pub harness: Vec<String>,
    pub global: bool,
    pub dev: bool,
    pub source: Option<&'a std::path::Path>,
    pub print_paths: bool,
    pub toolkit_only: bool,
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub check: bool,
    pub list_skills: bool,
    pub format: Fmt,
}

pub(crate) fn cmd_install(opts: InstallOptions) -> Result<()> {
    if opts.list_skills {
        // List mode: independent of --check / --skills. Reads the
        // manifest for per-harness deployment state and the embedded
        // registry for catalog + core classification.
        let source_root = opts
            .source
            .map(|s| s.to_path_buf())
            .unwrap_or_else(crate::assets::toolkit_home);
        let report = install::list_skills(&source_root)?;
        return emit(opts.format, &report);
    }
    if opts.check && !opts.skills.is_empty() {
        anyhow::bail!("--check and --skills are mutually exclusive; use --check to validate the full registry, or run without --check to deploy selected skills");
    }
    if opts.check {
        let source_root = opts
            .source
            .map(|s| s.to_path_buf())
            .unwrap_or_else(crate::assets::toolkit_home);
        let mut report = install::check_registry(&source_root)?;
        // M158 AC-07: layer file-tree drift findings on top of the
        // registry check so `mp install --check` surfaces siblings
        // missing or stale on disk after a torn install or a
        // hand-removal. Drift warnings don't change `ok` (the
        // registry itself is still valid) but they make the
        // on-disk mirror inconsistency visible.
        let drift = install::check_deployment_files(&source_root)?;
        report.warnings.extend(drift);
        return emit(opts.format, &report);
    }
    if opts.print_paths {
        let ids = install::resolve_harness_ids(&opts.harness)?;
        let paths: Vec<serde_json::Value> = ids
            .iter()
            .filter_map(|id| harness::harness_by_id(id))
            .map(|h| harness::print_paths_json(&h))
            .collect();
        return emit(opts.format, &json!({ "ok": true, "paths": paths }));
    }
    let ids = if opts.toolkit_only {
        Vec::new()
    } else {
        install::resolve_harness_ids(&opts.harness)?
    };
    let skills_filter: Option<Vec<String>> = if opts.skills.is_empty() {
        None
    } else {
        Some(opts.skills)
    };
    let agents_filter: Option<Vec<String>> = if opts.agents.is_empty() {
        None
    } else {
        Some(opts.agents)
    };
    let report = install::install(
        &ids,
        opts.global,
        opts.dev,
        opts.source,
        opts.toolkit_only,
        skills_filter.as_deref(),
        agents_filter.as_deref(),
    )?;
    emit(opts.format, &report)
}

pub(crate) fn cmd_uninstall(
    harness: Vec<String>,
    global: bool,
    purge: bool,
    format: Fmt,
) -> Result<()> {
    let ids = install::resolve_harness_ids(&harness)?;
    let report = install::uninstall(&ids, global, purge)?;
    emit(format, &report)
}
