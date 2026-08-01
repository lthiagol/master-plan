use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use tempfile::{NamedTempFile, TempDir};

use crate::assets;
use crate::config::{self, ProjectConfig};
use crate::model::*;
use crate::paths::{self, PlanContext};
use crate::track_kind;

/// Soft cap for plan/config/session JSON reads (64 MiB). Prevents unbounded
/// deserialize memory use from a corrupted or hostile file.
pub const MAX_PLAN_FILE_BYTES: u64 = 64 * 1024 * 1024;

static MUTATION_WRITE_COUNT: AtomicUsize = AtomicUsize::new(0);
static MUTATION_FAILPOINT_FIRED: AtomicBool = AtomicBool::new(false);
/// Armed by `MP_MUTATION_CRASH_AFTER_WRITE` after write N succeeds. Further
/// writes fail so a multi-file op cannot continue; `PlanWriteTxn::run_recoverable`
/// then either seals+aborts (op Ok) or leaves the txn for recovery (op Err).
static MUTATION_CRASH_ARMED: AtomicBool = AtomicBool::new(false);

/// True after a crash failpoint armed on a prior durable write in this process.
pub(crate) fn mutation_crash_armed() -> bool {
    MUTATION_CRASH_ARMED.load(Ordering::SeqCst)
}

pub(crate) fn read_text_bounded(path: &Path, max_bytes: u64) -> Result<String> {
    crate::json_input::read_file_bounded(path, max_bytes)
        .with_context(|| format!("read {}", path.display()))
}

/// Attach a "not found" wrapper only for missing paths; preserve size/UTF-8
/// errors from the bounded reader so oversized files are not mislabeled.
fn with_missing_resource_context<T>(
    result: Result<T>,
    not_found: impl FnOnce() -> String,
) -> Result<T> {
    result.map_err(|error| {
        let missing = error.chain().any(|cause| {
            cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
        });
        if missing {
            error.context(not_found())
        } else {
            error
        }
    })
}

pub(crate) fn atomic_write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    let dir = path
        .parent()
        .with_context(|| format!("path {} has no parent directory", path.display()))?;
    let mut tmp = NamedTempFile::new_in(dir)
        .with_context(|| format!("create temp file in {}", dir.display()))?;
    tmp.write_all(contents.as_ref())
        .with_context(|| format!("write temp file {}", tmp.path().display()))?;
    tmp.as_file()
        .sync_all()
        .with_context(|| format!("fsync temp file {}", tmp.path().display()))?;
    let persisted = tmp
        .persist(path)
        .with_context(|| format!("persist {}", path.display()))?;
    persisted
        .sync_all()
        .with_context(|| format!("fsync {}", path.display()))?;
    fs::File::open(dir)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("fsync directory {}", dir.display()))?;
    mutation_write_failpoint(path)?;
    Ok(())
}

pub(crate) fn rename_plan_path(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create rename parent {}", parent.display()))?;
    }
    fs::rename(from, to)
        .with_context(|| format!("rename {} -> {}", from.display(), to.display()))?;
    if let Some(parent) = to.parent() {
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| format!("fsync rename parent {}", parent.display()))?;
    }
    if let Err(error) = mutation_write_failpoint(to) {
        // Rename already landed; roll it back so callers never observe a torn
        // rename+error (plan relocate path vs config, archive mid-rename, etc.).
        if to.exists() && !from.exists() {
            if let Err(rollback) = fs::rename(to, from) {
                return Err(anyhow::anyhow!(
                    "{error:#}; rename rollback {} -> {} also failed: {rollback}",
                    to.display(),
                    from.display()
                ));
            }
            if let Some(parent) = from.parent() {
                let _ = fs::File::open(parent).and_then(|directory| directory.sync_all());
            }
        }
        return Err(error);
    }
    Ok(())
}

fn mutation_write_failpoint(path: &Path) -> Result<()> {
    if path.components().any(|c| c.as_os_str() == ".mp-txn")
        || MUTATION_FAILPOINT_FIRED.load(Ordering::SeqCst)
    {
        return Ok(());
    }
    if MUTATION_CRASH_ARMED.load(Ordering::SeqCst) {
        bail!(
            "injected mutation crash gate after prior write: {}",
            path.display()
        );
    }
    let fail_limit = std::env::var("MP_MUTATION_FAIL_AFTER_WRITE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    let crash_limit = std::env::var("MP_MUTATION_CRASH_AFTER_WRITE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    if fail_limit.is_none() && crash_limit.is_none() {
        return Ok(());
    }
    let count = MUTATION_WRITE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    if crash_limit == Some(count) {
        // Defer abort until the recoverable envelope seals (op Ok) or leaves
        // the txn pending (op Err after a later write is refused). Aborting
        // here would skip the durable COMMITTED marker and make recovery undo
        // a fully written multi-file mutation.
        MUTATION_CRASH_ARMED.store(true, Ordering::SeqCst);
        return Ok(());
    }
    if fail_limit == Some(count) {
        MUTATION_FAILPOINT_FIRED.store(true, Ordering::SeqCst);
        bail!(
            "injected mutation failure after durable write {count}: {}",
            path.display()
        );
    }
    Ok(())
}

pub fn init_plan(ctx: &PlanContext, profile: Option<&str>, force: bool) -> Result<Vec<String>> {
    let profile = profile.unwrap_or("full");

    // Compute intended plan-dir location relative to project root (before staging,
    // since during staging ctx.plan_dir is a temp directory).
    let plan_dir_rel = ctx
        .plan_dir
        .strip_prefix(&ctx.project_root)
        .unwrap_or(&ctx.plan_dir)
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();

    if ctx.plan_dir.exists() {
        if crate::migrate::plan_dir_has_legacy_toml(&ctx.plan_dir)? {
            anyhow::bail!(
                "legacy TOML plan artifacts detected in {}; migrate to JSON before init \
                 (cargo run -p mp --example migrate-toml-to-json -- {})",
                ctx.plan_dir.display(),
                ctx.plan_dir.display()
            );
        }
        let plan_json = ctx.plan_dir.join("plan.json");
        if plan_json.exists() && !force {
            anyhow::bail!(
                "project already initialized ({} exists); use --force to re-generate",
                plan_json.display()
            );
        }
        return init_plan_inner(ctx, profile, &plan_dir_rel);
    }

    // Fresh init: stage everything in a temp directory and atomically rename
    let staging = TempDir::new_in(&ctx.project_root)
        .with_context(|| format!("create staging dir in {}", ctx.project_root.display()))?;
    let staging_ctx = PlanContext {
        project_root: ctx.project_root.clone(),
        plan_dir: staging.path().to_path_buf(),
    };

    let created = match init_plan_inner(&staging_ctx, profile, &plan_dir_rel) {
        Ok(created) => created,
        Err(e) => {
            // TempDir auto-cleanup on drop; we must not rename partial state
            return Err(e);
        }
    };

    // Atomic rename of the staging directory into the final plan dir location
    if let Some(parent) = ctx.plan_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(staging.path(), &ctx.plan_dir)
        .with_context(|| format!("rename staging to {}", ctx.plan_dir.display()))?;

    // staging TempDir will be dropped but its path was renamed away, so cleanup is a no-op
    Ok(created)
}

fn init_plan_inner(ctx: &PlanContext, profile: &str, location: &str) -> Result<Vec<String>> {
    let mut cfg: serde_json::Value = serde_json::from_str(&config::profile_config_json(profile)?)?;
    // Set workflow.plan.location as a JSON string field (never splice into text).
    if let Some(plan) = cfg
        .pointer_mut("/workflow/plan")
        .and_then(|v| v.as_object_mut())
    {
        plan.insert(
            "location".to_string(),
            serde_json::Value::String(location.to_string()),
        );
    } else {
        bail!("config template missing workflow.plan object");
    }
    let cfg = serde_json::to_string_pretty(&cfg)?;
    let mut created = Vec::new();

    let dirs = [
        ctx.plan_dir.clone(),
        ctx.milestones_dir(),
        ctx.sessions_dir(),
        ctx.tracks_dir(),
        ctx.archive_dir(),
        ctx.archive_dir().join("milestones"),
        ctx.archive_dir().join("backlog"),
        ctx.archive_dir().join("sessions"),
        ctx.specs_dir(),
        ctx.challenges_dir(),
    ];
    for d in dirs {
        if !d.exists() {
            fs::create_dir_all(&d)?;
            let rel = relative_plan(&ctx.plan_dir, &d);
            if !rel.is_empty() {
                created.push(rel);
            }
        }
    }

    let mut plan: PlanFile =
        serde_json::from_str(&assets::read_embedded("templates/defaults/plan.json")?)?;
    plan.project.created = today();
    match profile {
        "full" => {
            plan.project.planning_status = "planning".to_string();
            plan.project.planning_phase = "brief".to_string();
        }
        "hybrid" | "session" => {
            plan.project.planning_status = "ready-for-execution".to_string();
            plan.project.planning_phase = "execution".to_string();
        }
        _ => {}
    }

    let plan_json = serde_json::to_string_pretty(&plan)?;
    let files: Vec<(&str, String)> = vec![
        (
            "AGENTS.md",
            assets::read_embedded("templates/AGENTS-TEMPLATE.md")?,
        ),
        ("config.json", cfg),
        ("plan.json", format!("{plan_json}\n")),
    ];

    let mut optional_files: Vec<(&str, &str)> = Vec::new();
    if profile == "full" {
        optional_files.push(("brief.json", "templates/defaults/brief.json"));
        optional_files.push(("backlog.json", "templates/defaults/backlog.json"));
        optional_files.push(("ideas.json", "templates/defaults/ideas.json"));
        optional_files.push(("decisions.json", "templates/defaults/decisions.json"));
        optional_files.push(("annotations.json", "templates/defaults/annotations.json"));
    } else if profile == "hybrid" {
        optional_files.push(("ideas.json", "templates/defaults/ideas.json"));
        optional_files.push(("decisions.json", "templates/defaults/decisions.json"));
        optional_files.push(("annotations.json", "templates/defaults/annotations.json"));
    }

    for (name, content) in files {
        let p = ctx.plan_dir.join(name);
        if !p.exists() {
            atomic_write(&p, content)?;
            created.push(name.to_string());
        }
    }

    for (name, template) in optional_files {
        let p = ctx.plan_dir.join(name);
        if !p.exists() {
            let mut content = assets::read_embedded(template)?;
            if name == "brief.json" {
                content = content.replace(
                    "\"created\": \"\"",
                    &format!("\"created\": \"{}\"", today()),
                );
            }
            atomic_write(&p, content)?;
            created.push(name.to_string());
        }
    }

    let loaded_cfg = load_config(ctx);
    if loaded_cfg.workflow.artifacts.tracks.unwrap_or(true) {
        for &tk in &track_kind::TrackKind::ALL {
            let kind = tk.as_str();
            let p = ctx.track_path(kind);
            if !p.exists() {
                let mut track = default_track(kind)?;
                if kind == "tweak" {
                    track.track.title = "Tweaks".to_string();
                    track.track.scope =
                        "Small improvements and polish; no new features.".to_string();
                }
                write_track(ctx, &p, &track)?;
                created.push(format!("tracks/{kind}.json"));
            }
        }
    }

    let meta = ctx.archive_meta_path();
    if !meta.exists() {
        write_archive_meta(&meta, &ArchiveMetaFile::default())?;
        created.push("archive/meta.json".to_string());
    }

    maybe_ensure_gitignore(ctx, &loaded_cfg)?;

    Ok(created)
}

fn maybe_ensure_gitignore(ctx: &PlanContext, cfg: &ProjectConfig) -> Result<()> {
    if cfg.workflow.plan.in_repo.unwrap_or(true) {
        return Ok(());
    }
    let loc = cfg.plan_location();
    let gitignore = ctx.project_root.join(".gitignore");
    let needle = format!("{loc}/");
    if gitignore.exists() {
        let content = fs::read_to_string(&gitignore)?;
        if content
            .lines()
            .any(|l| l.trim() == loc || l.trim() == needle.trim_end_matches('/'))
        {
            return Ok(());
        }
        let mut out = content;
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&needle);
        out.push('\n');
        atomic_write(gitignore, out)?;
    }
    Ok(())
}

fn relative_plan(plan_dir: &Path, path: &Path) -> String {
    path.strip_prefix(plan_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

pub fn default_track(kind: &str) -> Result<TrackFile> {
    let mut raw: TrackFile =
        serde_json::from_str(&assets::read_embedded("templates/defaults/track.json")?)?;
    raw.track.kind = kind.to_string();
    raw.track.perpetual = true;
    raw.track.created = today();
    raw.items.clear();
    if kind == "bugfix" {
        raw.track.title = "Bugfixes".to_string();
    } else {
        raw.track.title = "Tweaks".to_string();
    }
    Ok(raw)
}

pub fn try_load_config(ctx: &PlanContext) -> Result<ProjectConfig> {
    let path = ctx.plan_dir.join("config.json");
    if !path.exists() {
        return Ok(ProjectConfig::default());
    }
    let s = read_text_bounded(&path, MAX_PLAN_FILE_BYTES)
        .with_context(|| format!("read config {}", path.display()))?;
    serde_json::from_str(&s).with_context(|| format!("parse config {}", path.display()))
}

/// Load project config for read-mostly paths. Missing file uses defaults; corrupt
/// file also falls back to defaults — use [`try_load_config`] on write paths and
/// in validate/doctor where corruption must surface.
pub fn load_config(ctx: &PlanContext) -> ProjectConfig {
    try_load_config(ctx).unwrap_or_default()
}

pub fn load_plan(ctx: &PlanContext) -> Result<PlanFile> {
    let path = ctx.plan_dir.join("plan.json");
    let s = read_text_bounded(&path, MAX_PLAN_FILE_BYTES)?;
    Ok(serde_json::from_str(&s)?)
}

pub fn write_plan(ctx: &PlanContext, plan: &PlanFile) -> Result<()> {
    let path = ctx.plan_dir.join("plan.json");
    let json = serde_json::to_string_pretty(plan)?;
    atomic_write(path, format!("{json}\n"))?;
    Ok(())
}

/// Load a JSON collection file with default fallback (returns `Default::default()` if missing).
fn load_collection<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = read_text_bounded(path, MAX_PLAN_FILE_BYTES)?;
    Ok(serde_json::from_str(&raw)?)
}

/// Write a JSON collection file atomically (pretty, 2-space).
fn write_collection(path: impl AsRef<Path>, data: &impl serde::Serialize) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    atomic_write(path, format!("{json}\n"))?;
    Ok(())
}

pub fn load_backlog(ctx: &PlanContext) -> Result<BacklogFile> {
    load_collection(&ctx.backlog_path())
}

pub fn write_backlog(ctx: &PlanContext, backlog: &BacklogFile) -> Result<()> {
    write_collection(ctx.backlog_path(), backlog)
}

pub fn next_backlog_id(backlog: &BacklogFile) -> String {
    next_sequential_id(backlog.items.iter().map(|i| i.id.as_str()), "B-")
}

pub fn next_sequential_id<'a>(items: impl Iterator<Item = &'a str>, prefix: &str) -> String {
    let mut max = 0u32;
    for id in items {
        if let Some(n) = id.strip_prefix(prefix).and_then(|s| s.parse::<u32>().ok()) {
            max = max.max(n);
        }
    }
    format!("{}{:02}", prefix, max + 1)
}

pub fn load_decisions(ctx: &PlanContext) -> Result<DecisionsFile> {
    load_collection(&ctx.plan_dir.join("decisions.json"))
}

pub fn write_decisions(ctx: &PlanContext, decisions: &DecisionsFile) -> Result<()> {
    write_collection(ctx.plan_dir.join("decisions.json"), decisions)
}

pub fn next_decision_id(decisions: &DecisionsFile) -> String {
    next_sequential_id(decisions.decisions.iter().map(|d| d.id.as_str()), "D-")
}

pub fn write_config(ctx: &PlanContext, cfg: &ProjectConfig) -> Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    atomic_write(ctx.config_path(), format!("{json}\n"))?;
    Ok(())
}

pub fn load_milestone(path: &Path) -> Result<MilestoneFile> {
    let s = read_text_bounded(path, MAX_PLAN_FILE_BYTES)?;
    let mut m: MilestoneFile = serde_json::from_str(&s)?;
    m.normalize_steps_from_disk()
        .map_err(|message| anyhow::anyhow!("{}: {message}", path.display()))?;
    Ok(m)
}

pub fn write_milestone(path: &Path, m: &MilestoneFile) -> Result<()> {
    let mut out = m.clone();
    out.prepare_for_disk();
    let json = serde_json::to_string_pretty(&out)?;
    atomic_write(path, format!("{json}\n"))?;
    Ok(())
}

pub fn list_milestone_paths(ctx: &PlanContext) -> Result<Vec<PathBuf>> {
    let dir = ctx.milestones_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("json") {
            paths.push(p);
        }
    }
    // B-42 (S2): sort numerically by milestone id so downstream callers
    // (commands/list.rs, reviews.rs, plan_diff.rs, path_engine.rs) cannot
    // accidentally re-introduce the lex regression. The path → id
    // extraction lives in `paths::compare_milestone_paths`.
    paths.sort_by(|a, b| paths::compare_milestone_paths(a.as_path(), b.as_path()));
    Ok(paths)
}

pub fn list_domain_spec_paths(ctx: &PlanContext) -> Result<Vec<PathBuf>> {
    let dir = ctx.specs_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("json") {
            paths.push(p);
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn load_domain_spec(ctx: &PlanContext, domain: &str) -> Result<DomainSpecFile> {
    paths::assert_domain_id(domain)?;
    let path = ctx.domain_spec_path(domain);
    let s = with_missing_resource_context(read_text_bounded(&path, MAX_PLAN_FILE_BYTES), || {
        format!("domain spec {domain} not found at {}", path.display())
    })?;
    Ok(serde_json::from_str(&s)?)
}

pub fn write_domain_spec(ctx: &PlanContext, spec: &DomainSpecFile) -> Result<()> {
    paths::assert_domain_id(&spec.domain.id)?;
    let dir = ctx.specs_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    let path = ctx.domain_spec_path(&spec.domain.id);
    let json = serde_json::to_string_pretty(spec)?;
    atomic_write(path, format!("{json}\n"))?;
    Ok(())
}

pub fn list_challenge_paths(ctx: &PlanContext) -> Result<Vec<PathBuf>> {
    let dir = ctx.challenges_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("json") {
            paths.push(p);
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn load_challenge(path: &Path) -> Result<ChallengeFile> {
    let s = with_missing_resource_context(read_text_bounded(path, MAX_PLAN_FILE_BYTES), || {
        format!("challenge not found: {}", path.display())
    })?;
    Ok(serde_json::from_str(&s)?)
}

pub fn write_challenge(ctx: &PlanContext, path: &Path, challenge: &ChallengeFile) -> Result<()> {
    let cfg = load_config(ctx);
    crate::schema::enforce_challenge(&cfg, challenge)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(challenge)?;
    atomic_write(path, format!("{json}\n"))?;
    Ok(())
}

pub fn load_all_milestones(ctx: &PlanContext) -> Result<Vec<(PathBuf, MilestoneFile)>> {
    let mut out = Vec::new();
    for p in list_milestone_paths(ctx)? {
        out.push((p.clone(), load_milestone(&p)?));
    }
    let cfg = load_config(ctx);
    if cfg.workflow.artifacts.milestones.is_session() {
        for p in list_session_milestone_paths(ctx)? {
            out.push((p.clone(), load_milestone(&p)?));
        }
    } else if !cfg.auto_bind_branch() {
        if let Some(focus) = cfg.focus_session() {
            let m_path = session_dir(ctx, focus)?.join("milestone.json");
            if m_path.exists() {
                out.push((m_path.clone(), load_milestone(&m_path)?));
            }
        }
    }
    Ok(out)
}

pub fn list_session_dirs(ctx: &PlanContext) -> Result<Vec<PathBuf>> {
    let dir = ctx.sessions_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn list_session_milestone_paths(ctx: &PlanContext) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for dir in list_session_dirs(ctx)? {
        let p = dir.join("milestone.json");
        if p.exists() {
            paths.push(p);
        }
    }
    Ok(paths)
}

pub fn session_dir(ctx: &PlanContext, session_id: &str) -> Result<PathBuf> {
    paths::assert_safe_path_segment(session_id, "session")?;
    Ok(ctx.sessions_dir().join(session_id))
}

pub fn load_session(ctx: &PlanContext, session_id: &str) -> Result<SessionFile> {
    let path = session_dir(ctx, session_id)?.join("session.json");
    let s = with_missing_resource_context(read_text_bounded(&path, MAX_PLAN_FILE_BYTES), || {
        format!("session {session_id} not found")
    })?;
    Ok(serde_json::from_str(&s)?)
}

pub fn write_session(ctx: &PlanContext, session_id: &str, session: &SessionFile) -> Result<()> {
    let dir = session_dir(ctx, session_id)?;
    fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(session)?;
    atomic_write(dir.join("session.json"), format!("{json}\n"))?;
    Ok(())
}

pub fn load_session_from_path(path: &Path) -> Result<SessionFile> {
    let s = read_text_bounded(path, MAX_PLAN_FILE_BYTES)
        .with_context(|| format!("read session {}", path.display()))?;
    Ok(serde_json::from_str(&s)?)
}

pub fn write_session_file(path: &Path, session: &SessionFile) -> Result<()> {
    let json = serde_json::to_string_pretty(session)?;
    atomic_write(path, format!("{json}\n"))?;
    Ok(())
}

pub fn load_brief(ctx: &PlanContext) -> Result<BriefFile> {
    let path = ctx.brief_path();
    let s = read_text_bounded(&path, MAX_PLAN_FILE_BYTES)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str(&s)?)
}

pub fn write_brief(ctx: &PlanContext, brief: &BriefFile) -> Result<()> {
    let cfg = load_config(ctx);
    crate::schema::enforce_brief(&cfg, brief)?;
    let json = serde_json::to_string_pretty(brief)?;
    atomic_write(ctx.brief_path(), format!("{json}\n"))?;
    Ok(())
}

pub fn load_ideas(ctx: &PlanContext) -> Result<IdeasFile> {
    load_collection(&ctx.ideas_path())
}

pub fn write_ideas(ctx: &PlanContext, ideas: &IdeasFile) -> Result<()> {
    let cfg = load_config(ctx);
    crate::schema::enforce_ideas(&cfg, ideas)?;
    write_collection(ctx.ideas_path(), ideas)
}

pub fn next_idea_id(ideas: &IdeasFile) -> String {
    next_sequential_id(ideas.ideas.iter().map(|i| i.id.as_str()), "ID-")
}

pub fn load_annotations(ctx: &PlanContext) -> Result<AnnotationFile> {
    load_collection(&ctx.plan_dir.join("annotations.json"))
}

pub fn write_annotations(ctx: &PlanContext, annotations: &AnnotationFile) -> Result<()> {
    let cfg = load_config(ctx);
    crate::schema::enforce_annotations(&cfg, annotations)?;
    write_collection(ctx.plan_dir.join("annotations.json"), annotations)
}

pub fn next_annotation_id(annotations: &AnnotationFile) -> String {
    next_sequential_id(annotations.annotations.iter().map(|a| a.id.as_str()), "AN-")
}

pub fn next_brief_topic_id(brief: &BriefFile) -> String {
    next_sequential_id(brief.topics.iter().map(|t| t.id.as_str()), "T")
}

pub fn load_track(ctx: &PlanContext, kind: &str) -> Result<TrackFile> {
    let path = ctx.track_path(kind);
    let s = with_missing_resource_context(read_text_bounded(&path, MAX_PLAN_FILE_BYTES), || {
        format!("track not found: {kind}")
    })?;
    Ok(serde_json::from_str(&s)?)
}

pub fn write_track(ctx: &PlanContext, path: &Path, track: &TrackFile) -> Result<()> {
    let cfg = load_config(ctx);
    crate::schema::enforce_track(&cfg, track)?;
    let json = serde_json::to_string_pretty(track)?;
    atomic_write(path, format!("{json}\n"))?;
    Ok(())
}

pub fn track_prefix(kind: &str) -> Result<&'static str> {
    match kind {
        "bugfix" => Ok("BF"),
        "tweak" => Ok("TW"),
        _ => bail!("unknown track kind: {kind} (expected bugfix or tweak)"),
    }
}

pub fn next_track_item_id(track: &TrackFile, kind: &str) -> Result<String> {
    let prefix = track_prefix(kind)?;
    Ok(next_sequential_id(
        track.items.iter().map(|i| i.id.as_str()),
        &format!("{prefix}-"),
    ))
}

pub fn load_archive_meta(ctx: &PlanContext) -> Result<ArchiveMetaFile> {
    let path = ctx.archive_meta_path();
    if !path.exists() {
        return Ok(ArchiveMetaFile::default());
    }
    let raw = read_text_bounded(&path, MAX_PLAN_FILE_BYTES)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn write_archive_meta(path: &Path, meta: &ArchiveMetaFile) -> Result<()> {
    let json = serde_json::to_string_pretty(meta)?;
    atomic_write(path, format!("{json}\n"))?;
    Ok(())
}

pub fn append_archive_meta(ctx: &PlanContext, entry: ArchiveEntry) -> Result<()> {
    let path = ctx.archive_meta_path();
    let mut meta = load_archive_meta(ctx)?;
    meta.entries.push(entry);
    write_archive_meta(&path, &meta)
}

pub fn remove_archive_meta_entry(
    ctx: &PlanContext,
    entity_type: &str,
    entity_id: &str,
) -> Result<()> {
    let path = ctx.archive_meta_path();
    let mut meta = load_archive_meta(ctx)?;
    meta.entries
        .retain(|e| !(e.entity_type == entity_type && e.entity_id == entity_id));
    write_archive_meta(&path, &meta)
}

pub fn archive_milestone(ctx: &PlanContext, id: &str) -> Result<()> {
    let norm = paths::normalize_milestone_id(id);
    // Idempotency (M119/M120 sequence): if the milestone is already
    // archived (file lives under archive/milestones/ and is recorded in
    // archive/meta.json), treat the call as a no-op success. The
    // previous behavior errored with "not found", which broke the
    // re-archive workflow after the M98/M99 sequence.
    if let Ok(existing) = archived_milestone_path(ctx, &norm) {
        let meta = load_archive_meta(ctx)?;
        let already = meta
            .entries
            .iter()
            .any(|e| e.entity_type == "milestone" && e.entity_id == norm);
        if already {
            let _ = existing; // path is reachable; we just no-op
            return Ok(());
        }
    }
    let src = paths::find_milestone_file(&ctx.milestones_dir(), &norm)
        .with_context(|| format!("milestone {norm} not found"))?;
    let file_name = src.file_name().unwrap().to_string_lossy().to_string();
    let dest_dir = ctx.archive_dir().join("milestones");
    fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(&file_name);
    rename_plan_path(&src, &dest)?;
    append_archive_meta(
        ctx,
        ArchiveEntry {
            entity_type: "milestone".to_string(),
            entity_id: norm.clone(),
            original_path: format!("milestones/{file_name}"),
            archived_path: format!("archive/milestones/{file_name}"),
            archived_at: Utc::now().to_rfc3339(),
        },
    )?;
    Ok(())
}

pub fn restore_archived_milestone(ctx: &PlanContext, id: &str) -> Result<()> {
    let norm = paths::normalize_milestone_id(id);
    let archive_dir = ctx.archive_dir().join("milestones");
    let mut found = None;
    if archive_dir.exists() {
        for entry in fs::read_dir(&archive_dir)? {
            let p = entry?.path();
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if name.starts_with(&format!("{norm}-")) || name == format!("{norm}.json") {
                found = Some(p);
                break;
            }
        }
    }
    let src = found.with_context(|| format!("archived milestone {norm} not found"))?;
    let dest = ctx.milestones_dir().join(src.file_name().unwrap());
    fs::create_dir_all(ctx.milestones_dir())?;
    rename_plan_path(&src, &dest)?;
    remove_archive_meta_entry(ctx, "milestone", &norm)?;
    Ok(())
}

pub fn purge_archived_milestone(ctx: &PlanContext, id: &str) -> Result<()> {
    let norm = paths::normalize_milestone_id(id);
    let archive_dir = ctx.archive_dir().join("milestones");
    for entry in fs::read_dir(&archive_dir)? {
        let p = entry?.path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if name.starts_with(&format!("{norm}-")) || name == format!("{norm}.json") {
            fs::remove_file(&p)?;
            mutation_write_failpoint(&p)?;
            return Ok(());
        }
    }
    bail!("archived milestone {norm} not found");
}

pub fn list_archived_milestones(ctx: &PlanContext) -> Result<Vec<PathBuf>> {
    let dir = ctx.archive_dir().join("milestones");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        paths.push(entry?.path());
    }
    // B-42 (S2): sort numerically by milestone id (see list_milestone_paths).
    paths.sort_by(|a, b| paths::compare_milestone_paths(a.as_path(), b.as_path()));
    Ok(paths)
}

pub fn archived_milestone_path(ctx: &PlanContext, id: &str) -> Result<PathBuf> {
    let norm = paths::normalize_milestone_id(id);
    for p in list_archived_milestones(ctx)? {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if name.starts_with(&format!("{norm}-")) || name == format!("{norm}.json") {
            return Ok(p);
        }
    }
    bail!("archived milestone {norm} not found")
}

pub fn next_milestone_id(ctx: &PlanContext) -> Result<String> {
    let mut max = 0u32;
    let mut milestones = load_all_milestones(ctx)?;
    for path in list_session_milestone_paths(ctx)? {
        if !milestones.iter().any(|(existing, _)| existing == &path) {
            milestones.push((path.clone(), load_milestone(&path)?));
        }
    }
    for (_, m) in milestones {
        let id = paths::normalize_milestone_id(&m.milestone.id);
        if id.contains('.') {
            continue;
        }
        if let Ok(n) = id.parse::<u32>() {
            max = max.max(n);
        }
    }
    Ok(format!("{:02}", max + 1))
}

pub fn milestone_filename(id: &str, slug: &str) -> String {
    format!("{id}-{slug}.json")
}

pub fn today() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn slugify(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod archive_idempotency_tests {
    use super::*;
    use crate::paths::PlanContext;
    use tempfile::TempDir;

    /// M119/M120 sequence: `archive_milestone` must be idempotent.
    /// Re-archiving a milestone that is already in `archive/milestones/`
    /// (and recorded in `archive/meta.json`) returns Ok(()) instead of
    /// erroring with "not found". The previous behavior broke the
    /// M98/M99 archival workflow after a partial archive run.
    #[test]
    fn archive_milestone_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let plan_dir = tmp.path().to_path_buf();
        // Stage an active milestone.
        let active_dir = plan_dir.join("milestones");
        std::fs::create_dir_all(&active_dir).unwrap();
        let slug = "test-archive-idempotency";
        let id = "98";
        let json = format!(
            r#"{{
                "milestone": {{
                    "id": "{id}",
                    "title": "Test archive idempotency",
                    "slug": "{slug}",
                    "spec_status": "ready",
                    "execution_status": "blocked",
                    "lifecycle": "groomed"
                }},
                "intent": {{ "outcome": "Test" }},
                "scope": {{ "in_scope": ["x"], "out_of_scope": ["y", "z"] }}
            }}"#
        );
        std::fs::write(active_dir.join(format!("{id}-{slug}.json")), json).unwrap();
        let ctx = PlanContext {
            project_root: plan_dir.clone(),
            plan_dir: plan_dir.clone(),
        };

        // First archive: file moves to archive/, meta entry is appended.
        archive_milestone(&ctx, id).expect("first archive");
        assert!(!active_dir.join(format!("{id}-{slug}.json")).exists());
        let archive_path = archived_milestone_path(&ctx, id).unwrap();
        assert!(archive_path.exists());

        // Second archive: must return Ok(()) without touching the file
        // or duplicating the meta entry.
        let meta_before = load_archive_meta(&ctx).unwrap();
        let count_before = meta_before
            .entries
            .iter()
            .filter(|e| e.entity_type == "milestone" && e.entity_id == id)
            .count();
        archive_milestone(&ctx, id).expect("second archive (idempotent)");
        let meta_after = load_archive_meta(&ctx).unwrap();
        let count_after = meta_after
            .entries
            .iter()
            .filter(|e| e.entity_type == "milestone" && e.entity_id == id)
            .count();
        assert_eq!(
            count_before, count_after,
            "second archive must not duplicate meta entry"
        );
        assert_eq!(count_after, 1, "exactly one archive meta entry expected");
    }
}
