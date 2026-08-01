use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use serde::Serialize;
use walkdir::WalkDir;

use crate::paths::PlanContext;

#[derive(Debug, Serialize)]
pub struct BrownfieldScanReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    pub signals: Vec<ScanSignal>,
    pub gaps: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Serialize)]
pub struct ScanSignal {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub confidence: String,
}

pub fn scan(
    ctx: &PlanContext,
    domain: Option<&str>,
    query: Option<&str>,
) -> Result<BrownfieldScanReport> {
    let root = &ctx.project_root;
    let mut signals = Vec::new();
    let mut gaps = Vec::new();

    if root.join("src").is_dir() {
        signals.push(signal_path("entry_point", root.join("src"), "high"));
    } else if root.join("lib").is_dir() {
        signals.push(signal_path("entry_point", root.join("lib"), "high"));
    }

    for manifest in ["Cargo.toml", "package.json", "pyproject.toml", "go.mod"] {
        let p = root.join(manifest);
        if p.is_file() {
            signals.push(signal_path("manifest", p, "high"));
        }
    }

    if root.join("tests").is_dir() {
        signals.push(signal_path("test", root.join("tests"), "medium"));
    }

    let search_terms: Vec<String> = match (domain, query) {
        (Some(d), Some(q)) => vec![d.to_string(), q.to_string()],
        (Some(d), None) => vec![d.to_string()],
        (None, Some(q)) => vec![q.to_string()],
        (None, None) => vec![],
    };

    for term in &search_terms {
        let hits = ripgrep_hits(root, term, 12)?;
        if hits.is_empty() {
            gaps.push(format!("No files matching \"{term}\""));
        }
        for hit in hits {
            let kind = classify_hit(&hit, term);
            signals.push(ScanSignal {
                kind,
                path: Some(relative_path(root, &hit)),
                key: None,
                confidence: "medium".to_string(),
            });
        }
    }

    if let Some(d) = domain {
        let domain_dir = root.join("src").join(d);
        if !domain_dir.exists() {
            gaps.push(format!("No src/{d}/ directory"));
        }
        scan_config_keys(root, d, &mut signals);
    }

    if query.is_some() && signals.iter().filter(|s| s.kind == "test").count() == 0 {
        gaps.push("No tests matching query".to_string());
    }

    dedupe_signals(&mut signals);

    Ok(BrownfieldScanReport {
        domain: domain.map(str::to_string),
        query: query.map(str::to_string),
        signals,
        gaps,
        notes: "Use code zone search in harness; this command is structured assist.".to_string(),
    })
}

fn signal_path(kind: &str, path: PathBuf, confidence: &str) -> ScanSignal {
    ScanSignal {
        kind: kind.to_string(),
        path: Some(path.to_string_lossy().to_string()),
        key: None,
        confidence: confidence.to_string(),
    }
}

fn classify_hit(path: &Path, term: &str) -> String {
    let s = path.to_string_lossy();
    if s.contains("test") || s.contains("_test.") {
        "test".to_string()
    } else if s.ends_with(".env.example") || s.contains("config") {
        "config".to_string()
    } else if s.contains(term) {
        "match".to_string()
    } else {
        "entry_point".to_string()
    }
}

fn scan_config_keys(root: &Path, domain: &str, signals: &mut Vec<ScanSignal>) {
    let env_example = root.join(".env.example");
    if !env_example.is_file() {
        return;
    }
    let Ok(content) = std::fs::read_to_string(&env_example) else {
        return;
    };
    let needle = domain.to_uppercase();
    for line in content.lines() {
        let key = line.split('=').next().unwrap_or("").trim();
        if key.is_empty() || key.starts_with('#') {
            continue;
        }
        if key.to_uppercase().contains(&needle) {
            signals.push(ScanSignal {
                kind: "config".to_string(),
                path: Some(env_example.to_string_lossy().to_string()),
                key: Some(key.to_string()),
                confidence: "medium".to_string(),
            });
        }
    }
}

fn ripgrep_hits(root: &Path, pattern: &str, limit: usize) -> Result<Vec<PathBuf>> {
    if let Ok(output) = Command::new("rg")
        .args([
            "--files-with-matches",
            "-i",
            pattern,
            "--glob",
            "!target/**",
            "--glob",
            "!node_modules/**",
            "--glob",
            "!.git/**",
        ])
        .current_dir(root)
        .output()
    {
        if output.status.success() || !output.stdout.is_empty() {
            let mut hits = Vec::new();
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if line.is_empty() {
                    continue;
                }
                hits.push(root.join(line));
                if hits.len() >= limit {
                    break;
                }
            }
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
    }

    let needle = pattern.to_lowercase();
    let mut hits = Vec::new();
    for entry in WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(
                name,
                "target" | "node_modules" | ".git" | "master-plan" | ".mp"
            ) {
                continue;
            }
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_lowercase();
        if rel.contains(&needle) {
            hits.push(path.to_path_buf());
            if hits.len() >= limit {
                break;
            }
            continue;
        }
        if let Ok(content) = read_text_capped(path, BROWNFIELD_MAX_READ_BYTES) {
            if content.to_lowercase().contains(&needle) {
                hits.push(path.to_path_buf());
                if hits.len() >= limit {
                    break;
                }
            }
        }
    }
    Ok(hits)
}

const BROWNFIELD_MAX_READ_BYTES: u64 = 512 * 1024;

const SKIP_CONTENT_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "pdf", "zip", "gz", "tgz", "xz", "bz2", "7z",
    "wasm", "so", "dylib", "dll", "o", "a", "rlib", "exe", "bin", "class", "jar", "pyc", "pyo",
    "mp3", "mp4", "mov", "avi", "woff", "woff2", "ttf", "otf", "eot", "sqlite", "db", "lock",
];

fn should_skip_content_scan(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            SKIP_CONTENT_EXTENSIONS
                .iter()
                .any(|s| ext.eq_ignore_ascii_case(s))
        })
        .unwrap_or(false)
}

fn read_text_capped(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    if should_skip_content_scan(path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "skipped binary extension",
        ));
    }
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file too large for brownfield content scan",
        ));
    }
    std::fs::read_to_string(path)
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn dedupe_signals(signals: &mut Vec<ScanSignal>) {
    let mut seen = std::collections::HashSet::new();
    signals.retain(|s| {
        let key = format!(
            "{}:{}:{}",
            s.kind,
            s.path.as_deref().unwrap_or(""),
            s.key.as_deref().unwrap_or("")
        );
        seen.insert(key)
    });
}

pub fn detect_brownfield_likely(project_root: &Path) -> bool {
    let has_code = project_root.join("src").is_dir() || project_root.join("lib").is_dir();
    let has_tests = project_root.join("tests").is_dir()
        || WalkDir::new(project_root)
            .max_depth(4)
            .into_iter()
            .filter_map(|e| e.ok())
            .any(|e| {
                let p = e.path();
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains("test"))
                        .unwrap_or(false)
            });
    let has_manifest = ["Cargo.toml", "package.json", "pyproject.toml", "go.mod"]
        .iter()
        .any(|m| project_root.join(m).is_file());
    has_code && has_tests && has_manifest
}

pub fn detect_stack(project_root: &Path) -> Vec<String> {
    let mut stack = Vec::new();
    if project_root.join("Cargo.toml").is_file() {
        stack.push("rust".to_string());
    }
    if project_root.join("package.json").is_file() {
        stack.push("node".to_string());
    }
    if project_root.join("pyproject.toml").is_file() {
        stack.push("python".to_string());
    }
    if project_root.join("go.mod").is_file() {
        stack.push("go".to_string());
    }
    stack
}
