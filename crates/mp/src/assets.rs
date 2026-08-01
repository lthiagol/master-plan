use std::env;
use std::path::PathBuf;

use include_dir::Dir;

/// Embedded `templates/` tree (compiled into the binary at build time).
pub static EMBEDDED_TEMPLATES: Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../templates");
/// Embedded `schemas/` tree (compiled into the binary at build time).
pub static EMBEDDED_SCHEMAS: Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../schemas");

/// Toolkit root for filesystem OVERRIDES and install paths only.
///
/// Since M29, templates and schemas are embedded in the binary and do not need
/// to exist on disk. This function is used only to honor a local `MP_HOME`
/// override or to resolve install paths (e.g. the skill template source
/// under `templates/skills/`).
pub fn toolkit_home() -> PathBuf {
    if let Ok(home) = env::var("MP_HOME") {
        return PathBuf::from(home);
    }
    if let Ok(home) = env::var("MPH_HOME") {
        return PathBuf::from(home);
    }
    if let Some(dir) = env::var_os("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(dir);
        if let Some(root) = p.parent().and_then(|c| c.parent()) {
            if root.join("templates").is_dir() {
                return root.to_path_buf();
            }
        }
    }
    if let Ok(mut dir) = env::current_dir() {
        loop {
            if dir.join("templates").join("defaults").is_dir() {
                return dir.clone();
            }
            if !dir.pop() {
                break;
            }
        }
    }
    dirs_home().join(".agents").join("master-plan")
}

fn dirs_home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Look up an embedded asset by relative path (e.g. `templates/defaults/plan.json`
/// or `schemas/milestone.schema.json`), returning its UTF-8 contents. The
/// `templates/` / `schemas/` prefix selects which embedded tree to search; the
/// remainder is resolved within that tree (files are keyed relative to the root).
///
/// This checks the compile-time embedded registry only — it does NOT consult
/// `MP_HOME` — so it is suitable for integrity self-tests that must reflect
/// what was compiled into the binary, independent of the runtime environment.
pub fn embedded_asset(rel: &str) -> Option<&'static str> {
    let file = if let Some(sub) = rel.strip_prefix("templates/") {
        EMBEDDED_TEMPLATES.get_file(sub)
    } else if let Some(sub) = rel.strip_prefix("schemas/") {
        EMBEDDED_SCHEMAS.get_file(sub)
    } else {
        None
    }?;
    file.contents_utf8()
}

pub fn template_path(rel: &str) -> PathBuf {
    toolkit_home().join(rel)
}

/// Read an asset: a file under `MP_HOME` takes precedence (override); otherwise
/// the asset is served from the embedded tree. This keeps the binary
/// self-contained while preserving the `MP_HOME` escape hatch.
pub fn read_embedded(rel: &str) -> anyhow::Result<String> {
    if let Ok(home) = env::var("MP_HOME") {
        let p = PathBuf::from(home).join(rel);
        if p.is_file() {
            return Ok(std::fs::read_to_string(&p)?);
        }
    }
    embedded_asset(rel)
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("asset not found: {}", rel))
}

pub fn read_embedded_if_exists(rel: &str) -> Option<String> {
    if let Ok(home) = env::var("MP_HOME") {
        let p = PathBuf::from(home).join(rel);
        if p.is_file() {
            return std::fs::read_to_string(p).ok();
        }
    }
    embedded_asset(rel).map(|s| s.to_string())
}
