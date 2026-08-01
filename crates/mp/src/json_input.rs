use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Soft cap for `@file` / `--file` JSON payloads (32 MiB).
pub const MAX_JSON_INPUT_BYTES: u64 = 32 * 1024 * 1024;

fn default_project_root() -> Option<PathBuf> {
    std::env::var_os("MP_PROJECT")
        .or_else(|| std::env::var_os("MPH_PROJECT"))
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
}

/// Read UTF-8 text while enforcing `max_bytes` on the stream itself.
pub fn read_to_string_bounded(
    reader: impl Read,
    max_bytes: u64,
    source: impl std::fmt::Display,
) -> Result<String> {
    let limit = max_bytes
        .checked_add(1)
        .context("JSON input size limit overflow")?;
    let mut bytes = Vec::new();
    reader
        .take(limit)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {source}"))?;
    if bytes.len() as u64 > max_bytes {
        bail!("{source} exceeds {max_bytes} bytes; refusing to load");
    }
    String::from_utf8(bytes).with_context(|| format!("{source} is not valid UTF-8"))
}

pub(crate) fn read_file_bounded(path: &Path, max_bytes: u64) -> Result<String> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    read_to_string_bounded(file, max_bytes, format!("file {}", path.display()))
}

/// Open a path that has already been containment-checked / canonicalized.
/// On Unix, `O_NOFOLLOW` refuses a symlink swapped onto the canonical target
/// between check and open (residual TOCTOU on the final open).
fn open_canonical_regular_file(path: &Path) -> Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .with_context(|| format!("open {}", path.display()))?;
        let meta = file
            .metadata()
            .with_context(|| format!("stat {}", path.display()))?;
        if !meta.is_file() {
            bail!("refusing to read {}: not a regular file", path.display());
        }
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        let meta =
            std::fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            bail!("refusing to read {}: not a regular file", path.display());
        }
        File::open(path).with_context(|| format!("open {}", path.display()))
    }
}

fn canonical_contained_path(path: &Path, root: &Path) -> Result<PathBuf> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let root_c = root
        .canonicalize()
        .with_context(|| format!("canonicalize project root {}", root.display()))?;
    let candidate = abs
        .canonicalize()
        .with_context(|| format!("canonicalize input path {}", path.display()))?;
    if !candidate.starts_with(&root_c) {
        bail!(
            "refusing to read {} — path escapes project root {}",
            path.display(),
            root.display()
        );
    }
    Ok(candidate)
}

fn read_contained_file(path: &Path, root: Option<&Path>) -> Result<String> {
    match root {
        Some(root) => {
            let open_path = canonical_contained_path(path, root)?;
            // Containment used canonicalize (follows in-root symlinks). Open
            // the resolved path with O_NOFOLLOW so a post-check replacement
            // symlink on that canonical target cannot escape; in-root symlink
            // *inputs* remain allowed because we open the resolved target.
            let file = open_canonical_regular_file(&open_path)?;
            read_to_string_bounded(
                file,
                MAX_JSON_INPUT_BYTES,
                format!("file {}", path.display()),
            )
        }
        None => read_file_bounded(path, MAX_JSON_INPUT_BYTES),
    }
}

pub fn read_json_arg(arg: &str) -> Result<String> {
    read_json_arg_in(arg, default_project_root().as_deref())
}

/// Like [`read_json_arg`], but when `project_root` is set, `@file` paths
/// must stay under that root (prompt-injection / exfil guard).
pub fn read_json_arg_in(arg: &str, project_root: Option<&Path>) -> Result<String> {
    if arg == "@-" {
        return read_to_string_bounded(
            std::io::stdin().lock(),
            MAX_JSON_INPUT_BYTES,
            "standard input",
        );
    }
    if let Some(path) = arg.strip_prefix('@') {
        return read_contained_file(Path::new(path), project_root);
    }
    if arg.len() as u64 > MAX_JSON_INPUT_BYTES {
        bail!("inline JSON exceeds {MAX_JSON_INPUT_BYTES} bytes; refusing to load");
    }
    Ok(arg.to_string())
}

pub fn read_json_payload(file: Option<&Path>, json: Option<&str>) -> Result<String> {
    read_json_payload_in(file, json, default_project_root().as_deref())
}

pub fn read_json_payload_in(
    file: Option<&Path>,
    json: Option<&str>,
    project_root: Option<&Path>,
) -> Result<String> {
    if let Some(path) = file {
        return read_contained_file(path, project_root);
    }
    if let Some(j) = json {
        return read_json_arg_in(j, project_root);
    }
    anyhow::bail!("provide --json or --file");
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn open_canonical_regular_file_rejects_symlink() {
        let root = tempfile::TempDir::new().unwrap();
        let real = root.path().join("real.json");
        let link = root.path().join("link.json");
        std::fs::write(&real, "{}").unwrap();
        symlink(&real, &link).unwrap();

        assert!(open_canonical_regular_file(&real).is_ok());
        let error = open_canonical_regular_file(&link).unwrap_err().to_string();
        assert!(
            error.contains("open ") || error.contains("symbolic") || error.contains("Too many"),
            "{error}"
        );
    }
}
