//! Crash-recoverable envelope for multi-file plan mutations.
//!
//! The command layer owns the plan lock and opens this envelope before invoking
//! legacy mutation code. A bounded manifest records a complete before-image of
//! regular plan files. Failure rolls back immediately; process termination is
//! recovered by the next writer before it loads plan state.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const TXN_DIR: &str = ".mp-txn";
const COMMITTED_MARKER: &str = "COMMITTED";
const MANIFEST_VERSION: u32 = 1;
const MAX_MANIFEST_PATHS: usize = 100_000;

#[derive(Debug, Serialize, Deserialize)]
struct RecoveryManifest {
    version: u32,
    baseline: Vec<String>,
    #[serde(default)]
    directories: Vec<String>,
}

/// Before-image transaction used while a [`crate::plan_io::PlanWriteTxn`] is held.
pub(crate) struct RecoveryTxn {
    plan_dir: PathBuf,
    txn_dir: PathBuf,
    baseline: BTreeSet<PathBuf>,
    baseline_dirs: BTreeSet<PathBuf>,
    active: bool,
}

impl RecoveryTxn {
    pub(crate) fn begin(plan_dir: &Path) -> Result<Self> {
        recover_pending(plan_dir)?;
        let root = plan_dir.join(TXN_DIR);
        fs::create_dir_all(&root)
            .with_context(|| format!("create transaction root {}", root.display()))?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let txn_dir = root.join(format!("{}-{nonce}", std::process::id()));
        let backup_dir = txn_dir.join("before");
        fs::create_dir_all(&backup_dir)
            .with_context(|| format!("create transaction backup {}", backup_dir.display()))?;
        set_private_dir_permissions(&root)?;
        set_private_dir_permissions(&txn_dir)?;
        set_private_dir_permissions(&backup_dir)?;

        let (baseline, baseline_dirs) = collect_plan_tree(plan_dir)?;
        if baseline.len() > MAX_MANIFEST_PATHS {
            bail!(
                "plan transaction contains {} files (max {MAX_MANIFEST_PATHS})",
                baseline.len()
            );
        }
        for rel in &baseline {
            let src = plan_dir.join(rel);
            let dest = backup_dir.join(rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src, &dest).with_context(|| {
                format!(
                    "stage transaction before-image {} -> {}",
                    src.display(),
                    dest.display()
                )
            })?;
            sync_file(&dest)?;
        }

        let manifest = RecoveryManifest {
            version: MANIFEST_VERSION,
            baseline: baseline
                .iter()
                .map(|p| relative_string(p))
                .collect::<Result<Vec<_>>>()?,
            directories: baseline_dirs
                .iter()
                .map(|p| relative_string(p))
                .collect::<Result<Vec<_>>>()?,
        };
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        let manifest_path = txn_dir.join("manifest.json");
        crate::store::atomic_write(&manifest_path, bytes)?;
        set_private_permissions(&manifest_path)?;
        sync_dir(&backup_dir)?;
        sync_dir(&txn_dir)?;
        sync_dir(&root)?;

        Ok(Self {
            plan_dir: plan_dir.to_path_buf(),
            txn_dir,
            baseline,
            baseline_dirs,
            active: true,
        })
    }

    pub(crate) fn commit(mut self) -> Result<()> {
        // Durable seal before cleanup. A crash after this marker must keep the
        // after-image; recover_pending treats COMMITTED as "do not restore".
        write_committed_marker(&self.txn_dir)?;
        if crate::store::mutation_crash_armed() {
            // Sealed-but-cleanup-not-done window: leave the txn dir for
            // the next writer; Drop must not restore the before-image.
            self.active = false;
            std::process::abort();
        }
        self.active = false;
        fs::remove_dir_all(&self.txn_dir)
            .with_context(|| format!("remove transaction {}", self.txn_dir.display()))?;
        sync_dir(
            self.txn_dir
                .parent()
                .context("transaction directory has no parent")?,
        )
    }

    pub(crate) fn rollback(mut self) -> Result<()> {
        restore_before_image(
            &self.plan_dir,
            &self.txn_dir,
            &self.baseline,
            &self.baseline_dirs,
        )?;
        self.active = false;
        fs::remove_dir_all(&self.txn_dir)
            .with_context(|| format!("remove transaction {}", self.txn_dir.display()))?;
        Ok(())
    }
}

impl Drop for RecoveryTxn {
    fn drop(&mut self) {
        if self.active {
            let _ = restore_before_image(
                &self.plan_dir,
                &self.txn_dir,
                &self.baseline,
                &self.baseline_dirs,
            );
            let _ = fs::remove_dir_all(&self.txn_dir);
        }
    }
}

/// Recover every incomplete transaction. The plan write lock must be held.
pub(crate) fn recover_pending(plan_dir: &Path) -> Result<()> {
    let root = plan_dir.join(TXN_DIR);
    if !root.exists() {
        return Ok(());
    }
    reject_symlink(&root)?;
    let mut entries = fs::read_dir(&root)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let txn_dir = entry.path();
        reject_symlink(&txn_dir)?;
        if !entry.file_type()?.is_dir() {
            bail!(
                "unexpected non-directory transaction artifact {}",
                txn_dir.display()
            );
        }
        let committed_marker = txn_dir.join(COMMITTED_MARKER);
        if committed_marker.exists() {
            // Mutation finished; only cleanup was interrupted. Keep after-image.
            reject_symlink(&committed_marker)?;
            fs::remove_dir_all(&txn_dir).with_context(|| {
                format!(
                    "remove committed transaction {} after durable seal",
                    txn_dir.display()
                )
            })?;
            continue;
        }
        reject_symlink(&txn_dir.join("before"))?;
        let manifest_path = txn_dir.join("manifest.json");
        let raw =
            crate::store::read_text_bounded(&manifest_path, crate::store::MAX_PLAN_FILE_BYTES)?;
        let manifest: RecoveryManifest = serde_json::from_str(&raw)
            .with_context(|| format!("parse recovery manifest {}", manifest_path.display()))?;
        if manifest.version != MANIFEST_VERSION {
            bail!(
                "unsupported recovery manifest version {} in {}",
                manifest.version,
                manifest_path.display()
            );
        }
        if manifest.baseline.len() > MAX_MANIFEST_PATHS {
            bail!("recovery manifest has too many paths");
        }
        let baseline = manifest
            .baseline
            .iter()
            .map(|value| validate_relative(value))
            .collect::<Result<BTreeSet<_>>>()?;
        let baseline_dirs = manifest
            .directories
            .iter()
            .map(|value| validate_relative(value))
            .collect::<Result<BTreeSet<_>>>()?;
        restore_before_image(plan_dir, &txn_dir, &baseline, &baseline_dirs)?;
        fs::remove_dir_all(&txn_dir)
            .with_context(|| format!("remove recovered transaction {}", txn_dir.display()))?;
    }
    sync_dir(&root)?;
    Ok(())
}

fn write_committed_marker(txn_dir: &Path) -> Result<()> {
    let marker = txn_dir.join(COMMITTED_MARKER);
    // Marker lives under .mp-txn so mutation failpoints do not fire on it.
    crate::store::atomic_write(&marker, b"1\n")?;
    set_private_permissions(&marker)?;
    sync_dir(txn_dir)?;
    if let Some(parent) = txn_dir.parent() {
        sync_dir(parent)?;
    }
    Ok(())
}

fn restore_before_image(
    plan_dir: &Path,
    txn_dir: &Path,
    baseline: &BTreeSet<PathBuf>,
    baseline_dirs: &BTreeSet<PathBuf>,
) -> Result<()> {
    let (current_files, mut current_dirs) = collect_plan_tree(plan_dir)?;
    for rel in current_files {
        if !baseline.contains(&rel) {
            let path = plan_dir.join(&rel);
            reject_symlink(&path)?;
            fs::remove_file(&path)
                .with_context(|| format!("remove transaction-created file {}", path.display()))?;
        }
    }
    for rel in baseline {
        let src = txn_dir.join("before").join(rel);
        let dest = plan_dir.join(rel);
        reject_symlink_components(&txn_dir.join("before"), rel)?;
        reject_symlink_components(plan_dir, rel)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = fs::read(&src)
            .with_context(|| format!("read transaction before-image {}", src.display()))?;
        crate::store::atomic_write(&dest, bytes)
            .with_context(|| format!("restore transaction file {}", dest.display()))?;
    }
    current_dirs.retain(|dir| !baseline_dirs.contains(dir));
    let mut created_dirs: Vec<_> = current_dirs.into_iter().collect();
    created_dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for rel in created_dirs {
        let path = plan_dir.join(rel);
        match fs::remove_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(error) => return Err(error.into()),
        }
    }
    sync_dir(plan_dir)
}

fn collect_plan_tree(plan_dir: &Path) -> Result<(BTreeSet<PathBuf>, BTreeSet<PathBuf>)> {
    let mut files = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    collect_dir(plan_dir, plan_dir, &mut files, &mut dirs)?;
    Ok((files, dirs))
}

fn collect_dir(
    root: &Path,
    dir: &Path,
    files: &mut BTreeSet<PathBuf>,
    dirs: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    reject_symlink(dir)?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(root).context("path escaped plan root")?;
        if rel.components().next() == Some(Component::Normal(TXN_DIR.as_ref()))
            || rel == Path::new(".mp-write.lock")
        {
            continue;
        }
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            bail!("transaction refuses symlink {}", path.display());
        }
        if ty.is_dir() {
            dirs.insert(rel.to_path_buf());
            collect_dir(root, &path, files, dirs)?;
        } else if ty.is_file() {
            files.insert(rel.to_path_buf());
        }
    }
    Ok(())
}

fn validate_relative(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.len() > 4096 {
        bail!("invalid recovery path length");
    }
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("invalid recovery path {value:?}");
    }
    Ok(path)
}

fn relative_string(path: &Path) -> Result<String> {
    let value = path
        .to_str()
        .context("transaction path is not valid UTF-8")?;
    validate_relative(value)?;
    Ok(value.to_string())
}

fn reject_symlink(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        bail!("transaction refuses symlink {}", path.display());
    }
    Ok(())
}

fn reject_symlink_components(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    reject_symlink(&current)?;
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            bail!("invalid transaction path {}", relative.display());
        };
        current.push(segment);
        if current.exists() {
            reject_symlink(&current)?;
        }
    }
    Ok(())
}

fn sync_file(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn set_private_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn set_private_dir_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
