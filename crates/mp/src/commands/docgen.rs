//! M173 S3: `mp docgen` — walk the clap Command tree and emit
//! markdown tables for every command group. Output lands in
//! `docs/concepts/06 - Reference/generated/<group>.md` so the
//! hand-maintained `MP-COMMANDS.md` and `AGENT-READINESS.md` can
//! include them via `<!-- mp:include <fragment> -->` markers.
//!
//! The walk is read-only and deterministic — running it twice
//! produces byte-identical output. The format is intentionally
//! narrow (description, usage, options, subcommands) so the
//! generated tables stay diff-friendly across clap surface changes.

use anyhow::{Context, Result};
use clap::CommandFactory;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::Cli;
use crate::commands::common::emit_value;
use crate::paths::PlanContext;

/// Group-level summary for the JSON report.
#[derive(Debug, Serialize)]
pub struct DocgenReport {
    pub ok: bool,
    pub out_dir: String,
    pub groups: Vec<String>,
    pub files_written: usize,
}

pub(crate) fn cmd_docgen(
    ctx: &PlanContext,
    out: Option<&Path>,
    group: Option<&str>,
    format: crate::cli::OutputFormat,
    fields: &[String],
) -> Result<()> {
    let out_dir = match out {
        Some(p) => p.to_path_buf(),
        None => default_out_dir(ctx)?,
    };
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("create out dir {}", out_dir.display()))?;

    let cli = Cli::command();
    let mut groups: BTreeMap<String, String> = BTreeMap::new();

    for top in cli.get_subcommands() {
        let name = top.get_name().to_string();
        if let Some(filter) = group {
            if name != filter {
                continue;
            }
        }
        let md = render_group(top);
        groups.insert(name.clone(), md);
    }

    let mut files_written = 0usize;
    for (name, body) in &groups {
        let dest = out_dir.join(format!("{name}.md"));
        fs::write(&dest, body).with_context(|| format!("write {}", dest.display()))?;
        files_written += 1;
    }

    let report = DocgenReport {
        ok: true,
        out_dir: out_dir.to_string_lossy().to_string(),
        groups: groups.keys().cloned().collect(),
        files_written,
    };
    let value = serde_json::to_value(&report)?;
    emit_value(format, &value, fields)
}

fn default_out_dir(ctx: &PlanContext) -> Result<PathBuf> {
    // Default to `<plan_dir>/../docs/concepts/06 - Reference/generated/`
    // (i.e. `<project_root>/docs/concepts/06 - Reference/generated/`).
    // M173 F-07 (sub-agent review): the prior `current_dir()` fallback
    // silently wrote into the user's CWD on a fresh checkout, dropping
    // the bundle in the wrong place. Anchor the default to the
    // discovered plan dir so `mp docgen` from any subdirectory of
    // the repo writes to the canonical location.
    let plan_dir = ctx
        .plan_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("plan_dir has no parent: {}", ctx.plan_dir.display()))?;
    Ok(plan_dir.join("docs/concepts/06 - Reference/generated"))
}

fn render_group(cmd: &clap::Command) -> String {
    let name = cmd.get_name();
    let about = cmd.get_about().map(|s| s.to_string()).unwrap_or_default();
    let long_about = cmd
        .get_long_about()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let usage = cmd.clone().render_usage().to_string();
    let usage = usage.trim_end().to_string();
    let subs: Vec<&clap::Command> = cmd.get_subcommands().collect();

    let mut out = String::new();
    out.push_str(&format!("# `mp {name}`\n\n"));
    if !about.is_empty() {
        out.push_str(&format!("**{about}**\n\n"));
    }
    if !long_about.is_empty() && long_about != about {
        out.push_str(&format!("{long_about}\n\n"));
    }
    out.push_str(&format!("**Usage:**\n\n```text\n{usage}\n```\n\n"));

    if !subs.is_empty() {
        out.push_str("**Subcommands:**\n\n");
        out.push_str("| Name | Description |\n|------|-------------|\n");
        for s in &subs {
            let n = s.get_name();
            let d = s.get_about().map(|x| x.to_string()).unwrap_or_default();
            out.push_str(&format!("| `{n}` | {d} |\n"));
        }
        out.push('\n');
    }

    // Render the leaf command's options. For parent commands with
    // subcommands, the options table is intentionally omitted — the
    // parent carries the global flags, not the per-leaf options.
    if subs.is_empty() {
        let args: Vec<_> = cmd.get_arguments().collect();
        if !args.is_empty() {
            out.push_str("**Options:**\n\n");
            out.push_str("| Flag | Description |\n|------|-------------|\n");
            for a in args {
                let short = a.get_short().map(|c| format!("`-{c}`")).unwrap_or_default();
                let long = a.get_long().map(|l| format!("`--{l}`")).unwrap_or_default();
                let help = a.get_help().map(|s| s.to_string()).unwrap_or_default();
                let help = help.replace('|', "\\|").replace('\n', " ");
                let cell = if !short.is_empty() && !long.is_empty() {
                    format!("{short}, {long}")
                } else if !long.is_empty() {
                    long
                } else if !short.is_empty() {
                    short
                } else {
                    String::new()
                };
                out.push_str(&format!("| {cell} | {help} |\n"));
            }
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_group_emits_header_usage_and_subs() {
        // Build a synthetic clap Command for a stable test target.
        let cmd = clap::Command::new("foo")
            .about("foo command")
            .subcommand(clap::Command::new("bar").about("bar sub"));
        let md = render_group(&cmd);
        assert!(md.contains("# `mp foo`"), "header missing: {md}");
        assert!(md.contains("foo command"), "about missing: {md}");
        assert!(md.contains("**Usage:**"), "usage header missing: {md}");
        assert!(md.contains("foo"), "usage body missing: {md}");
        assert!(
            md.contains("**Subcommands:**") && md.contains("`bar`"),
            "subcommand table missing: {md}"
        );
    }

    #[test]
    fn render_group_leaf_includes_options_table() {
        let cmd = clap::Command::new("leaf").arg(
            clap::Arg::new("flag")
                .long("flag")
                .short('f')
                .help("a flag"),
        );
        let md = render_group(&cmd);
        assert!(md.contains("**Options:**"), "options header missing: {md}");
        assert!(md.contains("--flag"), "--flag row missing: {md}");
        assert!(md.contains("-f"), "-f row missing: {md}");
        assert!(md.contains("a flag"), "help text missing: {md}");
    }

    #[test]
    fn render_group_omits_options_table_for_parents() {
        let cmd = clap::Command::new("parent")
            .arg(clap::Arg::new("shared").long("shared"))
            .subcommand(clap::Command::new("child"));
        let md = render_group(&cmd);
        assert!(
            !md.contains("**Options:**"),
            "parent command should not emit Options table"
        );
    }
}
