use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::assets;
use crate::doctor;
use crate::harness;

const MIN_ARTIFACT_SIZE: u64 = 1;

/// M158: file names the skill deployer never copies to a harness's
/// destination skill dir. `manifest.json` is install-time metadata
/// re-read from source at every install and must not shadow the
/// source-of-truth on disk. `.DS_Store` / `Thumbs.db` are unrelated
/// to skill content and silently created by Finder / Explorer on macOS
/// and Windows respectively.
///
/// F-20 external review (cross-platform junk): `desktop.ini`
/// (Windows folder metadata, dropped by Explorer into any folder the
/// user opens via SMB share) belongs here too — Windows-authored skill
/// sources leak it on every save. We deliberately do NOT match
/// arbitrary leading-dotfiles at the top level; only the three
/// well-known OS-metadata filenames (`manifest.json` / `.DS_Store` /
/// `desktop.ini`) plus the Windows sibling `Thumbs.db`. Hidden files
/// in subdirectories are also filtered, see [`is_skill_deploy_skipped`].
const SKILL_DEPLOY_SKIP: &[&str] = &["manifest.json", ".DS_Store", "Thumbs.db", "desktop.ini"];

/// M158: file-name suffixes the skill deployer never copies. Covers
/// editor junk (Vim swap files `*.swp`, Vim swap-continuation
/// `*.swo`, Emacs backups `*~`) that may leak into skill source
/// directories during development.
///
/// F-20 external review (other-editor junk): Vim writes `.swp` and
/// `.swo` as a paired swap file/continuation set; the original
/// filter caught `.swp` and missed `.swo`. Same case-insensitivity
/// rule applies.
///
/// M158 round 2 (L-C-8): matched case-insensitively on the trailing
/// ASCII bytes. On macOS HFS+/APFS case-insensitive mounts, a
/// `foo.SWP` (uppercase) should also be skipped — otherwise the
/// skip filter would let it through. The locale-aware `to_lowercase`
/// form is intentionally not used; ASCII-only keeps the comparison
/// allocation-free and predictable on every platform.
const SKILL_DEPLOY_SKIP_SUFFIX: &[&str] = &[".swp", ".swo", "~"];

fn is_skill_deploy_skipped(name: &str) -> bool {
    // F-20 external review: an arbitrary leading-dotfile in a
    // subdirectory (`.hidden`, `.env`, `.gitignore` accidentally
    // landed in the source dir during development, `.#foo.md`
    // Emacs lock files, etc.) is almost certainly editor or
    // version-control metadata, not skill content. Filter them
    // uniformly — the cost is one byte of inspection per entry.
    if name.starts_with('.') {
        return true;
    }
    if SKILL_DEPLOY_SKIP.contains(&name) {
        return true;
    }
    SKILL_DEPLOY_SKIP_SUFFIX.iter().any(|s| {
        name.len() >= s.len()
            && name
                .as_bytes()
                .iter()
                .rev()
                .zip(s.as_bytes().iter().rev())
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub id: String,
    pub display: String,
    #[serde(default)]
    pub consumes: Vec<String>,
    #[serde(default)]
    pub min_mp_version: Option<String>,
    /// M146: `core` (default-deployed), `catalog` (opt-in via --skills),
    /// or `internal` (repo-only; never deployable via `mp install`).
    /// Default to `core` for backward compat with pre-M146 manifests.
    #[serde(default = "default_skill_category")]
    pub category: String,
    /// M146: upstream source identifier (e.g. `mattpocock/skills`).
    /// Catalog-only — core skills leave it empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// M146: upstream source URL (informational).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_url: String,
    /// M146: upstream version pin (informational; not enforced).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub upstream_version: String,
}

fn default_skill_category() -> String {
    "core".to_string()
}

impl SkillManifest {
    pub fn is_core(&self) -> bool {
        self.category == "core"
    }
    pub fn is_catalog(&self) -> bool {
        self.category == "catalog"
    }
    /// Repo-only skills (e.g. mp-code-review). Never selected by bare
    /// `mp install` or `mp install --skills=…`.
    pub fn is_internal(&self) -> bool {
        self.category == "internal"
    }
    pub fn is_deployable(&self) -> bool {
        self.is_core() || self.is_catalog()
    }
}

// ── M146: deployment manifest ──────────────────────────────────────────────

/// M146: an entry in `installed-skills.json` recording one deployed
/// skill with full provenance (skill id, harness, category, source,
/// upstream version, installed_at).
///
/// M158 round 2: `installed_path` records the resolved destination
/// directory at install time (e.g. `<MP_OPENCODE_SKILL_DIR>/<id>` or
/// `~/.agents/skills/<id>`). `mp install --check` compares the on-disk
/// state against THIS path, not against the current
/// `harness::resolved_global_skill_dir(h)` — otherwise a check run
/// with a different `MP_*_SKILL_DIR` would falsely report every
/// file as drift. Empty on entries created by pre-M158 installs;
/// callers fall back to the harness's current resolver in that
/// case (M-C-2 self-finding fix).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledSkill {
    pub skill_id: String,
    pub harness: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub upstream_version: String,
    pub installed_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub installed_path: String,
    /// Canonical skills root opened during uninstall. New manifests pair this
    /// with `artifact_path`; `installed_path` remains for read compatibility.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub harness_root: String,
    /// Validated path relative to `harness_root` (currently exactly skill_id).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artifact_path: String,
}

/// M146: the on-disk deployment manifest. Lives at
/// `<MP_HOME>/installed-skills.json` (i.e. environment config, not
/// plan data). Read-modify-written atomically by install, read by
/// list/check/uninstall.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledSkillsManifest {
    pub entries: Vec<InstalledSkill>,
}

impl InstalledSkillsManifest {
    pub fn path() -> PathBuf {
        install_dir().join("installed-skills.json")
    }

    /// Load the deployment manifest.
    ///
    /// Returns an empty manifest when the file is missing (first install
    /// on a fresh MP_HOME). A present-but-unparseable file is a real
    /// error — silently resetting it would lose deployment provenance
    /// (M146 F-04 external review). Callers that want lenient behavior
    /// can opt in via `load_lenient`.
    pub fn load() -> Result<Self> {
        let path = Self::path();
        match fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).with_context(|| {
                format!(
                    "deployment manifest at {} is present but unparseable; \
                     remove it explicitly if you intended to reset deployment provenance",
                    path.display()
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(anyhow::Error::new(e)
                .context(format!("read deployment manifest {}", path.display()))),
        }
    }

    /// Lenient load: missing file → empty manifest; parse error → empty
    /// manifest. Reserved for read-only surfaces (`--list-skills`,
    /// `--check` drift) where a corrupt manifest shouldn't block the
    /// command. Mutating paths (`install`, `uninstall`) use `load()`.
    pub fn load_lenient() -> Self {
        Self::load().unwrap_or_default()
    }

    /// In-memory dedup-replace on (skill_id, harness). Does NOT
    /// persist; pair with `save()` for a single atomic write after a
    /// batch of updates (M146 F-03 external review: the original
    /// `record()` saved on every call, which made install perform N×M
    /// atomic writes inside the harness×skill loop).
    pub fn record_entry(&mut self, entry: InstalledSkill) {
        if let Some(slot) = self
            .entries
            .iter_mut()
            .find(|e| e.skill_id == entry.skill_id && e.harness == entry.harness)
        {
            *slot = entry;
        } else {
            self.entries.push(entry);
        }
    }

    /// Atomic read-modify-write: dedupes on (skill_id, harness),
    /// replaces existing entries, persists via the existing
    /// `store::atomic_write` so a crash mid-write doesn't leave a
    /// half-written manifest. Prefer `record_entry` + `save()` inside
    /// loops to avoid one disk write per entry.
    pub fn record(&mut self, entry: InstalledSkill) -> Result<()> {
        self.record_entry(entry);
        self.save()
    }

    /// In-memory remove on (skill_id, harness); does NOT persist.
    /// Pair with `save()`. See `forget` for the persisting variant.
    pub fn forget_entry(&mut self, skill_id: &str, harness: &str) -> Vec<InstalledSkill> {
        let removed: Vec<InstalledSkill> = self
            .entries
            .iter()
            .filter(|e| e.skill_id == skill_id && e.harness == harness)
            .cloned()
            .collect();
        self.entries
            .retain(|e| !(e.skill_id == skill_id && e.harness == harness));
        removed
    }

    /// Remove entries whose (skill_id, harness) matches; persist once.
    /// Returns the list of removed entries so the caller can also
    /// remove the corresponding skill dirs.
    pub fn forget(&mut self, skill_id: &str, harness: &str) -> Result<Vec<InstalledSkill>> {
        let before = self.entries.len();
        let removed = self.forget_entry(skill_id, harness);
        if self.entries.len() != before {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(self)
            .with_context(|| format!("serialize {}", path.display()))?;
        crate::store::atomic_write(&path, format!("{body}\n"))
            .with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    /// F-22 external review (legacy installed_path gap): walk every
    /// entry; for any entry missing `installed_path`, capture the
    /// harness's CURRENT resolver path so subsequent `mp install --check`
    /// never falls back to the env-drift false-positive path. The
    /// backfill is a no-op for entries written post-M158 (their
    /// `installed_path` is already populated at install time). The
    /// function persists the manifest once, even if no entry was
    /// backfilled, so the atomic-write contract is honored and a
    /// crash mid-backfill leaves a recoverable on-disk state (the
    /// next run retries the backfill harmlessly). See the docstring
    /// at the install-site for the env-drift trade-off
    /// (`MP_*_SKILL_DIR` at backfill differs from the path used at
    /// the original install surfaces a "directory missing" warning
    /// on the next `--check` — that's deliberate).
    pub fn backfill_legacy_installed_paths(&mut self) -> Result<usize> {
        let mut changed = 0usize;
        for entry in self.entries.iter_mut() {
            let Some(h) = harness::harness_by_id(&entry.harness) else {
                continue; // unknown harness — leave legacy state untouched
            };
            if entry.installed_path.is_empty() {
                entry.installed_path = harness::skill_dir_for(&h, &entry.skill_id)
                    .to_string_lossy()
                    .to_string();
                changed += 1;
            }
            if entry.harness_root.is_empty() || entry.artifact_path.is_empty() {
                let root = harness::resolved_global_skill_dir(&h);
                if let Ok(canonical_root) = root.canonicalize() {
                    entry.harness_root = canonical_root.to_string_lossy().to_string();
                    entry.artifact_path = entry.skill_id.clone();
                    changed += 1;
                }
            }
        }
        if changed > 0 {
            self.save()?;
        }
        Ok(changed)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillRegistry {
    pub skills: Vec<SkillManifest>,
    pub source_root: PathBuf,
}

/// Reject skill ids that could escape a skills root when joined as a
/// path segment (`../`, separators, NUL, leading `.`, empty / `.` / `..`).
pub fn validate_skill_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains('/')
        || id.contains('\\')
        || id.contains('\0')
        || id.starts_with('.')
    {
        bail!("invalid skill id: {id:?}");
    }
    Ok(())
}

#[derive(Debug)]
struct UninstallTarget {
    root: PathBuf,
    relative: PathBuf,
    harness_id: String,
}

/// Resolve a manifest row into an opened-root deletion target. New rows carry
/// canonical `harness_root` + relative `artifact_path`. Legacy rows are accepted
/// only when their absolute `installed_path` is exactly the current canonical
/// harness root joined with the validated skill id.
fn skill_dir_for_uninstall(entry: &InstalledSkill) -> Result<Option<UninstallTarget>> {
    validate_skill_id(&entry.skill_id).with_context(|| {
        format!(
            "refusing to uninstall skill with invalid id {:?}",
            entry.skill_id
        )
    })?;
    let Some(h) = harness::harness_by_id(&entry.harness) else {
        return Ok(None);
    };
    let (root, relative) = if !entry.harness_root.is_empty() || !entry.artifact_path.is_empty() {
        if entry.harness_root.is_empty() || entry.artifact_path.is_empty() {
            bail!(
                "incomplete uninstall containment metadata for {}/{}",
                entry.harness,
                entry.skill_id
            );
        }
        let relative = PathBuf::from(&entry.artifact_path);
        ensure_safe_relative_artifact(&relative, &entry.skill_id)?;
        (PathBuf::from(&entry.harness_root), relative)
    } else {
        let current_root = harness::resolved_global_skill_dir(&h);
        let canonical_root = current_root.canonicalize().with_context(|| {
            format!(
                "cannot establish canonical harness root for legacy uninstall: {}",
                current_root.display()
            )
        })?;
        let recorded = if entry.installed_path.is_empty() {
            harness::skill_dir_for(&h, &entry.skill_id)
        } else {
            PathBuf::from(&entry.installed_path)
        };
        let expected = canonical_root.join(&entry.skill_id);
        if recorded != expected && recorded.canonicalize().ok().as_ref() != Some(&expected) {
            bail!(
                "legacy installed path {} is not contained at {}/{}; reinstall to refresh containment metadata",
                recorded.display(),
                canonical_root.display(),
                entry.skill_id
            );
        }
        (canonical_root, PathBuf::from(&entry.skill_id))
    };
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("canonicalize harness root {}", root.display()))?;
    if canonical_root != root {
        bail!(
            "recorded harness root is not canonical: {} (canonical {})",
            root.display(),
            canonical_root.display()
        );
    }
    let configured_root = harness::resolved_global_skill_dir(&h)
        .canonicalize()
        .with_context(|| {
            format!(
                "canonicalize configured harness root {}",
                harness::resolved_global_skill_dir(&h).display()
            )
        })?;
    if canonical_root != configured_root {
        bail!(
            "recorded harness root {} does not match configured canonical root {}; \
             update the harness root explicitly before uninstall",
            canonical_root.display(),
            configured_root.display()
        );
    }
    Ok(Some(UninstallTarget {
        root: canonical_root,
        relative,
        harness_id: h.id.to_string(),
    }))
}

fn ensure_safe_relative_artifact(path: &Path, skill_id: &str) -> Result<()> {
    use std::path::Component;
    let components: Vec<_> = path.components().collect();
    if components.len() != 1 || !matches!(components[0], Component::Normal(_)) || path.is_absolute()
    {
        bail!(
            "refusing non-relative or nested installed artifact path: {}",
            path.display()
        );
    }
    match path.file_name().and_then(|n| n.to_str()) {
        Some(name) if name == skill_id => Ok(()),
        other => bail!(
            "refusing to delete path whose basename ({other:?}) is not skill id {skill_id:?}: {}",
            path.display()
        ),
    }
}

#[cfg(unix)]
fn cstring_component(component: &std::ffi::OsStr) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(component.as_bytes())
        .map_err(|_| anyhow::anyhow!("path component contains NUL: {component:?}"))
}

#[cfg(unix)]
fn open_canonical_dir_no_follow(path: &Path) -> Result<std::os::fd::OwnedFd> {
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::fs::MetadataExt;

    if !path.is_absolute() {
        bail!("harness root must be absolute: {}", path.display());
    }
    let expected = fs::metadata(path)
        .with_context(|| format!("stat canonical harness root {}", path.display()))?;
    let slash = std::ffi::CString::new("/").expect("literal");
    // SAFETY: slash is a valid NUL-terminated path and returned fd ownership
    // transfers immediately into OwnedFd.
    let root_fd = unsafe {
        libc::open(
            slash.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(std::io::Error::last_os_error()).context("open filesystem root");
    }
    // SAFETY: open returned a unique live descriptor.
    let mut current = unsafe { OwnedFd::from_raw_fd(root_fd) };
    for component in path.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        let name = cstring_component(name)?;
        // SAFETY: current is a live directory fd and name is NUL-terminated.
        let next = unsafe {
            libc::openat(
                std::os::fd::AsRawFd::as_raw_fd(&current),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if next < 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!(
                    "open canonical harness-root component without following symlinks: {}",
                    path.display()
                )
            });
        }
        // SAFETY: openat returned a unique live descriptor.
        current = unsafe { OwnedFd::from_raw_fd(next) };
    }

    // Ensure the descriptor still identifies the directory canonicalization
    // inspected. This closes replacement races between canonicalize/stat/open.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: current is live and stat points to initialized writable memory.
    if unsafe { libc::fstat(std::os::fd::AsRawFd::as_raw_fd(&current), &mut stat) } != 0 {
        return Err(std::io::Error::last_os_error()).context("fstat opened harness root");
    }
    if stat.st_dev as u64 != expected.dev() || stat.st_ino as u64 != expected.ino() {
        bail!(
            "harness root changed while opening; refusing uninstall: {}",
            path.display()
        );
    }
    Ok(current)
}

#[cfg(unix)]
fn remove_open_directory_contents(fd: std::os::fd::RawFd) -> Result<()> {
    use std::ffi::CStr;

    // fdopendir owns its descriptor, so duplicate the caller's anchored fd.
    // SAFETY: fd is live; dup returns a distinct descriptor or -1.
    let duplicate = unsafe { libc::dup(fd) };
    if duplicate < 0 {
        return Err(std::io::Error::last_os_error()).context("dup uninstall directory fd");
    }
    // SAFETY: duplicate is a live directory descriptor; ownership transfers to DIR.
    let dir = unsafe { libc::fdopendir(duplicate) };
    if dir.is_null() {
        // fdopendir did not take ownership on failure.
        unsafe { libc::close(duplicate) };
        return Err(std::io::Error::last_os_error()).context("fdopendir uninstall target");
    }
    // dup() shares the directory stream offset with `fd`. Preflight may have
    // advanced that shared offset to EOF, so reset it before enumerating.
    // SAFETY: dir is a live DIR pointer returned by fdopendir.
    unsafe { libc::rewinddir(dir) };

    let result = (|| -> Result<()> {
        loop {
            // SAFETY: dir is live until closed below.
            let entry = unsafe { libc::readdir(dir) };
            if entry.is_null() {
                break;
            }
            // SAFETY: POSIX dirent d_name is NUL-terminated for a successful readdir.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            let mut stat: libc::stat = unsafe { std::mem::zeroed() };
            // SAFETY: fd/name/stat are valid; AT_SYMLINK_NOFOLLOW inspects the entry.
            if unsafe { libc::fstatat(fd, name.as_ptr(), &mut stat, libc::AT_SYMLINK_NOFOLLOW) }
                != 0
            {
                return Err(std::io::Error::last_os_error())
                    .context("inspect uninstall artifact entry");
            }
            if (stat.st_mode & libc::S_IFMT) == libc::S_IFDIR {
                // SAFETY: fd/name are valid and O_NOFOLLOW prevents a raced symlink.
                let child = unsafe {
                    libc::openat(
                        fd,
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                };
                if child < 0 {
                    return Err(std::io::Error::last_os_error())
                        .context("open uninstall child directory without following symlinks");
                }
                let child_result = remove_open_directory_contents(child);
                // SAFETY: child is a live descriptor returned by openat.
                unsafe { libc::close(child) };
                child_result?;
                // SAFETY: child was traversed relative to fd and name is unchanged.
                if unsafe { libc::unlinkat(fd, name.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
                    return Err(std::io::Error::last_os_error())
                        .context("remove uninstall child directory");
                }
            } else {
                // Symlinks and non-directories are removed as entries, never followed.
                // SAFETY: fd/name are valid.
                if unsafe { libc::unlinkat(fd, name.as_ptr(), 0) } != 0 {
                    return Err(std::io::Error::last_os_error())
                        .context("remove uninstall artifact entry");
                }
            }
        }
        Ok(())
    })();
    // SAFETY: dir is live and owns duplicate.
    unsafe { libc::closedir(dir) };
    result
}

#[cfg(unix)]
fn check_directory_delete_access(fd: std::os::fd::RawFd) -> Result<()> {
    let dot = std::ffi::CString::new(".").expect("literal");
    // Deleting children requires write+search permission on every directory.
    // SAFETY: fd is a live directory descriptor and dot is NUL-terminated.
    if unsafe { libc::faccessat(fd, dot.as_ptr(), libc::W_OK | libc::X_OK, 0) } != 0 {
        return Err(std::io::Error::last_os_error())
            .context("uninstall directory is not writable/searchable");
    }
    Ok(())
}

#[cfg(unix)]
fn preflight_directory_removal(fd: std::os::fd::RawFd) -> Result<()> {
    use std::ffi::CStr;

    check_directory_delete_access(fd)?;
    // SAFETY: fd is live; dup returns an independently owned descriptor.
    let duplicate = unsafe { libc::dup(fd) };
    if duplicate < 0 {
        return Err(std::io::Error::last_os_error()).context("dup uninstall preflight fd");
    }
    // SAFETY: ownership of duplicate transfers to DIR on success.
    let dir = unsafe { libc::fdopendir(duplicate) };
    if dir.is_null() {
        unsafe { libc::close(duplicate) };
        return Err(std::io::Error::last_os_error()).context("fdopendir uninstall preflight");
    }
    // A duplicated directory fd shares its stream offset with the anchored fd.
    // Always start this walk at the beginning.
    // SAFETY: dir is a live DIR pointer returned by fdopendir.
    unsafe { libc::rewinddir(dir) };
    let result = (|| -> Result<()> {
        loop {
            // SAFETY: dir remains live until closed below.
            let entry = unsafe { libc::readdir(dir) };
            if entry.is_null() {
                break;
            }
            // SAFETY: successful readdir returns a NUL-terminated d_name.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            let mut stat: libc::stat = unsafe { std::mem::zeroed() };
            // SAFETY: fd/name/stat are valid and symlinks are not followed.
            if unsafe { libc::fstatat(fd, name.as_ptr(), &mut stat, libc::AT_SYMLINK_NOFOLLOW) }
                != 0
            {
                return Err(std::io::Error::last_os_error())
                    .context("inspect uninstall preflight entry");
            }
            if (stat.st_mode & libc::S_IFMT) == libc::S_IFDIR {
                // SAFETY: O_NOFOLLOW prevents replacement with a symlink.
                let child = unsafe {
                    libc::openat(
                        fd,
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                };
                if child < 0 {
                    return Err(std::io::Error::last_os_error())
                        .context("open uninstall preflight directory");
                }
                let child_result = preflight_directory_removal(child);
                unsafe { libc::close(child) };
                child_result?;
            }
        }
        Ok(())
    })();
    // SAFETY: dir is live and owns duplicate.
    unsafe { libc::closedir(dir) };
    result
}

#[cfg(unix)]
fn remove_contained_skill(target: &UninstallTarget, skill_id: &str) -> Result<bool> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    ensure_safe_relative_artifact(&target.relative, skill_id)?;
    let root_fd = open_canonical_dir_no_follow(&target.root)?;
    let name = cstring_component(target.relative.as_os_str())?;
    // SAFETY: anchored root fd/name are valid and O_NOFOLLOW rejects symlinks.
    let child_fd = unsafe {
        libc::openat(
            root_fd.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if child_fd < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::NotFound {
            return Ok(false);
        }
        return Err(error).with_context(|| {
            format!(
                "open installed skill without following symlinks: {}/{}",
                target.root.display(),
                target.relative.display()
            )
        });
    }
    // SAFETY: openat returned a unique live descriptor.
    let child = unsafe { OwnedFd::from_raw_fd(child_fd) };

    // Expected artifact identity: a deployed skill is a real directory with a
    // regular, non-symlink SKILL.md entry. This prevents a forged manifest from
    // deleting an arbitrary same-named directory.
    let skill_md = std::ffi::CString::new("SKILL.md").expect("literal");
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: child is live and skill_md/stat are valid.
    let identity_ok = unsafe {
        libc::fstatat(
            child.as_raw_fd(),
            skill_md.as_ptr(),
            &mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
        && (stat.st_mode & libc::S_IFMT) == libc::S_IFREG;
    if !identity_ok {
        bail!(
            "refusing to remove {}/{}: expected regular SKILL.md identity marker is missing",
            target.root.display(),
            target.relative.display()
        );
    }
    // Check the complete tree before the first unlink so ordinary permission
    // failures leave both the deployment and manifest untouched.
    check_directory_delete_access(root_fd.as_raw_fd())
        .context("harness root is not writable for contained uninstall")?;
    preflight_directory_removal(child.as_raw_fd())
        .context("installed skill tree is not removable")?;
    let removal = remove_open_directory_contents(child.as_raw_fd());
    drop(child);
    removal?;
    // SAFETY: root fd remains anchored and name is the validated one-segment path.
    if unsafe { libc::unlinkat(root_fd.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| {
            format!(
                "remove contained skill directory {}/{}",
                target.root.display(),
                target.relative.display()
            )
        });
    }
    Ok(true)
}

#[cfg(not(unix))]
fn remove_contained_skill(_target: &UninstallTarget, _skill_id: &str) -> Result<bool> {
    bail!("secure descriptor-relative uninstall is unsupported on this platform")
}

impl SkillRegistry {
    pub fn load(source_root: &Path) -> Result<Self> {
        let skills_dir = source_root.join("templates/skills");
        if !skills_dir.is_dir() {
            bail!("skills directory not found at {}", skills_dir.display());
        }
        let mut skills = Vec::new();
        for entry in fs::read_dir(&skills_dir)? {
            let entry = entry?;
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let manifest_path = dir.join("manifest.json");
            if !manifest_path.is_file() {
                continue;
            }
            let content = fs::read_to_string(&manifest_path)
                .with_context(|| format!("failed to read {}", manifest_path.display()))?;
            let manifest: SkillManifest = serde_json::from_str(&content)
                .with_context(|| format!("invalid manifest at {}", manifest_path.display()))?;
            validate_skill_id(&manifest.id)
                .with_context(|| format!("invalid skill id in {}", manifest_path.display()))?;
            // category=internal skills stay on disk for repo maintainers
            // but are excluded from the consumer install registry.
            if manifest.is_internal() {
                continue;
            }
            skills.push(manifest);
        }
        skills.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Self {
            skills,
            source_root: source_root.to_path_buf(),
        })
    }

    pub fn from_embedded() -> Result<Self> {
        let source_root = crate::assets::toolkit_home();
        Self::load(&source_root)
    }

    pub fn skill_ids(&self) -> Vec<&str> {
        self.skills.iter().map(|s| s.id.as_str()).collect()
    }

    pub fn get(&self, id: &str) -> Option<&SkillManifest> {
        self.skills.iter().find(|s| s.id == id)
    }

    pub fn skill_source_path(&self, id: &str) -> PathBuf {
        self.source_root
            .join("templates/skills")
            .join(id)
            .join("SKILL.md")
    }

    /// M158: returns the directory containing every file that ships
    /// with the skill package (SKILL.md plus siblings such as
    /// `flow-stages.md`, `stages.toml`, `scripts/...`). Use this
    /// instead of [`Self::skill_source_path`] when copying more than
    /// just SKILL.md (which is the new normal; the old single-file
    /// shape left every relative link in SKILL.md as a dead reference).
    pub fn skill_source_dir(&self, id: &str) -> PathBuf {
        self.source_root.join("templates/skills").join(id)
    }

    pub fn validate_selection(&self, requested: &[String]) -> Result<Vec<String>> {
        let mut selected = Vec::new();
        let known: HashSet<&str> = self.skill_ids().into_iter().collect();
        let mut unknown = Vec::new();
        for s in requested {
            let s = s.trim();
            if s.is_empty() {
                continue;
            }
            if known.contains(s) {
                selected.push(s.to_string());
            } else {
                unknown.push(s.to_string());
            }
        }
        if !unknown.is_empty() {
            let known_list: Vec<&str> = known.into_iter().collect();
            bail!(
                "unknown skill(s): {}. Registered skills: {}",
                unknown.join(", "),
                known_list.join(", ")
            );
        }
        Ok(selected)
    }
}

#[derive(Debug, Serialize)]
pub struct CheckReport {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub skill_count: usize,
}

/// M146 S4: report shape for `mp install --list-skills`. One entry
/// per registry skill annotated with the deployment state read from
/// the manifest (which harnesses have it).
#[derive(Debug, Serialize)]
pub struct ListSkillsReport {
    pub ok: bool,
    pub skills: Vec<ListSkillsEntry>,
}

#[derive(Debug, Serialize)]
pub struct ListSkillsEntry {
    pub id: String,
    pub display: String,
    pub category: String,
    pub source: String,
    pub source_url: String,
    pub upstream_version: String,
    pub deployed_to: Vec<String>,
}

pub fn list_skills(source_root: &Path) -> Result<ListSkillsReport> {
    let registry = SkillRegistry::load(source_root)?;
    let manifest = InstalledSkillsManifest::load_lenient();
    let mut skills: Vec<ListSkillsEntry> = registry
        .skills
        .iter()
        .map(|s| {
            let deployed_to: Vec<String> = manifest
                .entries
                .iter()
                .filter(|e| e.skill_id == s.id)
                .map(|e| e.harness.clone())
                .collect();
            ListSkillsEntry {
                id: s.id.clone(),
                display: s.display.clone(),
                category: s.category.clone(),
                source: s.source.clone(),
                source_url: s.source_url.clone(),
                upstream_version: s.upstream_version.clone(),
                deployed_to,
            }
        })
        .collect();
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(ListSkillsReport { ok: true, skills })
}

pub fn check_registry(source_root: &Path) -> Result<CheckReport> {
    let registry = SkillRegistry::load(source_root)?;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let manifest = InstalledSkillsManifest::load_lenient();
    // M146 S5: deployment-drift phase. Reports (a) deployed-but-
    // removed-from-registry orphans, (b) registry skills that
    // haven't been deployed yet (advisory — not every registry skill
    // is supposed to deploy by default). Drift findings surface as
    // warnings so the existing `ok=true/false` logic for source-
    // registry validation isn't coupled to deployment state.
    //
    // M146 F-05 (external review): renamed to avoid shadowing the
    // `known_ids: HashSet<&str>` declared below for consumes/Pattern
    // validation. The two sets carry the same ids but different
    // ownership; keeping distinct names makes the data flow legible.
    let known_ids_owned: HashSet<String> = registry.skills.iter().map(|s| s.id.clone()).collect();
    for entry in &manifest.entries {
        if !known_ids_owned.contains(&entry.skill_id) {
            warnings.push(format!(
                "deployed skill '{}/{}' is no longer in the registry (orphan)",
                entry.skill_id, entry.harness
            ));
        }
    }
    for s in &registry.skills {
        let deployed: Vec<&InstalledSkill> = manifest
            .entries
            .iter()
            .filter(|e| e.skill_id == s.id)
            .collect();
        if deployed.is_empty() {
            warnings.push(format!(
                "registry skill '{}' is not deployed anywhere; pass --skills {} to install",
                s.id, s.id
            ));
        }
    }
    let known_ids: HashSet<&str> = registry.skill_ids().into_iter().collect();

    for skill in &registry.skills {
        let skill_path = registry.skill_source_path(&skill.id);
        if !skill_path.is_file() {
            errors.push(format!(
                "registered skill '{}' missing SKILL.md at {}",
                skill.id,
                skill_path.display()
            ));
            continue;
        }
        let size = fs::metadata(&skill_path)
            .with_context(|| format!("failed to read {}", skill_path.display()))?
            .len();
        if size < MIN_ARTIFACT_SIZE {
            errors.push(format!(
                "SKILL.md for '{}' is below minimum size ({} bytes)",
                skill.id, MIN_ARTIFACT_SIZE
            ));
        }

        for consumed in &skill.consumes {
            if !known_ids.contains(consumed.as_str()) {
                errors.push(format!(
                    "skill '{}' declares consumes: '{}' which is not a registered skill",
                    skill.id, consumed
                ));
            }
        }

        // M119 F-01: scan for `Pattern: <token>` lines and assert each
        // referenced id exists in the registry. Convention: a line that
        // starts with `Pattern:` (markdown blockquote / list item prefix
        // is allowed) followed by a registered skill id. mp-flow uses
        // these to reference sub-skill procedures; a stale reference
        // (e.g., `Pattern: mp-mentor` when only `mp-coordinator` /
        // `mp-runner` ship) is a real defect.
        if let Ok(content) = fs::read_to_string(&skill_path) {
            for line in content.lines() {
                let token = line
                    .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '>' | '-' | '*'))
                    .trim_start_matches("Pattern:")
                    .trim();
                if line.trim_start().starts_with("Pattern:")
                    && !token.is_empty()
                    && !known_ids.contains(token)
                {
                    errors.push(format!(
                        "skill '{}' references Pattern: '{}' which is not a registered skill",
                        skill.id, token
                    ));
                }
            }
        }
    }

    let skills_dir = source_root.join("templates/skills");
    if skills_dir.is_dir() {
        for entry in fs::read_dir(&skills_dir)? {
            let entry = entry?;
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let manifest_path = dir.join("manifest.json");
            if !manifest_path.is_file() {
                if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
                    warnings.push(format!(
                        "directory templates/skills/{} has no manifest.json — skipping",
                        name
                    ));
                }
            }
        }
    }

    let ok = errors.is_empty();
    Ok(CheckReport {
        ok,
        errors,
        warnings,
        skill_count: registry.skills.len(),
    })
}

/// M158 AC-07: file-tree drift detector. For each entry in the
/// deployment manifest, walks the source skill directory (recursive)
/// and the recorded `installed_path` directory (recursive, as
/// captured at install time — see `InstalledSkill::installed_path`).
/// Reports every missing or extra file as a drift warning.
///
/// M158 round 2 (M-C-2): uses the manifest's `installed_path` rather
/// than re-resolving the harness's current skill dir. A `check` run
/// with a different `MP_*_SKILL_DIR` than the `install` that wrote
/// the manifest would otherwise report every file as drift — the
/// recorded path is the source of truth for "where was it deployed".
///
/// F-22 external review (legacy installed_path gap): `install()`
/// now lazy-backfills `installed_path` on every entry that lacks it
/// (one entry per legacy manifest row). The fallback path here is
/// retained for entries written before the user's first post-M158
/// `mp install` ran. When the fallback fires, surface it as a
/// distinct warning ("legacy entry — run `mp install` to refresh")
/// so the env-drift false-positive is *visible* — operators can
/// run `mp install` (which now backfills) and the legacy window
/// closes.
///
/// Surfaced via `mp install --check` after every full sibling-bearing
/// install so a hand-removal of a single sibling (or a partial-
/// failure torn-state install) is at least visible, even if
/// `check_registry` itself is happy. Drift findings are warnings, not
/// errors — the registry remains valid; only the on-disk mirror is
/// inconsistent.
pub fn check_deployment_files(source_root: &Path) -> Result<Vec<String>> {
    let manifest = InstalledSkillsManifest::load_lenient();
    let registry = SkillRegistry::load(source_root).unwrap_or_else(|_| SkillRegistry {
        skills: vec![],
        source_root: source_root.to_path_buf(),
    });
    let mut warnings = Vec::new();
    let mut legacy_seen = false;
    for entry in &manifest.entries {
        let Some(h) = harness::harness_by_id(&entry.harness) else {
            continue;
        };
        let src_dir = registry.skill_source_dir(&entry.skill_id);
        // Prefer the recorded installed_path (env-stable across
        // check-time MP_*_SKILL_DIR changes). Fall back to the
        // harness's current resolver for legacy entries; surface the
        // fallback as an explicit warning so it's not silent.
        let dest_dir: PathBuf = if !entry.installed_path.is_empty() {
            PathBuf::from(&entry.installed_path)
        } else {
            legacy_seen = true;
            harness::skill_dir_for(&h, &entry.skill_id)
        };
        collect_skill_drift(
            &src_dir,
            &dest_dir,
            &entry.skill_id,
            &entry.harness,
            &mut warnings,
        );
    }
    if legacy_seen {
        warnings.push(
            "deployment manifest contains legacy entries without installed_path; \
             run `mp install` to backfill and close the M-C-2 env-drift window"
                .to_string(),
        );
    }
    Ok(warnings)
}

fn collect_skill_drift(
    src: &Path,
    dest: &Path,
    skill_id: &str,
    harness_id: &str,
    warnings: &mut Vec<String>,
) {
    if !src.is_dir() {
        return; // source gone — out of scope for deployment drift
    }
    if !dest.is_dir() {
        warnings.push(format!(
            "deployed skill {harness_id}/{skill_id} directory missing at {}",
            dest.display()
        ));
        return;
    }
    let src_entries: std::collections::BTreeMap<String, std::path::PathBuf> =
        match fs::read_dir(src) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
                .collect(),
            Err(_) => return,
        };
    let dest_entries: std::collections::BTreeMap<String, std::path::PathBuf> =
        match fs::read_dir(dest) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
                .collect(),
            Err(_) => return,
        };
    for (name, src_path) in &src_entries {
        if is_skill_deploy_skipped(name) {
            continue;
        }
        match dest_entries.get(name) {
            None => {
                let rel = src_path.strip_prefix(src).unwrap_or(src_path);
                warnings.push(format!(
                    "deployed skill {harness_id}/{skill_id} missing sibling {} (source has it)",
                    rel.display()
                ));
            }
            Some(dest_path) => {
                let src_is_dir = src_path.is_dir();
                let dest_is_dir = dest_path.is_dir();
                if src_is_dir && dest_is_dir {
                    collect_skill_drift(src_path, dest_path, skill_id, harness_id, warnings);
                } else if src_is_dir != dest_is_dir {
                    warnings.push(format!(
                        "deployed skill {harness_id}/{skill_id} sibling {name} type mismatch (src_dir={src_is_dir}, dest_dir={dest_is_dir})"
                    ));
                }
                // Same-type files: content comparison is out of scope; a
                // differing bytes count would imply a corrupted deploy
                // which the read-side verifier already catches.
            }
        }
    }
    for name in dest_entries.keys() {
        if is_skill_deploy_skipped(name) {
            continue;
        }
        if !src_entries.contains_key(name) {
            warnings.push(format!(
                "deployed skill {harness_id}/{skill_id} has stale sibling {name} (not in source; will be cleaned on next install)"
            ));
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InstallReport {
    pub ok: bool,
    pub mp_home: String,
    pub harnesses: Vec<String>,
    pub path_snippet: String,
    pub dev: bool,
    pub doctor: doctor::DoctorReport,
}

pub fn resolve_harness_ids(raw: &[String]) -> Result<Vec<String>> {
    if raw.is_empty() {
        return Ok(vec!["opencode".to_string()]);
    }
    let mut ids = Vec::new();
    for s in raw {
        if s == "both" || s == "all" {
            return Ok(harness::default_registry()
                .into_iter()
                .map(|h| h.id)
                .collect());
        }
        let parts: Vec<&str> = s.split(',').collect();
        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if harness::harness_by_id(part).is_none() {
                bail!(
                    "unknown harness '{part}'; valid: opencode, cursor, claude-code, \
                     gemini, codex, windsurf, cline, pi"
                );
            }
            ids.push(part.to_string());
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

pub fn install(
    harness_ids: &[String],
    _global: bool,
    dev: bool,
    source: Option<&Path>,
    toolkit_only: bool,
    skills_filter: Option<&[String]>,
    agents_filter: Option<&[String]>,
) -> Result<InstallReport> {
    let source_root = resolve_source_root(dev, source)?;
    let install_dir = install_dir();
    let bin_dir = install_dir.join("bin");
    fs::create_dir_all(&bin_dir)?;

    let mp_binary = resolve_mp_binary(&source_root, dev)?;
    let bin_dest = bin_dir.join("mp");
    fs::copy(&mp_binary, &bin_dest)
        .with_context(|| format!("copy mp binary from {}", mp_binary.display()))?;
    codesign_macos_binary(&bin_dest)?;

    let raul_binary = resolve_raul_binary(&source_root, dev)?;
    let raul_dest = bin_dir.join("raul");
    fs::copy(&raul_binary, &raul_dest)
        .with_context(|| format!("copy raul binary from {}", raul_binary.display()))?;
    codesign_macos_binary(&raul_dest)?;

    write_env_snippet(&install_dir)?;

    for dir in ["templates", "schemas", "docs"] {
        let src = source_root.join(dir);
        if src.is_dir() {
            mirror_tree(&src, &install_dir.join(dir))?;
        }
    }

    let registry = SkillRegistry::load(&source_root)?;
    // M146 S2: bare `mp install` deploys only `category=core` skills;
    // catalog skills are opt-in via `--skills=…`. Default selection is
    // derived from the registry (any core skill) so a new core skill
    // added to the catalog gets picked up automatically; pre-M146 hard-
    // coded list was a stop-gap until manifests carried a category.
    let deploy_skills = if let Some(filter) = skills_filter {
        registry.validate_selection(filter)?
    } else {
        registry
            .skills
            .iter()
            .filter(|s| s.is_core())
            .map(|s| s.id.clone())
            .collect()
    };

    if !toolkit_only {
        for skill_id in &deploy_skills {
            let skill_src = registry.skill_source_dir(skill_id);
            if !skill_src.is_dir() {
                bail!(
                    "skill template directory missing at {}",
                    skill_src.display()
                );
            }
            // M158 round 2 (L-C-10): also require a manifest entry
            // for this skill. A directory without manifest.json is
            // silently skipped by `SkillRegistry::load` and would
            // fall through to the "core / empty source" fallback
            // below, recording the deployment with wrong provenance.
            if registry.get(skill_id).is_none() {
                bail!(
                    "skill '{skill_id}' is in templates/skills/ but has no manifest.json — \
                     every deployable skill must ship a manifest.json with at least \
                     id + display + category"
                );
            }
        }
    }

    let mut deployed = Vec::new();
    // M146 S1: read-modify-write the deployment manifest so every
    // deployed skill is recorded with provenance (skill_id, harness,
    // category, source, upstream_version, installed_at). The manifest
    // lives at <MP_HOME>/installed-skills.json (environment config,
    // never plan data). `forget` on uninstall is symmetric to `record`
    // here.
    //
    // M146 F-03 (external review): batch the manifest mutations
    // in-memory via `record_entry` and persist once after the loop.
    // The prior `record()`-per-skill did N×M atomic writes inside the
    // harness×skill loop.
    let mut manifest = InstalledSkillsManifest::load()?;
    // F-22 external review (legacy installed_path gap): on every
    // install run, lazily backfill `installed_path` for any manifest
    // entry that pre-dates M158 (the field didn't exist yet). The
    // fallback path in `check_deployment_files` reproduces the
    // M-C-2 env-drift false positive against legacy entries; backfilling
    // makes the fallback unreachable for any entry the user has
    // touched post-M158. The backfill captures the harness's CURRENT
    // resolver path — if MP_*_SKILL_DIR at backfill differs from the
    // path used at the original install, the next `--check` would
    // surface "directory missing"; that's a deliberate ordering: better
    // to expose the move than to silently drift. The function runs
    // unconditionally (even with `--toolkit-only`) so a no-op install
    // is enough to refresh legacy entries on the user's machine.
    manifest.backfill_legacy_installed_paths()?;
    if !toolkit_only {
        for id in harness_ids {
            let h = harness::harness_by_id(id)
                .ok_or_else(|| anyhow::anyhow!("harness not found: {id}"))?;

            deploy_convention(&h)?;

            for skill_id in &deploy_skills {
                let skill_src = registry.skill_source_dir(skill_id);
                deploy_skill_to_harness(&h, skill_id, &skill_src)?;
                // Record the deployment with full provenance.
                let manifest_data = registry.get(skill_id);
                let (category, source, source_url, upstream_version) = manifest_data
                    .map(|m| {
                        (
                            m.category.clone(),
                            m.source.clone(),
                            m.source_url.clone(),
                            m.upstream_version.clone(),
                        )
                    })
                    .unwrap_or_else(|| {
                        (
                            "core".to_string(),
                            String::new(),
                            String::new(),
                            String::new(),
                        )
                    });
                let harness_root = harness::resolved_global_skill_dir(&h)
                    .canonicalize()
                    .with_context(|| {
                        format!(
                            "canonicalize deployed harness root {}",
                            harness::resolved_global_skill_dir(&h).display()
                        )
                    })?;
                manifest.record_entry(InstalledSkill {
                    skill_id: skill_id.clone(),
                    harness: h.id.clone(),
                    category,
                    source,
                    source_url,
                    upstream_version,
                    installed_at: crate::store::now_rfc3339(),
                    // M158 round 2 (M-C-2): capture the resolved
                    // destination dir at install time so a later
                    // `mp install --check` with a different
                    // MP_*_SKILL_DIR env doesn't falsely report
                    // drift.
                    installed_path: harness::skill_dir_for(&h, skill_id)
                        .to_string_lossy()
                        .to_string(),
                    harness_root: harness_root.to_string_lossy().to_string(),
                    artifact_path: skill_id.clone(),
                });
            }

            // M173 S2: agent deploy. Scans `templates/harness/<id>/agents/`
            // for each agent_id in agents_filter, copies the matching
            // `.md` to the harness's agent dir. Agents are filtered
            // (no "deploy all" mode) — a future core-agent set would
            // need its own registry shape, mirroring the SkillRegistry.
            if let Some(agents) = agents_filter {
                for agent_id in agents {
                    let src_file = source_root
                        .join("templates/harness")
                        .join(&h.id)
                        .join("agents")
                        .join(format!("{agent_id}.md"));
                    if !src_file.is_file() {
                        bail!(
                            "agent template missing at {} — every deployable agent must ship \
                             templates/harness/<harness>/agents/<id>.md",
                            src_file.display()
                        );
                    }
                    let dest = deploy_agent_to_harness(&h, agent_id, &src_file)?;
                    eprintln!("  agent {} deployed to {}", agent_id, dest.display());
                }
            }

            deployed.push(id.clone());
        }
        manifest.save()?;
    }

    verify_installed_artifacts(&install_dir)?;

    let mp_home = install_dir.to_string_lossy().to_string();
    let quoted_home = posix_single_quote(&mp_home);
    let quoted_bin = posix_single_quote(&format!("{mp_home}/bin"));
    let path_snippet = format!("export MP_HOME={quoted_home}\nexport PATH={quoted_bin}:\"$PATH\"");

    let doctor_report = doctor::doctor_install(&install_dir, &deployed);

    Ok(InstallReport {
        ok: doctor_report.ok,
        mp_home,
        harnesses: deployed,
        path_snippet,
        dev,
        doctor: doctor_report,
    })
}

#[derive(Debug, Serialize)]
pub struct UninstallReport {
    pub ok: bool,
    pub removed: Vec<String>,
}

pub fn uninstall(harness_ids: &[String], _global: bool, purge: bool) -> Result<UninstallReport> {
    let mut removed = Vec::new();

    // M146 S6: read the deployment manifest and remove exactly what it
    // claims. Survives registry changes across versions — a skill
    // dropped from the source registry after install (M141's
    // master-planner case) is still pruned because the manifest is
    // the source of truth.
    //
    // M146 F-03 (external review): batch manifest mutations via
    // `forget_entry` and persist once at the end. The prior
    // `forget()`-per-entry saved on every removal.
    let mut manifest = InstalledSkillsManifest::load()?;
    let manifest_entries = manifest.entries.clone();

    if purge {
        // Remove every manifest entry's skill dir across the known
        // harnesses (the manifest already records which harness).
        // Do this before deleting the toolkit root: test and custom
        // configurations may place harness roots beneath MP_INSTALL_DIR, and
        // hardened uninstall must canonicalize/open each recorded root while
        // it still exists.
        // F-05/F-06: validate skill_id + prefer installed_path.
        for entry in &manifest_entries {
            if let Some(target) = skill_dir_for_uninstall(entry)? {
                if remove_contained_skill(&target, &entry.skill_id)? {
                    removed.push(format!(
                        "skill:{}:{}/{}",
                        target.harness_id,
                        target.root.display(),
                        target.relative.display()
                    ));
                }
            }
            manifest.forget_entry(&entry.skill_id, &entry.harness);
            manifest.save()?;
        }
        for h in harness::default_registry() {
            let convention_path = harness::convention_path(&h);
            if convention_path.is_file() {
                fs::remove_file(&convention_path)?;
                removed.push(format!("convention:{}:{}", h.id, convention_path.display()));
            }
        }
        for (label, path) in [("toolkit", install_dir())] {
            if path.exists() {
                if path.is_dir() {
                    fs::remove_dir_all(&path)?;
                } else {
                    fs::remove_file(&path)?;
                }
                removed.push(format!("{label}:{}", path.display()));
            }
        }
    } else {
        for id in harness_ids {
            let h = harness::harness_by_id(id).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown harness '{id}'; valid: opencode, cursor, claude-code, \
                     gemini, codex, windsurf, cline, pi"
                )
            })?;
            // Only remove the skill dirs that the manifest says this
            // harness was given — never re-derive from the source
            // registry. F-05/F-06: validate skill_id + prefer installed_path.
            for entry in &manifest_entries {
                if entry.harness != h.id {
                    continue;
                }
                if let Some(target) = skill_dir_for_uninstall(entry)? {
                    if remove_contained_skill(&target, &entry.skill_id)? {
                        removed.push(format!(
                            "skill:{}/{}/{}",
                            id,
                            target.root.display(),
                            target.relative.display()
                        ));
                    }
                }
                manifest.forget_entry(&entry.skill_id, &entry.harness);
                manifest.save()?;
            }
            let convention_path = harness::convention_path(&h);
            if convention_path.is_file() {
                fs::remove_file(&convention_path)?;
                removed.push(format!("convention:{}:{}", id, convention_path.display()));
            }
        }
        let bin_dir = install_dir().join("bin");
        if bin_dir.is_dir()
            && (harness_ids.is_empty() || harness_ids.len() == harness::default_registry().len())
        {
            for entry in ["mp", "raul"] {
                let p = bin_dir.join(entry);
                if p.is_file() {
                    fs::remove_file(&p)?;
                    removed.push(format!("bin:{}", p.display()));
                }
            }
        }
        let env_path = install_dir().join("env.sh");
        if env_path.is_file() {
            fs::remove_file(&env_path)?;
            removed.push(format!("env:{}", env_path.display()));
        }
    }

    Ok(UninstallReport { ok: true, removed })
}

fn deploy_convention(h: &harness::HarnessDescriptor) -> Result<()> {
    let convention_path = harness::convention_path(h);
    if let Some(parent) = convention_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = assets::embedded_asset("templates/AGENTS-TEMPLATE.md")
        .unwrap_or("# Master Plan — Agent Instructions\n");
    crate::store::atomic_write(&convention_path, content)?;
    verify_single_artifact(&convention_path)?;
    Ok(())
}

fn deploy_skill_to_harness(
    h: &harness::HarnessDescriptor,
    skill_name: &str,
    skill_src_dir: &Path,
) -> Result<bool> {
    validate_skill_id(skill_name)?;
    let skills_root = harness::resolved_global_skill_dir(h);
    let skill_dir = skills_root.join(skill_name);
    if !skill_dir.starts_with(&skills_root) {
        bail!("invalid skill id (path escapes skills root): {skill_name:?}");
    }
    if !skill_src_dir.is_dir() {
        bail!(
            "skill template directory missing at {}",
            skill_src_dir.display()
        );
    }
    // M158: wipe-and-rewrite the destination so stale siblings from a
    // previous upstream version (e.g. a removed `atomic-writes.md`)
    // don't linger across reinstalls. Each file is then written via
    // `store::atomic_write` so a SIGINT mid-deploy leaves a torn
    // destination that the next install heals, never a partial write.
    if skill_dir.exists() {
        fs::remove_dir_all(&skill_dir)
            .with_context(|| format!("remove stale destination {}", skill_dir.display()))?;
    }
    fs::create_dir_all(&skill_dir)?;
    deploy_skill_files(skill_src_dir, &skill_dir)?;
    Ok(true)
}

/// M173 S2: deploy a single agent file to a harness's agent profile dir.
///
/// The source layout is `templates/harness/<harness>/agents/<id>.md`.
/// The destination is `<agent_profile_dir>/<id>.md` for the harness.
/// The agent file is read and written atomically; if the destination
/// exists from a prior install, it is overwritten. There is no
/// directory scaffolding — agents are single files (no siblings), so
/// the deploy is one file in / one file out.
///
/// The convention is mirrored from existing skills but agents are
/// coarser-grained: each harness gets its own copy of an agent file
/// so portability metadata (which targets accept this agent) can be
/// per-harness if a future iteration needs it.
fn deploy_agent_to_harness(
    h: &harness::HarnessDescriptor,
    agent_id: &str,
    src_file: &Path,
) -> Result<PathBuf> {
    validate_skill_id(agent_id)?;
    let agents_root = harness::resolved_agent_dir(h);
    fs::create_dir_all(&agents_root)
        .with_context(|| format!("create agents dir {}", agents_root.display()))?;
    let dest = agents_root.join(format!("{agent_id}.md"));
    let bytes =
        fs::read(src_file).with_context(|| format!("read agent source {}", src_file.display()))?;
    crate::store::atomic_write(&dest, &bytes)?;
    Ok(dest)
}

/// M158: copy every file under `src` (recursive, with subdirectory
/// support) into `dest`. Skips files matched by [`is_skill_deploy_skipped`]
/// (manifest.json install-time metadata + OS/editor junk). Each file is
/// written via `store::atomic_write` to preserve the M113 atomic-write
/// contract — no raw `fs::copy` race window. Mode bits are copied from
/// the source so executable scripts (e.g. diagnosing-bugs' template)
/// stay executable on the destination.
fn deploy_skill_files(src: &Path, dest: &Path) -> Result<()> {
    for entry in
        fs::read_dir(src).with_context(|| format!("read source skill dir {}", src.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if is_skill_deploy_skipped(&name) {
            continue;
        }
        let entry_src = entry.path();
        let entry_dest = dest.join(&file_name);
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file_type for {}", entry_src.display()))?;
        if file_type.is_dir() {
            fs::create_dir_all(&entry_dest)
                .with_context(|| format!("create subdir {}", entry_dest.display()))?;
            deploy_skill_files(&entry_src, &entry_dest)?;
        } else if file_type.is_symlink() {
            // Symlinks in source are skipped (M158 design decision:
            // no current skill ships a symlink; revisit if a future
            // skill needs one to share content between skills).
            continue;
        } else {
            let bytes =
                fs::read(&entry_src).with_context(|| format!("read {}", entry_src.display()))?;
            crate::store::atomic_write(&entry_dest, &bytes)?;
            // Preserve the source file's mode bits (esp. +x for
            // scripts). Best-effort on non-unix FS where
            // set_permissions is a no-op or fails; the deploy must not
            // fail because of a permission bit the destination cannot
            // represent. On unix we stat once and pass the metadata
            // through; on non-unix we don't even open the meta handle
            // (F-24 external review: the prior `let _ = &meta;` was a
            // dead-reference trick to silence the unused-warning linter
            // — folding the metadata acquisition into the unix branch
            // removes the suppression without changing behavior).
            #[cfg(unix)]
            {
                if let Ok(meta) = fs::metadata(&entry_src) {
                    let _ = fs::set_permissions(&entry_dest, meta.permissions());
                }
            }
        }
    }
    Ok(())
}

/// M158: variant of [`deploy_skill_files`] used by the project-local
/// install path. Mirrors the existing per-file symlink-on-unix /
/// copy-on-non-unix semantics from M119 — a project-local install is
/// expected to track source via symlink, while a global install copies
/// the bytes (no source tracking). Per-file (not whole-dir) symlinks
/// preserve the existing per-file `symlink_metadata` stale-detection
/// logic.
fn deploy_skill_files_per_file_symlink(src: &Path, dest: &Path) -> Result<()> {
    for entry in
        fs::read_dir(src).with_context(|| format!("read source skill dir {}", src.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if is_skill_deploy_skipped(&name) {
            continue;
        }
        let entry_src = entry.path();
        let entry_dest = dest.join(&file_name);
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file_type for {}", entry_src.display()))?;
        if file_type.is_dir() {
            fs::create_dir_all(&entry_dest)
                .with_context(|| format!("create subdir {}", entry_dest.display()))?;
            deploy_skill_files_per_file_symlink(&entry_src, &entry_dest)?;
        } else if file_type.is_symlink() {
            continue;
        } else {
            // Use symlink_metadata so a broken symlink (existing
            // destination whose target is missing) is detected and
            // removed — `dest.exists()` would follow the symlink and
            // return false, leaving the dangling link in place.
            if let Ok(meta) = fs::symlink_metadata(&entry_dest) {
                if meta.file_type().is_symlink() || meta.is_file() {
                    fs::remove_file(&entry_dest)?;
                }
            }
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&entry_src, &entry_dest)?;
            }
            #[cfg(not(unix))]
            {
                fs::copy(&entry_src, &entry_dest)?;
            }
        }
    }
    Ok(())
}

/// M158 round 2 (M-C-6): per-file wipe for `install_project_skill`.
/// Walks `src` and `dest` in parallel; deploys every source entry
/// (same as `deploy_skill_files_per_file_symlink`) and removes any
/// dest entry that is NOT in source AND looks mp-owned (matches the
/// skip filter's complement — anything we'd consider a skill file).
///
/// Pre-existing user files at the destination survive a re-init
/// because they aren't in source. Stale mp-owned siblings (a file
/// we deployed in an earlier run that the source has since removed)
/// are cleaned. This replaces the M158 whole-dir wipe which
/// silently deleted user files.
fn sync_skill_dir_per_file_symlink(src: &Path, dest: &Path) -> Result<()> {
    // Pass 1: prune dest entries that are mp-owned siblings but no
    // longer in source. Use symlink_metadata so a dangling symlink
    // is still detected (and removed).
    if dest.is_dir() {
        for entry in
            fs::read_dir(dest).with_context(|| format!("read dest skill dir {}", dest.display()))?
        {
            let entry = entry?;
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if is_skill_deploy_skipped(&name) {
                continue;
            }
            let entry_src = src.join(&file_name);
            let entry_dest = entry.path();
            let dest_type = entry.file_type()?;
            if dest_type.is_symlink() {
                // A symlink at dest — only remove if the corresponding
                // source entry exists and isn't a symlink. If source
                // doesn't have this entry at all, it was a stale
                // sibling from a prior install — remove it.
                if !entry_src.exists() {
                    fs::remove_file(&entry_dest)?;
                }
                continue;
            }
            if dest_type.is_dir() {
                if !entry_src.is_dir() {
                    // Source no longer has this dir (or has it as a
                    // file). Whole-dir remove is safe here because
                    // we know dest is mp-owned: a sibling of a skill
                    // we deployed, not a user file (user files at
                    // <project>/.opencode/skills/<id>/ are flat files
                    // by convention, not nested dirs).
                    fs::remove_dir_all(&entry_dest)?;
                } else {
                    sync_skill_dir_per_file_symlink(&entry_src, &entry_dest)?;
                }
                continue;
            }
            // Regular file at dest. If source doesn't have it, it's a
            // stale mp-owned sibling or a user file. We can't tell
            // them apart by extension alone, so we conservatively
            // KEEP any file at dest that isn't in source — the
            // trade-off is that removing upstream skills leaves their
            // previously-deployed files at the project-local dest.
            // The global install path uses whole-dir wipe for that;
            // here we trade absolute cleanup for user-file safety.
            if !entry_src.exists() {
                continue;
            }
        }
    }
    // Pass 2: deploy every source entry (same as the per-file
    // symlink walk). Files already in place are untouched.
    deploy_skill_files_per_file_symlink(src, dest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSkillHarness {
    Cursor,
    Opencode,
}

/// Install the full CPD skill set (every registered skill) into a single
/// harness's project-local skill dir. Returns the list of paths
/// installed (one per skill id).
///
/// Layout: `<project_root>/.{harness}/skills/<skill_id>/SKILL.md` —
/// each skill id gets its own subdir, mirroring the global install
/// layout. The harness's `project_skill_dir` is the parent path; the
/// skill id is appended per skill.
pub fn install_project_skill(
    project_root: &Path,
    harness: ProjectSkillHarness,
) -> Result<Vec<String>> {
    let harness_id = match harness {
        ProjectSkillHarness::Cursor => "cursor",
        ProjectSkillHarness::Opencode => "opencode",
    };
    let h = harness::harness_by_id(harness_id)
        .ok_or_else(|| anyhow::anyhow!("harness not registered: {harness_id}"))?;
    let skills_root = match &h.project_skill_dir {
        Some(rel) => project_root.join(rel),
        None => project_root.join(format!(".{harness_id}/skills")),
    };
    let registry = SkillRegistry::from_embedded().unwrap_or_else(|_| SkillRegistry {
        skills: vec![],
        source_root: PathBuf::from("."),
    });
    let mut installed = Vec::new();
    // M146 consistency: deploy only `category=core` skills, matching what
    // `mp install` (global) deploys by default. Catalog skills (spec-grill,
    // codebase-design, diagnosing-bugs) are opt-in via `mp install --skills`,
    // not auto-deployed into every `mp init --with-*-skill` project.
    for skill in registry.skills.iter().filter(|s| s.is_core()) {
        // Skill ids come from the on-disk manifests and land as path
        // segments under the project's skill root. Reject anything
        // that could escape the project dir (path traversal).
        validate_skill_id(&skill.id)?;
        let skill_src_dir = registry.skill_source_dir(&skill.id);
        if !skill_src_dir.is_dir() {
            bail!(
                "skill template directory missing at {}",
                skill_src_dir.display()
            );
        }
        let dest_dir = skills_root.join(&skill.id);
        // Belt-and-braces: verify the joined path stays inside
        // `skills_root` (catches any remaining traversal case the
        // lexical rules above miss).
        if !dest_dir.starts_with(&skills_root) {
            bail!(
                "invalid skill id (path escapes skills root): {:?}",
                skill.id
            );
        }
        fs::create_dir_all(&dest_dir)?;
        // M158 round 2 (M-C-6): per-file wipe instead of whole-dir
        // wipe. The previous M158 deploy did `fs::remove_dir_all`
        // on the whole destination, which silently deleted any
        // pre-existing user files at `.opencode/skills/<id>/...`.
        // Smart-wipe: enumerate source and dest together; remove
        // only dest entries that are mp-owned siblings (i.e. were
        // deployed by an earlier run of `install_project_skill` and
        // are no longer in source). User files outside the mp-owned
        // set survive a re-init.
        sync_skill_dir_per_file_symlink(&skill_src_dir, &dest_dir)?;
        installed.push(dest_dir.to_string_lossy().to_string());
    }
    Ok(installed)
}

fn verify_installed_artifacts(dir: &Path) -> Result<()> {
    walk_verify_artifacts(dir, dir)
}

fn walk_verify_artifacts(root: &Path, current: &Path) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_verify_artifacts(root, &path)?;
        } else {
            let size = fs::metadata(&path)?.len();
            let rel = path.strip_prefix(root).unwrap_or(&path);
            if size < MIN_ARTIFACT_SIZE {
                bail!(
                    "installed artifact is below minimum size ({} bytes): {}",
                    MIN_ARTIFACT_SIZE,
                    rel.display()
                );
            }
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                toml::from_str::<toml::Value>(&content).with_context(|| {
                    format!("installed TOML artifact is unparseable: {}", rel.display())
                })?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                serde_json::from_str::<serde_json::Value>(&content).with_context(|| {
                    format!("installed JSON artifact is unparseable: {}", rel.display())
                })?;
            }
        }
    }
    Ok(())
}

fn verify_single_artifact(path: &Path) -> Result<()> {
    let size = fs::metadata(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .len();
    if size < MIN_ARTIFACT_SIZE {
        bail!(
            "installed artifact is below minimum size ({} bytes): {}",
            MIN_ARTIFACT_SIZE,
            path.display()
        );
    }
    Ok(())
}

fn resolve_source_root(dev: bool, source: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = source {
        if path.is_file() {
            bail!(
                "--source must be a master-plan repo root, not a binary path (got {})",
                path.display()
            );
        }
        if !path.join("templates").is_dir() {
            bail!("--source must be a master-plan repo root (templates/ missing)");
        }
        return Ok(path.to_path_buf());
    }
    if dev {
        let home = assets::toolkit_home();
        if home.join("templates").is_dir() {
            return Ok(home);
        }
        bail!("--dev requires --source or MP_HOME pointing at repo root");
    }
    let home = assets::toolkit_home();
    if home.join("templates").is_dir() {
        return Ok(home);
    }
    bail!("cannot resolve toolkit source; set MP_HOME or use --dev --source")
}

fn resolve_mp_binary(source_root: &Path, dev: bool) -> Result<PathBuf> {
    if dev {
        let release = source_root.join("target/release/mp");
        if release.is_file() {
            return Ok(release);
        }
        let debug = source_root.join("target/debug/mp");
        if debug.is_file() {
            return Ok(debug);
        }
        if let Ok(exe) = env::current_exe() {
            return exe.canonicalize().context("canonicalize current mp binary");
        }
        bail!(
            "mp binary not found at {} or {}; run `cargo build -p mp` or `make install-global` first",
            release.display(),
            debug.display()
        );
    }
    env::current_exe().context("resolve current mp binary")
}

fn resolve_raul_binary(source_root: &Path, dev: bool) -> Result<PathBuf> {
    if dev {
        let release = source_root.join("target/release/raul");
        if release.is_file() {
            return Ok(release);
        }
        let debug = source_root.join("target/debug/raul");
        if debug.is_file() {
            return Ok(debug);
        }
        if let Ok(exe) = env::current_exe() {
            if let Some(parent) = exe.parent() {
                let sibling = parent.join("raul");
                if sibling.is_file() {
                    return Ok(sibling);
                }
            }
        }
        bail!(
            "raul binary not found at {} or {}; run `cargo build -p raul` or `make install-global` first",
            release.display(),
            debug.display()
        );
    }
    env::current_exe().context("resolve current raul binary")
}

pub fn write_env_snippet(install_dir: &Path) -> Result<()> {
    let mp_home = install_dir.to_string_lossy();
    let quoted_home = posix_single_quote(&mp_home);
    let quoted_bin = posix_single_quote(&format!("{mp_home}/bin"));
    let content = format!(
        "# Source this file in agent shells.\nexport MP_HOME={quoted_home}\nexport PATH={quoted_bin}:\"$PATH\"\n"
    );
    crate::store::atomic_write(install_dir.join("env.sh"), content)?;
    Ok(())
}

fn posix_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub fn env_snippet_path(install_dir: &Path) -> PathBuf {
    install_dir.join("env.sh")
}

pub fn install_dir() -> PathBuf {
    env::var_os("MP_INSTALL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".agents").join("master-plan"))
}

pub fn agents_skill_dir() -> PathBuf {
    harness::harness_by_id("opencode")
        .map(|h| harness::resolved_global_skill_dir(&h))
        .unwrap_or_else(|| home_dir().join(".agents/skills"))
}

pub fn cursor_skill_dir() -> PathBuf {
    harness::harness_by_id("cursor")
        .map(|h| harness::resolved_global_skill_dir(&h))
        .unwrap_or_else(|| home_dir().join(".cursor/skills"))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Recursively mirror `src` into `dst`: copy/overwrite every file from src,
/// and **prune** any file or directory present in `dst` but absent from `src`.
///
/// The three trees copied by `install` (`templates`, `schemas`, `docs`) are
/// fully managed by `mp install` — the install dir must reflect the current
/// source. A merge-only copy leaves stale orphans behind when a template is
/// removed from source (e.g. the `master-planner` skill dirs dropped in M141),
/// and those orphans then trip `verify_installed_artifacts` (below
/// `MIN_ARTIFACT_SIZE`, or just stale junk). Mirroring keeps the install dir an
/// exact reflection of source across upgrades.
///
/// Prune decisions use `DirEntry::file_type()` (no symlink follow) so a
/// symlinked entry is removed as a single entry, never recursed into.
fn mirror_tree(src: &Path, dst: &Path) -> Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        let src_names: HashSet<std::ffi::OsString> = fs::read_dir(src)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        // Prune dst entries that no longer exist in src. `read_dir(dst)` is
        // safe here: `create_dir_all` above guarantees dst exists (no-op if it
        // already did), so a first-time install simply iterates an empty dir.
        for entry in fs::read_dir(dst)? {
            let entry = entry?;
            if !src_names.contains(&entry.file_name()) {
                let p = entry.path();
                // file_type() does not follow symlinks — a symlinked entry is
                // treated as a single removable entry, not recursed into.
                if entry.file_type()?.is_dir() {
                    fs::remove_dir_all(&p)?;
                } else {
                    fs::remove_file(&p)?;
                }
            }
        }
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            mirror_tree(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        // src is a file. If dst drifted into a directory across versions,
        // remove the dir first so the file lands at dst, not inside it.
        if dst.is_dir() {
            fs::remove_dir_all(dst)?;
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
    }
    Ok(())
}

/// Re-sign a freshly-copied Mach-O binary with a plain adhoc
/// signature so macOS AMFI/ASP accepts it on the final install path.
///
/// Context: modern Apple toolchains (clang, rust-lld) emit a Mach-O
/// with the `linker-signed` (0x20002) CodeDirectory flag, which is
/// the binary-content form but not the form macOS 26.x's provenance
/// sandbox accepts on every install path (stow into dotfiles, for
/// example, triggers ASP `Unable to apply provenance sandbox` and the
/// kernel kills the process with SIGKILL before the entry point runs).
/// A plain `codesign --force --deep --sign -` against the binary in
/// its **final** install location replaces the linker-signed flag with
/// the `adhoc` (0x2) form that AMFI/ASP trusts.
///
/// Must run **after** the binary is in its final location — re-signing
/// a binary at path A and then moving it leaves a path/provenance
/// mismatch. The order in [`install`] is: `fs::copy` (stamps
/// `com.apple.provenance` for the final path) → this helper (binds
/// the signature to the file at the final path).
///
/// No-op on non-macOS targets and on systems where `codesign` is not
/// on `PATH` (Linux dev machines, CI containers). Errors are surfaced
/// as warnings on non-macOS and as hard failures on macOS — the latter
/// because a missing `codesign` on macOS means the install is broken
/// the moment the user runs the binary.
fn codesign_macos_binary(path: &Path) -> Result<()> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let display = path.display().to_string();
        let output = Command::new("codesign")
            .args(["--force", "--deep", "--sign", "-", &display])
            .output();

        match output {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                bail!(
                    "codesign failed for {} (exit {:?}): {}",
                    display,
                    out.status.code(),
                    stderr.trim()
                );
            }
            Err(e) => bail!("codesign could not be spawned for {}: {}", display, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_root() -> PathBuf {
        // F-26 external review: prefer the in-repo source tree over
        // MP_HOME for unit tests. The MP_HOME lookup at the top used
        // to win first — on a developer's machine where MP_HOME =
        // ~/.agents/master-plan, that MP_HOME may contain only an
        // out-of-date skill subset (e.g. leftover from a probe), and
        // every assertion that pins skill counts or known ids (>= 3
        // skills, contains mp-flow, etc.) silently fails because the
        // foreign registry is the test's source-of-truth. Tests are
        // pinning THIS repo's registry; use the repo's templates/.
        let from_cargo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        if from_cargo.join("templates/skills").is_dir() {
            return from_cargo;
        }
        if let Ok(home) = std::env::var("MP_HOME") {
            let p = PathBuf::from(home);
            if p.join("templates/skills").is_dir() {
                return p;
            }
        }
        PathBuf::from(".")
    }

    #[test]
    fn registry_loads_from_embedded() {
        let root = source_root();
        if !root.join("templates/skills").is_dir() {
            return;
        }
        let registry = SkillRegistry::load(&root).unwrap();
        assert!(!registry.skills.is_empty(), "registry should have skills");
        let ids: Vec<&str> = registry.skill_ids();
        assert!(ids.contains(&"mp-flow"), "registry should include mp-flow");
        assert!(
            ids.contains(&"mp-runner"),
            "registry should include mp-runner"
        );
        assert!(
            ids.contains(&"mp-coordinator"),
            "registry should include mp-coordinator"
        );
        assert!(
            ids.contains(&"spec-grill"),
            "registry should include spec-grill"
        );
        assert!(
            !ids.contains(&"mp-code-review"),
            "registry must exclude category=internal skills (mp-code-review)"
        );
    }

    #[test]
    fn registry_validate_selection_rejects_unknown() {
        let root = source_root();
        if !root.join("templates/skills").is_dir() {
            return;
        }
        let registry = SkillRegistry::load(&root).unwrap();
        let result = registry.validate_selection(&["bogus".to_string()]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bogus"), "error should mention bogus: {err}");
        assert!(err.contains("mp-flow"), "error should list mp-flow: {err}");
        assert!(
            err.contains("spec-grill"),
            "error should list spec-grill: {err}"
        );
    }

    #[test]
    fn registry_validate_selection_accepts_known() {
        let root = source_root();
        if !root.join("templates/skills").is_dir() {
            return;
        }
        let registry = SkillRegistry::load(&root).unwrap();
        let selected = registry
            .validate_selection(&["mp-flow".to_string(), "spec-grill".to_string()])
            .unwrap();
        assert_eq!(selected.len(), 2);
        assert!(selected.contains(&"mp-flow".to_string()));
        assert!(selected.contains(&"spec-grill".to_string()));
    }

    #[test]
    fn check_registry_passes_on_clean_state() {
        let root = source_root();
        if !root.join("templates/skills").is_dir() {
            return;
        }
        let report = check_registry(&root).unwrap();
        assert!(report.ok, "check should pass, errors: {:?}", report.errors);
        assert!(report.skill_count >= 3);
    }

    #[test]
    fn check_registry_errors_on_missing_skill_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let skills_dir = tmp.path().join("templates/skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let skill_dir = skills_dir.join("ghost");
        fs::create_dir_all(&skill_dir).unwrap();
        let manifest = serde_json::json!({"id": "ghost", "display": "Ghost"});
        fs::write(
            skill_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let report = check_registry(tmp.path()).unwrap();
        assert!(!report.ok);
        assert!(
            report.errors.iter().any(|e| e.contains("ghost")),
            "errors should mention ghost: {:?}",
            report.errors
        );
    }

    #[test]
    fn check_registry_errors_on_consumes_unknown() {
        let tmp = tempfile::TempDir::new().unwrap();
        let skills_dir = tmp.path().join("templates/skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let skill_dir = skills_dir.join("oracle");
        fs::create_dir_all(&skill_dir).unwrap();
        let manifest = serde_json::json!({
            "id": "oracle",
            "display": "Oracle",
            "consumes": ["nonexistent"]
        });
        fs::write(
            skill_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# oracle\n").unwrap();

        let report = check_registry(tmp.path()).unwrap();
        assert!(!report.ok);
        assert!(
            report.errors.iter().any(|e| e.contains("nonexistent")),
            "errors should mention nonexistent: {:?}",
            report.errors
        );
    }

    /// M119 F-01: a Pattern: block that references a skill id not in the
    /// registry must surface as a check_registry error. M120's content
    /// introduces these; a stale reference (e.g., `Pattern: mp-mentor`
    /// when only `mp-coordinator` / `mp-runner` ship) is a real defect.
    #[test]
    fn check_registry_errors_on_stale_pattern_reference() {
        let tmp = tempfile::TempDir::new().unwrap();
        let skills_dir = tmp.path().join("templates/skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let skill_dir = skills_dir.join("oracle");
        fs::create_dir_all(&skill_dir).unwrap();
        let manifest = serde_json::json!({
            "id": "oracle",
            "display": "Oracle"
        });
        fs::write(
            skill_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# oracle\n\nPattern: nonexistent-skill\n",
        )
        .unwrap();

        let report = check_registry(tmp.path()).unwrap();
        assert!(!report.ok, "stale Pattern: reference should fail the check");
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("nonexistent-skill")),
            "errors should mention the stale Pattern id: {:?}",
            report.errors
        );
    }

    /// M119 F-01 (positive case): a Pattern: block that references a
    /// known skill id passes the check. mp-flow's content uses these
    /// to cross-reference sub-skill procedures.
    #[test]
    fn check_registry_passes_on_known_pattern_reference() {
        let tmp = tempfile::TempDir::new().unwrap();
        let skills_dir = tmp.path().join("templates/skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let oracle_dir = skills_dir.join("oracle");
        fs::create_dir_all(&oracle_dir).unwrap();
        let partner_dir = skills_dir.join("partner");
        fs::create_dir_all(&partner_dir).unwrap();
        fs::write(
            oracle_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({"id": "oracle", "display": "Oracle"}))
                .unwrap(),
        )
        .unwrap();
        fs::write(oracle_dir.join("SKILL.md"), "# oracle\n").unwrap();
        fs::write(
            partner_dir.join("manifest.json"),
            serde_json::to_string_pretty(
                &serde_json::json!({"id": "partner", "display": "Partner"}),
            )
            .unwrap(),
        )
        .unwrap();
        fs::write(
            partner_dir.join("SKILL.md"),
            "# partner\n\nPattern: oracle\n",
        )
        .unwrap();

        let report = check_registry(tmp.path()).unwrap();
        assert!(
            report.ok,
            "known Pattern: reference should pass; errors: {:?}",
            report.errors
        );
    }

    /// M141 R1 + code-review remediation: skill ids that contain
    /// path-traversal segments are rejected by `validate_skill_id`
    /// (shared by project install, global deploy, and registry load).
    #[test]
    fn install_project_skill_rejects_path_traversal_skill_id() {
        let bad_ids = [
            "",
            "../etc/passwd",
            "foo/../bar",
            "foo/bar",
            "foo\\bar",
            "foo\0bar",
            "..",
            ".",
            ".hidden",
        ];
        for bad in bad_ids {
            assert!(
                validate_skill_id(bad).is_err(),
                "skill id {bad:?} should be flagged as invalid"
            );
        }
        assert!(
            validate_skill_id("mp-flow").is_ok(),
            "clean skill id should be accepted"
        );
    }

    #[test]
    fn registry_load_rejects_traversal_manifest_id() {
        let tmp = tempfile::TempDir::new().unwrap();
        let skill_dir = tmp.path().join("templates/skills/evil");
        fs::create_dir_all(&skill_dir).unwrap();
        let manifest = serde_json::json!({
            "id": "../../../tmp/pwn",
            "display": "evil",
        });
        fs::write(
            skill_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let err = SkillRegistry::load(tmp.path()).unwrap_err();
        assert!(
            err.to_string().contains("invalid skill id")
                || err
                    .chain()
                    .any(|e| e.to_string().contains("invalid skill id")),
            "expected invalid skill id error, got: {err:#}"
        );
    }

    /// F-05: uninstall path resolution rejects traversal skill ids.
    #[test]
    fn skill_dir_for_uninstall_rejects_invalid_skill_id() {
        let entry = InstalledSkill {
            skill_id: "..".into(),
            harness: "opencode".into(),
            category: "core".into(),
            source: String::new(),
            source_url: String::new(),
            upstream_version: String::new(),
            installed_at: String::new(),
            installed_path: "/tmp/skills/..".into(),
            harness_root: String::new(),
            artifact_path: String::new(),
        };
        let err = skill_dir_for_uninstall(&entry).unwrap_err();
        assert!(
            err.to_string().contains("invalid") || err.to_string().contains("refusing"),
            "got: {err:#}"
        );
    }

    /// F-05: path basename must equal skill id; `..` components refused.
    #[test]
    fn ensure_safe_relative_artifact_guards() {
        assert!(ensure_safe_relative_artifact(Path::new("mp-flow"), "mp-flow").is_ok());
        assert!(
            ensure_safe_relative_artifact(Path::new("../evil"), "evil").is_err(),
            "parent-dir component must fail"
        );
        assert!(
            ensure_safe_relative_artifact(Path::new("other"), "mp-flow").is_err(),
            "basename mismatch must fail"
        );
        assert!(
            ensure_safe_relative_artifact(Path::new("/tmp/skills/mp-flow"), "mp-flow").is_err(),
            "absolute artifact path must fail"
        );
    }

    /// F-06: prefer installed_path over current env resolver.
    #[test]
    fn skill_dir_for_uninstall_prefers_installed_path() {
        let recorded = "/legacy/skills-root/mp-flow";
        let entry = InstalledSkill {
            skill_id: "mp-flow".into(),
            harness: "opencode".into(),
            category: "core".into(),
            source: String::new(),
            source_url: String::new(),
            upstream_version: String::new(),
            installed_at: String::new(),
            installed_path: recorded.into(),
            harness_root: String::new(),
            artifact_path: String::new(),
        };
        assert!(
            skill_dir_for_uninstall(&entry).is_err(),
            "legacy absolute path outside current harness root must fail closed"
        );
    }
}
