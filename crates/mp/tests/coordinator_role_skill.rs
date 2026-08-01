use std::fs;

use common::repo_root;

mod common;

fn read_md(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| panic!("missing: {}", path.display()))
}

fn skills_dir() -> std::path::PathBuf {
    repo_root().join("templates/skills")
}

#[test]
fn mp_coordinator_walks_planning_then_reviewing() {
    let coord_dir = skills_dir().join("mp-coordinator");
    assert!(coord_dir.is_dir(), "mp-coordinator skill dir must exist");

    let skill_md = read_md(&coord_dir.join("SKILL.md"));
    let planning_md = read_md(&coord_dir.join("planning.md"));
    let reviewing_md = read_md(&coord_dir.join("reviewing.md"));
    let spec_co_design_md = read_md(&coord_dir.join("spec-co-design.md"));

    // AC-01: SKILL.md has role identity, sub-mode map, hand-off contract
    assert!(
        skill_md.contains("coordinator"),
        "SKILL.md must contain 'coordinator' (role identity)"
    );
    let section_count = skill_md.lines().filter(|l| l.starts_with("## ")).count();
    assert!(
        section_count >= 3,
        "SKILL.md must have >= 3 top-level sections, got {}",
        section_count
    );
    assert!(
        skill_md.contains("Stage ownership"),
        "SKILL.md must have stage ownership table"
    );
    assert!(
        skill_md.contains("Sub-mode map"),
        "SKILL.md must have sub-mode map"
    );
    assert!(
        skill_md.contains("Hand-off contract"),
        "SKILL.md must have hand-off contract"
    );

    // AC-02: planning.md is self-contained (CLI contract, fragment discipline,
    // state updates, per-profile checklists inline) — no back-link to a
    // separate master-planner skill.
    assert!(
        !planning_md.contains("master-planner"),
        "planning.md must not back-link to a master-planner skill"
    );
    assert!(
        planning_md.contains("Stage 1"),
        "planning.md must cover Stage 1 (Draft)"
    );
    assert!(
        planning_md.contains("Stage 4"),
        "planning.md must cover Stage 4 (Approve)"
    );

    // AC-03: reviewing.md inlines the lesson-pattern pre-screen (no
    // external lessons doc, no L-codes, no mp-code-review dependency).
    assert!(
        reviewing_md.contains("author should not be the only reviewer")
            || reviewing_md.contains("author should not be the only"),
        "reviewing.md must inline the session-boundary discipline prose"
    );
    assert!(
        !reviewing_md.contains("docs/code-review-lessons.md"),
        "reviewing.md must not point at the archived lessons doc"
    );
    assert!(
        !reviewing_md.contains("lesson-pattern library"),
        "reviewing.md must not point at a non-existent external library"
    );
    assert!(
        !reviewing_md.contains("mp-code-review"),
        "reviewing.md must not reference the repo-internal mp-code-review skill"
    );
    assert!(
        reviewing_md.to_lowercase().contains("two-round"),
        "reviewing.md must reference two-round review"
    );
    assert!(
        reviewing_md.contains("Stage 8"),
        "reviewing.md must cover Stage 8 (External review)"
    );
    assert!(
        reviewing_md.contains("Stage 10"),
        "reviewing.md must cover Stage 10 (Re-review)"
    );

    // AC-04: spec-co-design.md references spec-grill
    assert!(
        spec_co_design_md.contains("spec-grill"),
        "spec-co-design.md must reference spec-grill"
    );

    // Single-source invariant: sub-mode files reference primary tools, don't inline
    for (file, name) in [
        (&planning_md, "planning.md"),
        (&reviewing_md, "reviewing.md"),
        (&spec_co_design_md, "spec-co-design.md"),
    ] {
        assert!(
            !file.contains("Pattern: "),
            "{} must not duplicate lesson patterns (single-source invariant)",
            name
        );
    }

    // Cross-reference: mp-flow stages.toml coordinator stages match mp-coordinator table
    let flow_toml = read_md(&repo_root().join("templates/skills/mp-flow/stages.toml"));
    assert!(
        flow_toml.contains("[role_binding.coordinator]"),
        "mp-flow stages.toml must have coordinator role binding"
    );

    // Verify the mp-coordinator manifest exists and has proper structure
    let manifest_path = coord_dir.join("manifest.json");
    assert!(manifest_path.is_file(), "manifest.json must exist");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["id"], "mp-coordinator");
    assert!(manifest["consumes"].is_array());
    let consumes: Vec<&str> = manifest["consumes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(consumes.contains(&"spec-grill"));
    assert!(consumes.contains(&"mp-flow"));
    // mp-coordinator consumes the cross-role mp-flow skill + the
    // spec-grill sub-mode. Pre-M141 also listed master-planner and
    // mp-handoff; both are gone (M141 consolidation).
    assert!(!consumes.contains(&"master-planner"));
    assert!(!consumes.contains(&"mp-handoff"));

    // Verify mp-coordinator install command works (checks the init flow)
    let env = TestEnv::new();
    let out = env.run_at_repo(&[
        "install",
        "--dev",
        "--toolkit-only",
        "--skills",
        "mp-coordinator",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "install --skills=mp-coordinator should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn mp_coordinator_deploys_with_sub_modes() {
    let env = TestEnv::blank();
    let root = repo_root();
    let install_dir = env.tmp.path().join("install-target");

    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", &install_dir)
        .arg("--plan-dir")
        .arg(env.tmp.path().join("master-plan"))
        .arg("install")
        .arg("--dev")
        .arg("--harness")
        .arg("opencode")
        .arg("--skills")
        .arg("mp-coordinator")
        .arg("--format")
        .arg("json");
    common::isolated_harness_env(&mut cmd, env.tmp.path());

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install --skills=mp-coordinator should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify mp-coordinator files are deployed
    let skill_dir = env
        .tmp
        .path()
        .join("harness/opencode/skills/mp-coordinator");
    assert!(skill_dir.is_dir(), "mp-coordinator skill dir should exist");

    let skill_md = skill_dir.join("SKILL.md");
    assert!(skill_md.is_file(), "SKILL.md should be deployed");
    let content = fs::read_to_string(&skill_md).unwrap();
    assert!(
        content.contains("coordinator"),
        "deployed SKILL.md should contain the role identity"
    );

    // Verify sub-mode files are also deployed (they're in the same dir)
    // The install copies the SKILL.md to the harness target; sub-mode files
    // are referenced by the SKILL.md but not individually copied to the harness
    // (the agent reads them from the source template directory or from the
    // bundled templates/ directory at MP_HOME).
    let templates_dir = env
        .tmp
        .path()
        .join("install-target/templates/skills/mp-coordinator");
    assert!(
        templates_dir.join("SKILL.md").is_file(),
        "SKILL.md should be in templates"
    );
    assert!(
        templates_dir.join("planning.md").is_file(),
        "planning.md should be in templates"
    );
    assert!(
        templates_dir.join("reviewing.md").is_file(),
        "reviewing.md should be in templates"
    );
    assert!(
        templates_dir.join("spec-co-design.md").is_file(),
        "spec-co-design.md should be in templates"
    );
}

/// Inline TestEnv for this test file (avoids importing common::TestEnv
/// which requires `common::repo_root`).
struct TestEnv {
    pub tmp: tempfile::TempDir,
}

impl TestEnv {
    fn new() -> Self {
        let env = Self::blank();
        assert!(
            env.run(&["init", "--profile", "full", "--format", "json"])
                .status
                .success(),
            "mp init --profile full failed"
        );
        env
    }

    fn blank() -> Self {
        Self {
            tmp: tempfile::TempDir::new().expect("temp"),
        }
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        let args = args.to_vec();
        common::run_with_retry(
            || {
                let mut cmd = std::process::Command::new(common::mp_bin());
                cmd.current_dir(self.tmp.path())
                    .env("MP_HOME", repo_root())
                    .args(&args);
                cmd
            },
            2,
        )
    }

    fn run_at_repo(&self, args: &[&str]) -> std::process::Output {
        // Redirect install output into the test's temp dir so the test never
        // mutates (or depends on) the user's real ~/.agents/master-plan.
        // Without this, a stale orphan there (e.g. a post-M141 master-planner
        // dir) made the test fail on the host's global install state.
        let install_dir = self.tmp.path().join("install-target");
        common::run_with_retry(
            || {
                let mut cmd = std::process::Command::new(common::mp_bin());
                cmd.current_dir(repo_root())
                    .env("MP_HOME", repo_root())
                    .env("MP_INSTALL_DIR", &install_dir)
                    .args(args);
                cmd
            },
            2,
        )
    }
}
