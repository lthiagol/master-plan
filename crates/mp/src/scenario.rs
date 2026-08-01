use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
pub struct ScenarioManifest {
    pub id: String,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub description: String,
    pub fixture: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    pub assert: ScenarioAssert,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioAssert {
    pub exit_code: i32,
    #[serde(default)]
    pub stdout_json_file: Option<String>,
    #[serde(default)]
    pub stdout_contains: Option<String>,
    #[serde(default)]
    pub fs_unchanged: bool,
}

#[derive(Debug)]
pub struct ScenarioResult {
    pub id: String,
    pub passed: bool,
    pub message: String,
}

pub fn discover_scenarios(scenarios_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(scenarios_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let manifest = entry.path().join("scenario.json");
            if manifest.exists() {
                paths.push(manifest);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn run_scenario(
    repo_root: &Path,
    manifest_path: &Path,
    mp_bin: &Path,
) -> Result<ScenarioResult> {
    let raw = fs::read_to_string(manifest_path)?;
    let manifest: ScenarioManifest = serde_json::from_str(&raw)?;
    let scenario_dir = manifest_path.parent().context("scenario dir")?;

    if manifest.phase == "planned" {
        return Ok(ScenarioResult {
            id: manifest.id,
            passed: true,
            message: "skipped (phase=planned)".to_string(),
        });
    }

    let fixture_root = {
        let joined = repo_root.join("tests/fixtures").join(&manifest.fixture);
        let fixtures = repo_root.join("tests/fixtures");
        let canon_fixtures = fixtures.canonicalize().unwrap_or(fixtures);
        let canon_joined = joined.canonicalize().unwrap_or(joined.clone());
        if !canon_joined.starts_with(&canon_fixtures) {
            return Ok(ScenarioResult {
                id: manifest.id.clone(),
                passed: false,
                message: format!("fixture path escapes tests/fixtures: {}", manifest.fixture),
            });
        }
        joined
    };
    if !fixture_root.is_dir() {
        return Ok(ScenarioResult {
            id: manifest.id.clone(),
            passed: false,
            message: format!("fixture missing: {}", fixture_root.display()),
        });
    }

    let temp = tempfile::tempdir()?;
    copy_dir_recursive(&fixture_root, temp.path())?;
    let snapshot = dir_snapshot(temp.path());

    let mut cmd = Command::new(mp_bin);
    cmd.current_dir(temp.path());
    cmd.env("MP_HOME", repo_root);
    for (k, v) in &manifest.env {
        cmd.env(k, v.replace("{{repo_root}}", &repo_root.to_string_lossy()));
    }
    for arg in &manifest.command {
        cmd.arg(arg);
    }

    let output = cmd
        .output()
        .with_context(|| format!("run scenario {}", manifest.id))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(1);

    if exit_code != manifest.assert.exit_code {
        return Ok(ScenarioResult {
            id: manifest.id,
            passed: false,
            message: format!(
                "exit code {exit_code}, expected {}; stderr: {stderr}",
                manifest.assert.exit_code
            ),
        });
    }

    if let Some(expected_rel) = &manifest.assert.stdout_json_file {
        if expected_rel.contains("..") || Path::new(expected_rel).is_absolute() {
            return Ok(ScenarioResult {
                id: manifest.id,
                passed: false,
                message: format!("stdout_json_file escapes scenario dir: {expected_rel}"),
            });
        }
        let expected_path = scenario_dir.join(expected_rel);
        let expected_raw = fs::read_to_string(&expected_path)?;
        let expected: Value = serde_json::from_str(&expected_raw)?;
        let actual: Value = serde_json::from_str(&stdout)
            .with_context(|| format!("stdout not json for {}", manifest.id))?;
        if !json_contains(&expected, &actual) {
            return Ok(ScenarioResult {
                id: manifest.id,
                passed: false,
                message: format!("stdout mismatch\nexpected: {expected}\nactual: {actual}"),
            });
        }
    }

    if let Some(needle) = &manifest.assert.stdout_contains {
        if !stdout.contains(needle) {
            return Ok(ScenarioResult {
                id: manifest.id,
                passed: false,
                message: format!("stdout missing substring: {needle}"),
            });
        }
    }

    if manifest.assert.fs_unchanged {
        let after = dir_snapshot(temp.path());
        if after != snapshot {
            return Ok(ScenarioResult {
                id: manifest.id,
                passed: false,
                message: "filesystem changed but fs_unchanged=true".to_string(),
            });
        }
    }

    Ok(ScenarioResult {
        id: manifest.id,
        passed: true,
        message: "ok".to_string(),
    })
}

pub fn run_all_implemented(repo_root: &Path, mp_bin: &Path) -> Result<Vec<ScenarioResult>> {
    let scenarios_dir = repo_root.join("tests/scenarios");
    let manifests = discover_scenarios(&scenarios_dir)?;
    let mut results = Vec::new();
    for manifest in manifests {
        results.push(run_scenario(repo_root, &manifest, mp_bin)?);
    }
    Ok(results)
}

fn json_contains(expected: &Value, actual: &Value) -> bool {
    match expected {
        Value::Object(exp) => {
            let act = match actual.as_object() {
                Some(o) => o,
                None => return false,
            };
            exp.iter()
                .all(|(k, v)| act.get(k).map(|a| json_contains(v, a)).unwrap_or(false))
        }
        Value::Array(exp) => {
            let act = match actual.as_array() {
                Some(a) => a,
                None => return false,
            };
            if exp.len() != act.len() {
                return false;
            }
            exp.iter().zip(act.iter()).all(|(e, a)| json_contains(e, a))
        }
        _ => expected == actual,
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn dir_snapshot(root: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.file_type().is_file() {
            let rel = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .to_string();
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push((rel, len));
        }
    }
    out.sort();
    out
}

#[allow(dead_code)]
pub fn assert_all_passed(results: &[ScenarioResult]) -> Result<()> {
    let failures: Vec<_> = results
        .iter()
        .filter(|r| !r.passed && r.message != "skipped (phase=planned)")
        .collect();
    if failures.is_empty() {
        return Ok(());
    }
    let msg = failures
        .iter()
        .map(|r| format!("{}: {}", r.id, r.message))
        .collect::<Vec<_>>()
        .join("\n");
    bail!("scenario failures:\n{msg}");
}
