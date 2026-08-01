use std::fs;

use anyhow::Result;

use crate::cli::OutputFormat as Fmt;
use crate::commands::common::emit;
use crate::digest::{self, DigestOptions};
use crate::paths::PlanContext;

pub(crate) fn cmd_digest(ctx: &PlanContext, opts: DigestOptions, format: Fmt) -> Result<()> {
    let since = digest::resolve_since(ctx, &opts)?;
    digest::validate_since(&since)?;
    let report = digest::build_digest(ctx, &since)?;

    if opts.markdown {
        let md = digest::format_markdown(&report);
        if let Some(path) = opts.out {
            fs::write(&path, &md)?;
        } else {
            print!("{md}");
        }
        return Ok(());
    }

    if let Some(path) = opts.out {
        let json = serde_json::to_string_pretty(&report)?;
        fs::write(path, json)?;
        return Ok(());
    }

    emit(format, &report)
}
