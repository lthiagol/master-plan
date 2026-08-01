use anyhow::{Context, Result};
use serde_json::json;
use std::path::PathBuf;

use crate::cli::{OutputFormat as Fmt, ScratchCmd};
use crate::commands::common::emit_value;
use crate::paths::PlanContext;

pub(crate) fn cmd_scratch(
    ctx: &PlanContext,
    cmd: ScratchCmd,
    format: Fmt,
    fields: &[String],
) -> Result<()> {
    match cmd {
        ScratchCmd::Path => {
            let dir = scratch_dir(ctx)?;
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create scratch dir: {}", dir.display()))?;
            let value = json!({ "path": dir.to_string_lossy() });
            emit_value(format, &value, fields)
        }
        ScratchCmd::New { label } => {
            let dir = scratch_dir(ctx)?;
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create scratch dir: {}", dir.display()))?;
            let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
            let subdir = dir.join(format!("{}-{}", sanitize_label(&label), timestamp));
            std::fs::create_dir_all(&subdir).with_context(|| {
                format!("failed to create scratch subdir: {}", subdir.display())
            })?;
            let value = json!({
                "path": subdir.to_string_lossy(),
                "scratch_dir": dir.to_string_lossy(),
            });
            emit_value(format, &value, fields)
        }
    }
}

fn scratch_dir(ctx: &PlanContext) -> Result<PathBuf> {
    Ok(ctx.project_root.join(".mp-scratch"))
}

fn sanitize_label(label: &str) -> String {
    let sanitized: String = label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized
        .trim_matches(|c: char| c == '.' || c == '_' || c == '-')
        .chars()
        .take(50)
        .collect::<String>();
    if trimmed.is_empty() {
        "scratch".to_string()
    } else {
        trimmed
    }
}
