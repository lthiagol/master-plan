use std::path::PathBuf;

// M151 subdir: single source of truth for harness launch commands
// (opencode/pi/cursor v1) and the herdr agent start argv template
// each one resolves to. The skill-install descriptors below keep the
// install-skill layout (where to deploy `.opencoderules`/skills)
// separate; `registry::HarnessRegistry` owns the *launch* surface.
pub mod auto;
pub mod registry;

pub use auto::{
    auto_set_target, detect_installed_harnesses, is_harness_fully_installed, AutoSetDecision,
};
pub use registry::{HarnessEntry, HarnessError, HarnessRegistry, SUPPORTED_NAMES};

#[derive(Debug, Clone, serde::Serialize)]
pub struct HarnessDescriptor {
    pub id: String,
    pub display_name: String,
    pub convention_file_name: String,
    /// When set, convention file is deployed here instead of under the skill dir.
    pub global_convention_path: Option<PathBuf>,
    /// Parent directory under which per-skill subdirs are deployed.
    /// `skill_dir_for(h, id) = resolved_global_skill_dir(h) / id`.
    pub global_skill_dir: PathBuf,
    /// Project-local parent dir for per-skill subdirs (e.g.
    /// `.cursor/skills/`). `None` for harnesses without a project
    /// install path. Skill subdirs follow the same `id` suffix.
    pub project_skill_dir: Option<PathBuf>,
    pub agent_profile_dir: PathBuf,
    pub supports_global: bool,
    pub supports_project: bool,
}

/// Generic harness registry. The install surface is driven entirely by
/// `templates/skills/*` directories and the SkillRegistry — the harness
/// descriptors name generic per-harness parent dirs, not specific
/// skill ids. Skills are siblings under each harness's
/// `global_skill_dir`.
pub fn default_registry() -> Vec<HarnessDescriptor> {
    let home = dirs_or_home();
    vec![
        HarnessDescriptor {
            id: "opencode".into(),
            display_name: "OpenCode".into(),
            convention_file_name: ".opencoderules".into(),
            global_convention_path: None,
            global_skill_dir: home.join(".agents/skills"),
            project_skill_dir: Some(PathBuf::from(".opencode/skills")),
            agent_profile_dir: home.join(".agents/skills"),
            supports_global: true,
            supports_project: true,
        },
        HarnessDescriptor {
            id: "cursor".into(),
            display_name: "Cursor".into(),
            convention_file_name: ".cursorrules".into(),
            global_convention_path: None,
            global_skill_dir: home.join(".cursor/skills"),
            project_skill_dir: Some(PathBuf::from(".cursor/skills")),
            agent_profile_dir: home.join(".cursor/skills"),
            supports_global: true,
            supports_project: true,
        },
        HarnessDescriptor {
            id: "claude-code".into(),
            display_name: "Claude Code".into(),
            convention_file_name: "CLAUDE.md".into(),
            global_convention_path: None,
            global_skill_dir: home.join(".claude/skills"),
            project_skill_dir: None,
            agent_profile_dir: home.join(".claude/skills"),
            supports_global: true,
            supports_project: false,
        },
        HarnessDescriptor {
            id: "gemini".into(),
            display_name: "Gemini Code Assist".into(),
            convention_file_name: ".gemini/rules".into(),
            global_convention_path: None,
            global_skill_dir: home.join(".gemini/skills"),
            project_skill_dir: None,
            agent_profile_dir: home.join(".gemini/skills"),
            supports_global: true,
            supports_project: false,
        },
        HarnessDescriptor {
            id: "codex".into(),
            display_name: "Codex CLI".into(),
            convention_file_name: "CODEX.md".into(),
            global_convention_path: None,
            global_skill_dir: home.join(".codex/skills"),
            project_skill_dir: None,
            agent_profile_dir: home.join(".codex/skills"),
            supports_global: true,
            supports_project: false,
        },
        HarnessDescriptor {
            id: "windsurf".into(),
            display_name: "Windsurf".into(),
            convention_file_name: ".windsurfrules".into(),
            global_convention_path: None,
            global_skill_dir: home.join(".windsurf/skills"),
            project_skill_dir: None,
            agent_profile_dir: home.join(".windsurf/skills"),
            supports_global: true,
            supports_project: false,
        },
        HarnessDescriptor {
            id: "cline".into(),
            display_name: "Cline".into(),
            convention_file_name: ".clinerules".into(),
            global_convention_path: None,
            global_skill_dir: home.join(".cline/skills"),
            project_skill_dir: None,
            agent_profile_dir: home.join(".cline/skills"),
            supports_global: true,
            supports_project: false,
        },
        HarnessDescriptor {
            id: "pi".into(),
            display_name: "Pi".into(),
            convention_file_name: "AGENTS.md".into(),
            global_convention_path: Some(home.join(".pi/agent/AGENTS.md")),
            // Pi scans both its native ~/.pi/agent/skills directory and the
            // shared ~/.agents/skills directory. Master Plan uses the shared
            // path as the canonical skill install target so installing both
            // OpenCode and Pi does not create duplicate skill names at Pi
            // startup.
            global_skill_dir: home.join(".agents/skills"),
            project_skill_dir: Some(PathBuf::from(".pi/skills")),
            agent_profile_dir: home.join(".pi/agent"),
            supports_global: true,
            supports_project: true,
        },
    ]
}

pub fn harness_by_id(id: &str) -> Option<HarnessDescriptor> {
    default_registry().into_iter().find(|h| h.id == id)
}

pub fn resolved_global_skill_dir(h: &HarnessDescriptor) -> PathBuf {
    for key in [
        format!("MP_{}_SKILL_DIR", h.id.to_uppercase().replace('-', "_")),
        format!("MP_{}_SKILL_DIR", h.id.to_uppercase()),
    ] {
        if let Ok(val) = std::env::var(&key) {
            return PathBuf::from(val);
        }
    }
    h.global_skill_dir.clone()
}

/// Resolve the directory a specific skill id should be deployed under
/// for a given harness. Each skill id gets its own subdir under the
/// harness's global_skill_dir (or override).
pub fn skill_dir_for(h: &HarnessDescriptor, skill_id: &str) -> PathBuf {
    resolved_global_skill_dir(h).join(skill_id)
}

/// M173 S2: resolve the directory where an agent file should be
/// deployed for a given harness. The convention is one `<id>.md`
/// per agent directly under the agent dir; agents don't get their
/// own subdirectory like skills do.
///
/// Per-harness default:
/// - opencode: `~/.agents/agents/` (sibling of `~/.agents/skills/`)
/// - cursor:   `~/.cursor/agents/`
/// - claude-code, gemini, codex, windsurf, cline, pi: not yet wired
///   (M173 ships agents for opencode + cursor only; calling this
///   function for an unsupported harness returns the harness's
///   `agent_profile_dir` for diagnostic purposes).
///
/// Honors `MP_<HARNESS>_AGENT_DIR` env var overrides, matching the
/// `MP_<HARNESS>_SKILL_DIR` pattern that the skill install path uses.
pub fn resolved_agent_dir(h: &HarnessDescriptor) -> PathBuf {
    // POSIX env vars can't contain hyphens, so any harness id with a
    // hyphen (e.g. `claude-code`) MUST be normalized to underscores
    // when building the env-var key. The replacement is unconditional
    // (no second uppercase-only probe) — the first call already
    // handles the underscored form, so a second probe would never
    // match anything different and only adds dead code.
    let env_key = format!("MP_{}_AGENT_DIR", h.id.to_uppercase().replace('-', "_"));
    if let Ok(val) = std::env::var(&env_key) {
        return PathBuf::from(val);
    }
    // Default: derive from agent_profile_dir (which is currently the
    // skill dir for opencode/cursor). We strip a trailing `/skills`
    // and append `/agents` so the agent dir is a sibling of the
    // skill dir. Falls back to agent_profile_dir/agents when the
    // pattern doesn't match (e.g. for harnesses without a `/skills`
    // suffix).
    let profile = &h.agent_profile_dir;
    if profile.file_name().and_then(|n| n.to_str()) == Some("skills") {
        if let Some(parent) = profile.parent() {
            return parent.join("agents");
        }
    }
    profile.join("agents")
}

pub fn convention_path(h: &HarnessDescriptor) -> PathBuf {
    let skill_dir = resolved_global_skill_dir(h);
    if let Some(path) = &h.global_convention_path {
        // Convention file lives one level above the skill root (the
        // agent's profile dir), both in production and under test
        // overrides. The override path follows the same relative
        // offset, so a uniform `skill_dir.parent()` rule keeps the
        // production layout and the test-override layout consistent.
        if skill_dir != h.global_skill_dir {
            return skill_dir
                .parent()
                .map(|agent| agent.join(&h.convention_file_name))
                .unwrap_or_else(|| path.clone());
        }
        return path.clone();
    }
    skill_dir.join(&h.convention_file_name)
}

pub fn skill_dir(h: &HarnessDescriptor) -> &PathBuf {
    &h.global_skill_dir
}

pub fn profile_dir(h: &HarnessDescriptor) -> &PathBuf {
    &h.agent_profile_dir
}

pub fn print_paths_json(h: &HarnessDescriptor) -> serde_json::Value {
    // `skill_dir` here is the resolved per-harness path (honors
    // MP_<id>_SKILL_DIR overrides), so it stays consistent with
    // `convention_file` (which also goes through resolved_global_skill_dir).
    serde_json::json!({
        "id": h.id,
        "display_name": h.display_name,
        "convention_file": convention_path(h),
        "skill_dir": resolved_global_skill_dir(h),
        "project_skill_dir": h.project_skill_dir,
        "profile_dir": h.agent_profile_dir,
    })
}

fn dirs_or_home() -> PathBuf {
    if let Ok(val) = std::env::var("HOME") {
        PathBuf::from(val)
    } else {
        PathBuf::from(".")
    }
}
