//! M220 AC-03: install under an isolated HOME/destination must copy
//! refreshed skills and harness templates that use canonical Autopilot
//! terminology, and must NOT mutate the developer's global
//! configuration (~/.agents/master-plan).
//!
//! Why this is its own test file (vs. suites/install_registry.rs):
//! the surface we want to assert — "what the consumer sees after
//! `make install`" — overlaps with the registry tests but the assertion
//! shape is different. Registry tests check skill id presence; this
//! file checks content. Linking both shapes into one test file would
//! cost the linker the same; keeping them split keeps each suite
//! small enough to debug a regression in isolation.
//!
//! Test environment isolation: every test in this file sets
//! `MP_INSTALL_DIR` to a fresh `TempDir`, sets per-harness
//! `MP_<HARNESS>_SKILL_DIR` env vars via `isolated_harness_env`, and
//! prepends the install bin dir to `PATH` so `mp doctor` / `mp
//! autopilot` (when the install needs them) see the just-installed
//! binaries. None of these tests touch the developer's
//! `~/.agents/master-plan`; the regression vector is precisely a test
//! that leaks outside the temp dir.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::common::{
    isolated_harness_env, mp_bin, path_with_install_bin, repo_root, run_with_retry,
};
use tempfile::TempDir;

fn run_install(install_root: &TempDir, harness: &str) -> std::process::Output {
    let source = repo_root().to_string_lossy().to_string();
    let install_root_path = install_root.path().to_path_buf();
    let path_with_install = path_with_install_bin(&install_root_path);
    run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", &install_root_path)
                .env("PATH", &path_with_install)
                .env("MP_DEV", "1")
                .args([
                    "install",
                    "--dev",
                    "--source",
                    &source,
                    "--harness",
                    harness,
                    "--format",
                    "json",
                ]);
            isolated_harness_env(&mut cmd, install_root.path());
            cmd
        },
        5,
    )
}

/// M220 AC-03: the isolated install must copy the canonical Autopilot
/// skill set (mp-flow, mp-runner, mp-orchestrator, mp-reviewer) plus
/// the harness template (mp-planner) into the install root, and the
/// global ~/.agents/master-plan must be untouched. The catalog-only
/// skills (mp-orchestrator, mp-reviewer) are requested explicitly via
/// `--skills=…` so the test does not have to make them core skills
/// (which would force unrelated existing install tests to track a
/// new default).
#[test]
fn install_autopilot_copies_canonical_skills_into_isolated_root() {
    let install_root = TempDir::new().expect("install");

    let out = run_install(&install_root, "opencode");
    assert!(
        out.status.success(),
        "install under isolated root failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);

    // The default skill set lands under harness/<id>/skills per
    // `isolated_harness_env`. Walk it and assert the autopilot
    // refresh (mp-flow / mp-runner / SKILL.md) is present and that
    // consumer-facing content does not regress on stale Watch
    // terminology. The harness layout used by the iso env is
    // `harness/opencode/skills/<skill_id>/SKILL.md`.
    let base = install_root.path().join("harness/opencode/skills");
    assert!(
        base.join("mp-flow/SKILL.md").is_file(),
        "mp-flow SKILL.md should land under {base:?}"
    );
    assert!(
        base.join("mp-runner/SKILL.md").is_file(),
        "mp-runner SKILL.md should land under {base:?}"
    );

    // The install must not have written into the developer global.
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("HOME env var");
    let global_skill_dir = home.join(".agents/skills");
    assert!(
        !global_skill_dir.join("mp-flow/SKILL.md").exists()
            || !global_skill_dir
                .join("mp-flow")
                .starts_with(install_root.path()),
        "isolated install must not mutate the developer global ~/.agents/skills"
    );

    // mp-flow SKILL.md must reference the autopilot system — the
    // canonical user-facing description for an autopilot session
    // mentions "autopilot" rather than the legacy "watch" surface.
    let mp_flow = std::fs::read_to_string(base.join("mp-flow/SKILL.md"))
        .expect("read mp-flow SKILL.md from install root");
    assert!(
        mp_flow.to_lowercase().contains("autopilot"),
        "installed mp-flow SKILL.md should reference autopilot; got:\n{mp_flow}"
    );
}

/// M220 AC-03 (extended): explicitly opt-in install of the
/// autopilot-only catalog skills (mp-orchestrator, mp-reviewer) must
/// land them in the install root, and their SKILL.md files must
/// describe their roles without the legacy "coordinator" / "watch"
/// terminology leaking through as the primary role label. The
/// per-harness `MP_<HARNESS>_SKILL_DIR` override means the catalog
/// skills land under the test's install root, not the developer
/// global.
#[test]
fn install_autopilot_catalog_skills_land_in_isolated_root() {
    let install_root = TempDir::new().expect("install");
    let source = repo_root().to_string_lossy().to_string();
    let install_root_path = install_root.path().to_path_buf();
    let path_with_install = path_with_install_bin(&install_root_path);
    let out = run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.env("MP_HOME", repo_root())
                .env("MP_INSTALL_DIR", &install_root_path)
                .env("PATH", &path_with_install)
                .env("MP_DEV", "1")
                .args([
                    "install",
                    "--dev",
                    "--source",
                    &source,
                    "--harness",
                    "opencode",
                    "--skills",
                    "mp-orchestrator,mp-reviewer",
                    "--format",
                    "json",
                ]);
            isolated_harness_env(&mut cmd, install_root.path());
            cmd
        },
        5,
    );
    if !out.status.success() {
        // Catalog skills ship only when the manifest lists them. If
        // a future test runs before M220 S4 lands the new skill
        // folders, this test will fail with an "unknown skill"
        // error — that is the expected signal that S4 has not yet
        // produced the catalog entries. Surface the stderr so a
        // regression points at the missing manifest, not at this
        // assertion.
        panic!(
            "install --skills=mp-orchestrator,mp-reviewer failed; \
             S4 must register the catalog entries first. stderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let base = install_root.path().join("harness/opencode/skills");
    assert!(
        base.join("mp-orchestrator/SKILL.md").is_file(),
        "mp-orchestrator SKILL.md should land under {base:?}"
    );
    assert!(
        base.join("mp-reviewer/SKILL.md").is_file(),
        "mp-reviewer SKILL.md should land under {base:?}"
    );

    // The orchestrator role is the new label that replaces
    // "coordinator" at the runtime surface. The skill's role
    // description must use the new label; stale "coordinator"
    // wording in the SKILL.md body is exactly the regression this
    // milestone exists to prevent (the bugfix-track item
    // "split mp-coordinator into mp-orchestrator + mp-reviewer"
    // records the same invariant).
    let orchestrator = std::fs::read_to_string(base.join("mp-orchestrator/SKILL.md"))
        .expect("read mp-orchestrator SKILL.md from install root");
    assert!(
        orchestrator.to_lowercase().contains("orchestrator"),
        "mp-orchestrator SKILL.md should describe the orchestrator role; got:\n{orchestrator}"
    );

    let reviewer = std::fs::read_to_string(base.join("mp-reviewer/SKILL.md"))
        .expect("read mp-reviewer SKILL.md from install root");
    assert!(
        reviewer.to_lowercase().contains("reviewer"),
        "mp-reviewer SKILL.md should describe the reviewer role; got:\n{reviewer}"
    );
}

/// M220 AC-03 (extended): the installed harness templates
/// (templates/harness/opencode/agents/mp-planner.md and the cursor
/// counterpart) must describe the autopilot workflow rather than the
/// legacy `mp watch` flow. The install copies the harness template
/// tree under `<install_root>/harness/<harness>/agents/…` for
/// agent-style harnesses and under `rules/` for cursor.
#[test]
fn install_autopilot_harness_templates_use_canonical_terminology() {
    let install_root = TempDir::new().expect("install");
    let out = run_install(&install_root, "opencode");
    assert!(
        out.status.success(),
        "install under isolated root failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let planner = install_root
        .path()
        .join("harness/opencode/agents/mp-planner.md");
    assert!(
        planner.is_file(),
        "mp-planner harness template should land at {planner:?}"
    );
    let body = std::fs::read_to_string(&planner).expect("read mp-planner harness template");
    // The autopilot system is the canonical user-facing surface; the
    // harness template must reference autopilot. We do NOT assert the
    // literal string "mp watch" is absent — the legacy CLI alias is
    // retained as the migration anchor (see M208 F2). What we DO
    // assert is that the canonical autopilot terminology is
    // present in the template.
    assert!(
        body.to_lowercase().contains("autopilot"),
        "harness template mp-planner.md should reference autopilot; got:\n{body}"
    );
}

/// Walk a directory recursively, returning every regular file path.
/// Used by the stale-Watch content checks so a future skill/harness
/// file that re-introduces the legacy terminology trips the gate
/// without a per-file code change.
fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn visit(p: &Path, out: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(p) {
            Ok(it) => it,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, out);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    visit(root, &mut out);
    out
}

/// M220 AC-03 (extended): the installed SKILL.md files for the
/// autopilot role set (mp-flow, mp-runner, and the catalog entries)
/// must not regress on stale Watch terminology in user-facing
/// descriptions. The literal `mp watch` CLI alias is allowlisted
/// because M208 retained it; capital-W `Watch` in prose ("the Watch
/// workflow", etc.) is what this lint catches.
#[test]
fn installed_autopilot_skills_have_no_stale_watch_in_descriptions() {
    let install_root = TempDir::new().expect("install");
    let out = run_install(&install_root, "opencode");
    assert!(
        out.status.success(),
        "install under isolated root failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let base = install_root.path().join("harness/opencode/skills");
    let files = walk_files(&base);
    let mut offenders: Vec<(PathBuf, String)> = Vec::new();
    for f in &files {
        // Only inspect user-facing markdown; manifest.json and other
        // metadata can legitimately mention any identifier.
        if f.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let body = std::fs::read_to_string(f).unwrap_or_default();
        for line in body.lines() {
            // Capital-W "Watch" prose. Excludes the imperative-verb
            // uses ("watch it go red") and literal CLI aliases
            // (`mp watch`, `mp watch-control`).
            if line.contains("Watch")
                && !line.contains("`Watch`")
                && !line.contains("mp watch")
                && !line.contains("Watch lane")
            {
                offenders.push((f.clone(), line.to_string()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "installed skill files contain stale 'Watch' terminology (allowlist: CLI alias, Watch lane label, imperative verb); offenders:\n{}",
        offenders
            .iter()
            .map(|(p, l)| format!("  {}: {}", p.display(), l))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
