use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::config::ProjectConfig;
use crate::store;

/// Reject path segments that could escape a parent directory when joined.
/// Used for domain ids, session ids, and similar plan-relative names.
pub fn assert_safe_path_segment(id: &str, kind: &str) -> Result<()> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains('/')
        || id.contains('\\')
        || id.contains('\0')
    {
        bail!("invalid {kind} id: {id:?}");
    }
    Ok(())
}

/// Domain ids must match `[a-z][a-z0-9-]*` (same rules as `specs init`).
pub fn assert_domain_id(domain: &str) -> Result<()> {
    assert_safe_path_segment(domain, "domain")?;
    if !domain
        .chars()
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false)
    {
        bail!("domain id must start with a lowercase letter");
    }
    if domain
        .chars()
        .any(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
    {
        bail!("domain id must match [a-z][a-z0-9-]*");
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct PlanContext {
    pub project_root: PathBuf,
    pub plan_dir: PathBuf,
}

impl PlanContext {
    pub fn discover(plan_dir: Option<PathBuf>, project_root: Option<PathBuf>) -> Result<Self> {
        let project_root = project_root
            .or_else(|| env::var_os("MP_PROJECT").map(PathBuf::from))
            .or_else(|| env::var_os("MPH_PROJECT").map(PathBuf::from))
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let plan_dir = plan_dir
            .or_else(|| env::var_os("MP_PLAN_DIR").map(PathBuf::from))
            .unwrap_or_else(|| resolve_plan_dir(&project_root));

        Ok(Self {
            project_root,
            plan_dir,
        })
    }

    pub fn ensure_plan_exists(&self) -> Result<()> {
        anyhow::ensure!(
            self.plan_dir.is_dir(),
            "plan directory not found: {}",
            self.plan_dir.display()
        );
        Ok(())
    }

    pub fn milestones_dir(&self) -> PathBuf {
        self.plan_dir.join("milestones")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.plan_dir.join("sessions")
    }

    pub fn brief_path(&self) -> PathBuf {
        self.plan_dir.join("brief.json")
    }

    pub fn ideas_path(&self) -> PathBuf {
        self.plan_dir.join("ideas.json")
    }

    pub fn backlog_path(&self) -> PathBuf {
        self.plan_dir.join("backlog.json")
    }

    /// M95 ER-reapplication L6: single source of truth for the
    /// decisions file path. The search command previously joined
    /// `plan_dir.join("decisions.json")` directly; routing through
    /// this accessor keeps every consumer aligned if the path ever
    /// moves (e.g. versioning under `decisions/v2.json`).
    pub fn decisions_path(&self) -> PathBuf {
        self.plan_dir.join("decisions.json")
    }

    pub fn config_path(&self) -> PathBuf {
        self.plan_dir.join("config.json")
    }

    pub fn tracks_dir(&self) -> PathBuf {
        self.plan_dir.join("tracks")
    }

    pub fn archive_dir(&self) -> PathBuf {
        self.plan_dir.join("archive")
    }

    pub fn archive_meta_path(&self) -> PathBuf {
        self.archive_dir().join("meta.json")
    }

    pub fn track_path(&self, kind: &str) -> PathBuf {
        self.tracks_dir().join(format!("{kind}.json"))
    }

    pub fn specs_dir(&self) -> PathBuf {
        self.plan_dir.join("specs")
    }

    pub fn domain_spec_path(&self, domain: &str) -> PathBuf {
        self.specs_dir().join(format!("{domain}.json"))
    }

    pub fn challenges_dir(&self) -> PathBuf {
        self.plan_dir.join("reviews/challenges")
    }

    /// M180: project-shared activity journal. Stored as a single
    /// `activity.json` at the plan root — not under `.mp/` because it
    /// travels with the project (teammates + clones see the same feed).
    /// Absent file is treated as an empty journal (no backfill, no
    /// migration). See `crate::activity` for the on-disk shape and
    /// retention contract.
    pub fn activity_path(&self) -> PathBuf {
        self.plan_dir.join("activity.json")
    }
}

pub fn resolve_plan_dir(project_root: &Path) -> PathBuf {
    let master_plan = project_root.join("master-plan");
    let dot_mp = project_root.join(".mp");

    if master_plan.is_dir() && !dot_mp.is_dir() {
        return master_plan;
    }
    if dot_mp.is_dir() && !master_plan.is_dir() {
        return dot_mp;
    }

    for candidate in [dot_mp, master_plan.clone()] {
        if candidate.join("config.json").exists() {
            if let Ok(cfg) = read_config_at(&candidate) {
                return project_root.join(cfg.plan_location());
            }
        }
    }

    master_plan
}

fn read_config_at(plan_dir: &Path) -> Result<ProjectConfig> {
    let path = plan_dir.join("config.json");
    let raw = store::read_text_bounded(&path, store::MAX_PLAN_FILE_BYTES)
        .with_context(|| format!("read config {}", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn find_milestone_in_ctx(ctx: &PlanContext, id: &str) -> Option<PathBuf> {
    let norm = normalize_milestone_id(id);
    if let Some(p) = find_milestone_file(&ctx.milestones_dir(), &norm) {
        return Some(p);
    }
    if !ctx.sessions_dir().is_dir() {
        return None;
    }
    if let Ok(entries) = std::fs::read_dir(ctx.sessions_dir()) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let m_path = entry.path().join("milestone.json");
            if !m_path.exists() {
                continue;
            }
            if let Ok(raw) = store::read_text_bounded(&m_path, store::MAX_PLAN_FILE_BYTES) {
                if let Ok(m) = serde_json::from_str::<crate::model::MilestoneFile>(&raw) {
                    if normalize_milestone_id(&m.milestone.id) == norm {
                        return Some(m_path);
                    }
                }
            }
        }
    }
    None
}

pub fn find_milestone_file(dir: &Path, id: &str) -> Option<PathBuf> {
    let normalized = normalize_milestone_id(id);
    let pattern = format!("{normalized}-*.json");
    let pattern2 = format!("{normalized}.json");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".json") {
                continue;
            }
            if name == pattern2 || name.starts_with(&format!("{normalized}-")) {
                return Some(entry.path());
            }
        }
    }
    glob_simple(dir, &pattern).into_iter().next().or_else(|| {
        glob_simple(dir, &format!("M{normalized}-*.json"))
            .into_iter()
            .next()
    })
}

fn glob_simple(dir: &Path, pattern: &str) -> Vec<PathBuf> {
    let re = pattern.replace('.', "\\.").replace('*', ".*");
    let re = regex::Regex::new(&format!("^{re}$")).ok();
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(r) = &re {
                if r.is_match(&name) {
                    out.push(entry.path());
                }
            }
        }
    }
    out
}

pub fn normalize_milestone_id(id: &str) -> String {
    let id = id.trim().trim_start_matches(['M', 'm']);
    if let Some((base, suffix)) = id.split_once('.') {
        let base_norm = normalize_top_level_milestone_id(base);
        return format!("{base_norm}.{suffix}");
    }
    normalize_top_level_milestone_id(id)
}

fn normalize_top_level_milestone_id(id: &str) -> String {
    let digits: String = id.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return id.to_string();
    }
    format!("{:02}", digits.parse::<u32>().unwrap_or(0))
}

pub fn compare_milestone_ids(a: &str, b: &str) -> std::cmp::Ordering {
    milestone_sort_key(a).cmp(&milestone_sort_key(b))
}

pub fn milestone_sort_key(id: &str) -> Vec<u32> {
    normalize_milestone_id(id)
        .split('.')
        .filter_map(|p| p.parse().ok())
        .collect()
}

pub fn display_milestone_id(id: &str) -> String {
    format!("M{}", normalize_milestone_id(id))
}

/// M104 (B-42): extract the leading milestone id segment from a
/// milestone filename of the form `<id>-<slug>.json` or `<id>.json`.
/// Returns an empty string for paths with no usable stem.
///
/// Centralizes the logic previously duplicated in
/// `store::list_milestone_paths` and `store::list_archived_milestones`.
pub fn id_from_milestone_filename(p: &Path) -> &str {
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .split('-')
        .next()
        .unwrap_or("")
}

/// M104 (B-42): sort comparator for milestone-file paths by their
/// numeric id. Use in `.sort_by` calls; the comparator never panics
/// on missing/unexpected stems — empty stems sort together as a
/// stable group at the front.
pub fn compare_milestone_paths(a: &Path, b: &Path) -> std::cmp::Ordering {
    let aid = id_from_milestone_filename(a);
    let bid = id_from_milestone_filename(b);
    compare_milestone_ids(aid, bid)
}
