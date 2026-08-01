use std::fs;

use anyhow::{Context, Result};
use serde_json::json;

use crate::cli::{ChangelogCmd, OutputFormat as Fmt};
use crate::commands::common::emit;
use crate::paths::PlanContext;

pub(crate) fn cmd_changelog(ctx: &PlanContext, cmd: ChangelogCmd, format: Fmt) -> Result<()> {
    match cmd {
        ChangelogCmd::Show { version } => {
            let changelog_path = ctx.project_root.join("CHANGELOG.md");
            let content = fs::read_to_string(&changelog_path)
                .with_context(|| format!("read {}", changelog_path.display()))?;

            let output = if let Some(ver) = version {
                extract_version(&content, &ver)
            } else {
                content
            };

            emit(format, &json!({ "ok": true, "changelog": output }))
        }
        ChangelogCmd::Add {
            entry,
            version,
            section,
            milestone: _milestone,
        } => {
            let changelog_path = ctx.project_root.join("CHANGELOG.md");
            let content = if changelog_path.exists() {
                fs::read_to_string(&changelog_path)
                    .with_context(|| format!("read {}", changelog_path.display()))?
            } else {
                String::new()
            };

            let result = add_entry(&content, &version, &section, &entry)?;
            fs::write(&changelog_path, &result)
                .with_context(|| format!("write {}", changelog_path.display()))?;
            emit(
                format,
                &json!({ "ok": true, "version": version, "section": section, "entry": entry }),
            )
        }
        ChangelogCmd::Init => {
            let changelog_path = ctx.project_root.join("CHANGELOG.md");
            if changelog_path.exists() {
                anyhow::bail!("CHANGELOG.md already exists");
            }
            let scaffold = "# Changelog\n\n## [Unreleased]\n\n### Added\n\n### Fixed\n\n### Changed\n\n### Deprecated\n\n### Removed\n\n### Security\n".to_string();
            fs::write(&changelog_path, &scaffold)
                .with_context(|| format!("write {}", changelog_path.display()))?;
            emit(format, &json!({ "ok": true, "created": "CHANGELOG.md" }))
        }
        ChangelogCmd::Generate { version } => {
            let changelog_path = ctx.project_root.join("CHANGELOG.md");
            let existing = if changelog_path.exists() {
                fs::read_to_string(&changelog_path)
                    .with_context(|| format!("read {}", changelog_path.display()))?
            } else {
                String::new()
            };

            let plan = crate::store::load_plan(ctx)?;
            let all_milestones = crate::store::load_all_milestones(ctx)?;

            let mut entries = Vec::new();
            for r in &plan.releases {
                if r.version == version
                    || version.trim_start_matches('v') == r.version.trim_start_matches('v')
                {
                    for m_id in &r.milestones {
                        if let Some((_, m)) =
                            all_milestones.iter().find(|(_, m)| m.milestone.id == *m_id)
                        {
                            let wp_sections: Vec<String> = m
                                .work_packages
                                .iter()
                                .map(|wp| {
                                    let steps: Vec<String> = m
                                        .steps
                                        .iter()
                                        .filter(|s| s.work_package == wp.id && s.status == "done")
                                        .map(|s| format!("  - {}", s.action))
                                        .collect();
                                    if steps.is_empty() {
                                        String::new()
                                    } else {
                                        let mut block = format!("  - {}\n", wp.name);
                                        for s in &steps {
                                            block.push_str(s);
                                            block.push('\n');
                                        }
                                        block
                                    }
                                })
                                .collect();

                            let mut section = format!("### {}\n\n", m.milestone.title);
                            for wp in &wp_sections {
                                if !wp.is_empty() {
                                    section.push_str(wp);
                                    section.push('\n');
                                }
                            }
                            entries.push(section);
                        }
                    }
                    break;
                }
            }

            let today = crate::store::today();
            let version_section = format!(
                "## v{} ({})\n\n{}",
                version.trim_start_matches('v'),
                today,
                entries.join("\n")
            );

            // Check if already in the changelog
            let version_header = format!("## v{}", version.trim_start_matches('v'));
            if existing.contains(&version_header) {
                anyhow::bail!("version {version} already exists in CHANGELOG.md");
            }

            // Insert after the Unreleased section or at the top
            let result = if existing.starts_with("# Changelog") {
                // Replace #[Unreleased] section with #[Unreleased] + new version
                let unreleased_end = existing
                    .find("\n## [Unreleased]")
                    .and_then(|_| existing[10..].find("\n## "))
                    .map(|i| i + 10)
                    .unwrap_or(existing.len());
                let mut out = existing[..unreleased_end].to_string();
                out.push('\n');
                out.push_str(&version_section);
                out.push('\n');
                out.push_str(&existing[unreleased_end..]);
                out
            } else {
                let mut out = String::new();
                out.push_str("# Changelog\n\n");
                out.push_str(&version_section);
                out.push('\n');
                out.push_str(&existing);
                out
            };

            fs::write(&changelog_path, &result)
                .with_context(|| format!("write {}", changelog_path.display()))?;
            emit(
                format,
                &json!({ "ok": true, "version": version, "entries": entries.len() }),
            )
        }
    }
}

fn extract_version(content: &str, version: &str) -> String {
    let mut in_target = false;
    let mut result = String::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            let header_ver = line
                .strip_prefix("## ")
                .and_then(|h| h.split_whitespace().next())
                .unwrap_or("");
            let matches =
                header_ver.trim_start_matches('v').trim() == version.trim_start_matches('v');
            if matches {
                in_target = true;
                result.push_str(line);
                result.push('\n');
                continue;
            } else if in_target {
                break;
            }
        }
        if in_target {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

fn add_entry(content: &str, version: &str, section: &str, entry: &str) -> Result<String> {
    let bullet = format!("- {entry}");
    let section_header = format!("### {section}");
    let version_header = format!("## v{}", version.trim_start_matches('v'));

    if content.contains(&bullet) {
        return Ok(content.to_string());
    }

    let lines: Vec<&str> = content.lines().collect();
    let mut result = String::new();
    let mut added = false;

    let version_idx = lines.iter().position(|line| {
        if let Some(h) = line.strip_prefix("## ") {
            let hv = h.split_whitespace().next().unwrap_or("");
            return hv.trim_start_matches('v').trim() == version.trim_start_matches('v');
        }
        false
    });

    if let Some(vi) = version_idx {
        let mut next_version = lines.len();
        for (i, line) in lines.iter().enumerate().skip(vi + 1) {
            if line.starts_with("## ") && !line.starts_with("### ") {
                next_version = i;
                break;
            }
        }

        let section_idx = (vi + 1..next_version).position(|i| lines[i].trim() == section_header);

        if let Some(si) = section_idx {
            let si = si + vi + 1;
            let mut insert_at = si + 1;
            for (i, line) in lines
                .iter()
                .enumerate()
                .skip(si + 1)
                .take(next_version - (si + 1))
            {
                if !line.is_empty() && !line.starts_with("- ") {
                    insert_at = i;
                    break;
                }
                insert_at = i + 1;
            }

            for (i, line) in lines.iter().enumerate() {
                result.push_str(line);
                result.push('\n');
                if i == insert_at - 1 {
                    result.push_str(&bullet);
                    result.push('\n');
                    added = true;
                }
            }
        } else {
            // Version exists but section does not: insert section immediately
            // after the version header (blank line optional).
            for (i, line) in lines.iter().enumerate() {
                result.push_str(line);
                result.push('\n');
                if i == vi {
                    result.push_str(&section_header);
                    result.push('\n');
                    result.push_str(&bullet);
                    result.push('\n');
                    result.push('\n');
                    added = true;
                }
            }
        }
    }

    if !added {
        // Missing version section: preserve full history then insert the new
        // block near the top (M170 F-07 / F-02 residual). Pre-fix left
        // `result` empty and wrote only the new header — wiping CHANGELOG.md.
        let new_block = format!("{version_header}\n\n{section_header}\n{bullet}\n");
        return Ok(insert_new_version_section(content, &new_block));
    }

    Ok(result)
}

/// Insert a brand-new `## vX` block while keeping every existing line.
/// Newest version is placed before the first existing `## ` header
/// (keep-a-changelog order); if none exists, the block is appended.
fn insert_new_version_section(content: &str, new_block: &str) -> String {
    if content.is_empty() {
        return format!("# Changelog\n\n{new_block}");
    }
    if let Some(idx) = content.find("\n## ") {
        let mut out = content[..=idx].to_string();
        out.push_str(new_block);
        if !new_block.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&content[idx + 1..]);
        return out;
    }
    let mut out = content.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(new_block);
    out
}
