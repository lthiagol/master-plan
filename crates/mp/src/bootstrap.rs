use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::brownfield;
use crate::paths::PlanContext;
use crate::store;

#[derive(Debug, Serialize)]
pub struct BootstrapReport {
    pub ok: bool,
    pub profile: String,
    pub brownfield_likely: bool,
    pub stack: Vec<String>,
    pub project_name: String,
    pub description: String,
    pub applied: Vec<String>,
    pub suggestions: Vec<String>,
}

pub fn apply_from_repo(ctx: &PlanContext, profile: Option<&str>) -> Result<BootstrapReport> {
    let profile = profile.unwrap_or("full");
    let root = &ctx.project_root;
    let stack = brownfield::detect_stack(root);
    let brownfield_likely = brownfield::detect_brownfield_likely(root);
    let project_name = detect_project_name(root).unwrap_or_else(|| {
        root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string()
    });
    let description = detect_description(root).unwrap_or_default();

    let mut plan = store::load_plan(ctx)?;
    let mut applied = Vec::new();

    if plan.project.name.is_empty() && !project_name.is_empty() {
        plan.project.name = project_name.clone();
        applied.push("project.name".to_string());
    }
    if plan.project.description.is_empty() && !description.is_empty() {
        plan.project.description = description.clone();
        applied.push("project.description".to_string());
    }
    if plan.project.stack.is_empty() && !stack.is_empty() {
        plan.project.stack = stack.clone();
        applied.push("project.stack".to_string());
    }

    if profile == "full" && brownfield_likely && plan.charter.goals.is_empty() {
        let goal = if description.is_empty() {
            "Maintain and extend the existing codebase with spec-driven changes.".to_string()
        } else {
            format!("Evolve the project: {description}")
        };
        plan.charter.goals.push(goal);
        applied.push("charter.goals".to_string());
    }

    if brownfield_likely {
        plan.project.planning_status = "planning".to_string();
        if profile == "full" {
            plan.project.planning_phase = "charter".to_string();
            applied.push("planning_phase=charter".to_string());
        }
    }

    store::write_plan(ctx, &plan)?;

    let mut suggestions = Vec::new();
    if brownfield_likely {
        suggestions.push(
            "Use delta milestones for behavior changes (mp specs init + change_kind=delta)"
                .to_string(),
        );
        suggestions.push("Run mp brownfield scan before interviewing".to_string());
    }
    if profile == "hybrid" {
        suggestions.push("Plan is gitignored — confirm .gitignore includes plan path".to_string());
    }
    if profile == "full" && brownfield_likely {
        suggestions.push("Review charter goals in plan.json before mp brief todo".to_string());
    }

    // Scan markdown docs for charter and backlog candidates
    let md_candidates = detect_markdown_candidates(root);
    for goal in &md_candidates.goals {
        suggestions.push(format!("Goal candidate (from markdown): {goal}"));
    }
    for non_goal in &md_candidates.non_goals {
        suggestions.push(format!("Non-goal candidate (from markdown): {non_goal}"));
    }
    for backlog in &md_candidates.backlog {
        suggestions.push(format!("Backlog candidate (from markdown): {backlog}"));
    }

    Ok(BootstrapReport {
        ok: true,
        profile: profile.to_string(),
        brownfield_likely,
        stack,
        project_name,
        description,
        applied,
        suggestions,
    })
}

fn detect_project_name(root: &Path) -> Option<String> {
    if let Ok(cargo) = fs::read_to_string(root.join("Cargo.toml")) {
        if let Some(name) = parse_toml_string_field(&cargo, "name") {
            return Some(name);
        }
    }
    if let Ok(pkg) = fs::read_to_string(root.join("package.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&pkg) {
            if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                return Some(name.to_string());
            }
        }
    }
    if let Ok(py) = fs::read_to_string(root.join("pyproject.toml")) {
        if let Some(name) = parse_toml_string_field(&py, "name") {
            return Some(name);
        }
    }
    None
}

fn parse_toml_string_field(content: &str, key: &str) -> Option<String> {
    let needle = format!("{key} = ");
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with(&needle) {
            return Some(
                line.trim_start_matches(&needle)
                    .trim()
                    .trim_matches('"')
                    .to_string(),
            );
        }
    }
    None
}

fn detect_description(root: &Path) -> Option<String> {
    for name in ["README.md", "Readme.md", "readme.md", "README"] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let content = fs::read_to_string(&path).ok()?;
        let mut lines = content.lines().map(str::trim).filter(|l| !l.is_empty());
        if let Some(first) = lines.next() {
            if first.starts_with('#') {
                return Some(first.trim_start_matches('#').trim().to_string());
            }
            return Some(first.to_string());
        }
    }
    None
}

struct MdCandidates {
    goals: Vec<String>,
    non_goals: Vec<String>,
    backlog: Vec<String>,
}

fn detect_markdown_candidates(root: &Path) -> MdCandidates {
    let mut goals = Vec::new();
    let mut non_goals = Vec::new();
    let mut backlog = Vec::new();

    // Collect contents of markdown docs that may hold charter/backlog data
    let candidates = [
        root.join("status.md"),
        root.join("backlog.md"),
        root.join("DESIGN.md"),
    ];

    for path in &candidates {
        if !path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };

        let mut current_section = String::new();
        for line in content.lines() {
            let trimmed = line.trim().to_lowercase();

            if trimmed.starts_with("## ") || trimmed.starts_with("### ") {
                current_section = trimmed.trim_start_matches('#').trim().to_string();
            }

            if current_section.contains("non-goal")
                || current_section.contains("non_goal")
                || current_section.contains("out of scope")
                || current_section.contains("not doing")
            {
                if line.trim().starts_with("- ") || line.trim().starts_with("* ") {
                    let entry = line
                        .trim()
                        .trim_start_matches("- ")
                        .trim_start_matches("* ");
                    if !entry.is_empty() && entry.len() > 10 {
                        non_goals.push(entry.to_string());
                    }
                }
            } else if (current_section.contains("goal") || current_section.contains("objective"))
                && (line.trim().starts_with("- ") || line.trim().starts_with("* "))
            {
                let entry = line
                    .trim()
                    .trim_start_matches("- ")
                    .trim_start_matches("* ");
                if !entry.is_empty() && entry.len() > 10 {
                    goals.push(entry.to_string());
                }
            }

            if (current_section.contains("backlog") || current_section.contains("todo"))
                && (line.trim().starts_with("- ") || line.trim().starts_with("* "))
            {
                let entry = line
                    .trim()
                    .trim_start_matches("- ")
                    .trim_start_matches("* ");
                if !entry.is_empty() && entry.len() > 10 {
                    backlog.push(entry.to_string());
                }
            }
        }
    }

    // Also scan README for "Goals" sections
    let readme = root.join("README.md");
    if readme.is_file() {
        if let Ok(content) = fs::read_to_string(&readme) {
            let mut in_goals = false;
            for line in content.lines() {
                let trimmed = line.trim().to_lowercase();
                if trimmed.starts_with("## ") || trimmed.starts_with("### ") {
                    in_goals = (trimmed.contains("goal")
                        && !trimmed.contains("non-goal")
                        && !trimmed.contains("non_goal"))
                        || trimmed.contains("objective");
                    continue;
                }
                if in_goals && (line.trim().starts_with("- ") || line.trim().starts_with("* ")) {
                    let entry = line
                        .trim()
                        .trim_start_matches("- ")
                        .trim_start_matches("* ");
                    if !entry.is_empty() && entry.len() > 10 {
                        goals.push(entry.to_string());
                    }
                }
            }
        }
    }

    MdCandidates {
        goals,
        non_goals,
        backlog,
    }
}
