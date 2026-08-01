//! M146: deployment manifest, list-skills, drift detection.
//!
//! Complements the existing install_skills_v2.rs with the M146-specific
//! surfaces:
//!   * `installed-skills.json` is written/read with full provenance
//!   * bare `mp install` deploys only category=core skills
//!   * `mp install --list-skills` reports deployment state per harness
//!   * `mp install --check` reports orphans / un-deployed registry skills
//!   * `mp uninstall` reads the manifest, prunes exactly what was
//!     deployed (M141 stale-copy case)

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::TestEnv;

fn mp_bin() -> &'static Path {
    common::mp_bin()
}

fn workspace_root() -> PathBuf {
    common::repo_root()
}

fn run_at(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(mp_bin());
    cmd.current_dir(workspace_root())
        .env("MP_HOME", workspace_root())
        .env("MP_INSTALL_DIR", env.tmp.path());
    common::isolated_harness_env(&mut cmd, env.tmp.path());
    cmd.args(args);
    cmd.output().expect("failed to run mp")
}

fn init_clean_plan(env: &TestEnv) {
    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
}

// ---------------------------------------------------------------------------
// AC-01: installed-skills.json is written with full provenance
// ---------------------------------------------------------------------------

#[test]
fn install_writes_installed_skills_manifest() {
    let env = TestEnv::blank();
    init_clean_plan(&env);
    let install_dir = env.tmp.path();

    let out = run_at(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest_path = install_dir.join("installed-skills.json");
    assert!(
        manifest_path.is_file(),
        "manifest not written at {manifest_path:?}"
    );
    let raw = std::fs::read_to_string(&manifest_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let entries = v["entries"].as_array().expect("entries array");
    assert!(
        entries
            .iter()
            .any(|e| e["skill_id"] == "mp-flow" && !e["category"].as_str().unwrap_or("").is_empty()),
        "mp-flow entry should be present with category; got: {entries:?}"
    );
    let flow = entries.iter().find(|e| e["skill_id"] == "mp-flow").unwrap();
    assert_eq!(flow["category"], "core", "mp-flow should be category=core");
    assert!(
        flow["installed_at"].is_string() && !flow["installed_at"].as_str().unwrap_or("").is_empty(),
        "installed_at should be RFC3339"
    );
}

// ---------------------------------------------------------------------------
// AC-02: bare `mp install` deploys only category=core skills
// ---------------------------------------------------------------------------

#[test]
fn bare_install_deploys_only_core() {
    let env = TestEnv::blank();
    init_clean_plan(&env);

    let out = run_at(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());

    let manifest_path = env.tmp.path().join("installed-skills.json");
    let raw = std::fs::read_to_string(&manifest_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let entries = v["entries"].as_array().unwrap();
    // Bare install must NOT deploy catalog skills.
    for e in entries {
        assert_eq!(
            e["category"], "core",
            "bare install must NOT deploy catalog skills; got: {e:?}"
        );
    }
    assert!(
        entries.iter().all(|e| e["skill_id"] != "spec-grill"),
        "spec-grill (catalog) must not appear in bare install manifest"
    );
    assert!(
        entries.iter().all(|e| e["skill_id"] != "diagnosing-bugs"),
        "diagnosing-bugs (catalog) must not appear in bare install manifest"
    );
    assert!(
        entries.iter().all(|e| e["skill_id"] != "codebase-design"),
        "codebase-design (catalog) must not appear in bare install manifest"
    );
}

#[test]
fn opt_in_catalog_via_skills_flag() {
    let env = TestEnv::blank();
    init_clean_plan(&env);
    // --skills=diagnosing-bugs picks up a catalog skill explicitly.
    let out = run_at(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "diagnosing-bugs",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "diagnosing-bugs install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Manifest must include the catalog skill.
    let raw = std::fs::read_to_string(env.tmp.path().join("installed-skills.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let entries = v["entries"].as_array().unwrap();
    assert!(
        entries.iter().any(|e| e["skill_id"] == "diagnosing-bugs"),
        "diagnosing-bugs should be in the manifest; got: {entries:?}"
    );
}

#[test]
fn unknown_skill_error_lists_available() {
    let env = TestEnv::blank();
    init_clean_plan(&env);
    let out = run_at(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "nope-not-a-skill",
            "--format",
            "json",
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("diagnosing-bugs"),
        "error should list catalog skill: {stderr}"
    );
    assert!(
        stderr.contains("codebase-design"),
        "error should list catalog skill: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// AC-03: --list-skills reports deployment state per harness
// ---------------------------------------------------------------------------

#[test]
fn list_skills_reports_deployment_state() {
    let env = TestEnv::blank();
    init_clean_plan(&env);

    let out = run_at(
        &env,
        &["install", "--list-skills", "--dev", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "list-skills failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let skills = v["skills"].as_array().expect("skills array");
    let by_id: std::collections::HashMap<String, &serde_json::Value> = skills
        .iter()
        .map(|s| (s["id"].as_str().unwrap().to_string(), s))
        .collect();
    // Core skills present.
    assert!(by_id.contains_key("mp-flow"), "mp-flow missing from list");
    assert_eq!(by_id["mp-flow"]["category"], "core");
    // Catalog skills present.
    assert!(
        by_id.contains_key("diagnosing-bugs"),
        "diagnosing-bugs missing from list"
    );
    assert_eq!(by_id["diagnosing-bugs"]["category"], "catalog");
    assert!(
        by_id.contains_key("codebase-design"),
        "codebase-design missing from list"
    );
    // source + source_url populated for catalog.
    assert!(!by_id["diagnosing-bugs"]["source"]
        .as_str()
        .unwrap_or("")
        .is_empty());
    assert!(!by_id["diagnosing-bugs"]["source_url"]
        .as_str()
        .unwrap_or("")
        .is_empty());
    // deployed_to list (empty here, --toolkit-only).
    assert!(by_id["mp-flow"]["deployed_to"].is_array());
}

// ---------------------------------------------------------------------------
// AC-04: --check detects drift (orphan / un-deployed)
// ---------------------------------------------------------------------------

#[test]
fn check_reports_orphans_and_undeployed() {
    let env = TestEnv::blank();
    init_clean_plan(&env);

    // Seed the manifest with an orphan entry (skill that no longer
    // exists in the registry) and confirm --check surfaces it.
    let manifest_path = env.tmp.path().join("installed-skills.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "entries": [
                {
                    "skill_id": "master-planner",
                    "harness": "opencode",
                    "category": "core",
                    "installed_at": "2026-01-01T00:00:00Z"
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let out = run_at(&env, &["install", "--check", "--dev", "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = v["warnings"].as_array().cloned().unwrap_or_default();
    let warnings_joined = serde_json::to_string(&warnings).unwrap();
    assert!(
        warnings_joined.contains("master-planner") && warnings_joined.contains("orphan"),
        "expected orphan warning for master-planner; got: {warnings_joined}"
    );
    // diagnosing-bugs (catalog) was NOT in the manifest → should
    // be flagged as un-deployed.
    let undep_warning_present = warnings
        .iter()
        .any(|w| w.to_string().contains("diagnosing-bugs"));
    assert!(
        undep_warning_present,
        "expected un-deployed warning for diagnosing-bugs; got: {warnings:?}"
    );
}

// ---------------------------------------------------------------------------
// AC-05: mp uninstall reads the manifest and prunes exactly what was
// deployed. Simulates the M141 stale-copy case: a skill that the
// registry no longer ships is still pruned because the manifest is
// authoritative.
// ---------------------------------------------------------------------------

#[test]
fn uninstall_prunes_orphan_skill_via_manifest() {
    let env = TestEnv::blank();
    init_clean_plan(&env);

    // Override the opencode skill dir to a path under our temp dir
    // so we can observe the manifest-driven uninstall without touching
    // the developer's real ~/.agents/skills.
    let fake_skill_root = env.tmp.path().join("harness/opencode/skills");
    let skill_dir = fake_skill_root.join("master-planner");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), "# stale master-planner").unwrap();

    // Pre-seed the deployment manifest with the orphan entry.
    let manifest_path = env.tmp.path().join("installed-skills.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "entries": [
                {
                    "skill_id": "master-planner",
                    "harness": "opencode",
                    "category": "core",
                    "installed_at": "2026-01-01T00:00:00Z"
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let mut cmd = Command::new(mp_bin());
    cmd.current_dir(workspace_root())
        .env("MP_HOME", workspace_root())
        .env("MP_INSTALL_DIR", env.tmp.path())
        .env("MP_OPENCODE_SKILL_DIR", &fake_skill_root)
        .args(["uninstall", "--harness", "opencode", "--format", "json"]);
    let out = cmd.output().expect("failed to run mp");
    assert!(
        out.status.success(),
        "uninstall failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !skill_dir.exists(),
        "master-planner skill dir should be removed by manifest-driven uninstall"
    );

    // Manifest entry should also be pruned.
    let raw = std::fs::read_to_string(&manifest_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let entries = v["entries"].as_array().unwrap();
    assert!(
        entries.is_empty(),
        "manifest entries should be pruned after uninstall; got: {entries:?}"
    );
}
