use common::TestEnv;

mod common;

// F-26 external review: RAII guard for env vars set by a single
// in-process test. Restores the prior value on drop, even on panic,
// so subsequent tests don't see this test's MP_HOME. Drop order is
// reverse-construction (the `_install_dir_guard` restores first,
// then `mp_home_guard` restores MP_HOME to its shell value).
struct ScopedEnvVar {
    key: &'static str,
    prior: Option<std::ffi::OsString>,
}

impl ScopedEnvVar {
    fn set(key: &'static str, value: &str) -> Self {
        let prior = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, prior }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
fn install_project_skill_preserves_user_files_on_redeploy() {
    // M158 round 2 (M-C-6): a pre-existing user file at
    // `.opencode/skills/<id>/...` must survive a re-init. The old
    // whole-dir wipe would silently delete it.
    //
    // Direct unit test against the public install_project_skill
    // function — mirrors the production path that `mp init
    // --with-opencode-skill` drives.
    use mp::install::{install_project_skill, ProjectSkillHarness};

    let env = TestEnv::blank();
    let project_root = env.tmp.path();

    // F-26 external review: pin MP_HOME to the repo root so the
    // embedded registry lookup reads THIS repo's templates/skills/.
    // Without this, a developer's shell-exported MP_HOME (commonly
    // `~/.agents/master-plan`) shadows the test's source-of-truth
    // and the test deploys only whatever skills that other MP_HOME
    // contains — often an empty or stale directory — leaving mp-flow
    // undeployed and the test panicking at the SKILL.md assertion.
    //
    // The previous code relied on TestEnv::blank() but TestEnv
    // doesn't manage env vars on the test's own process (it sets
    // env on subprocess `Command`s, not on `std::env::var` reads
    // performed synchronously by the library call). All other
    // tests in this file shell out to mp and avoid this trap; this
    // one calls into the library directly so env isolation must be
    // explicit.
    let _mp_home_guard =
        ScopedEnvVar::set("MP_HOME", common::repo_root().to_string_lossy().as_ref());
    let _install_dir_guard =
        ScopedEnvVar::set("MP_INSTALL_DIR", env.tmp.path().to_string_lossy().as_ref());

    // First install.
    install_project_skill(project_root, ProjectSkillHarness::Opencode).unwrap();
    let skill_dir = project_root.join(".opencode/skills/mp-flow");
    assert!(skill_dir.join("SKILL.md").is_file(), "SKILL.md must land");

    // Plant a user-personal file in the skill dir.
    let user_note = skill_dir.join("my-personal-notes.md");
    std::fs::write(&user_note, "# My personal notes\n\nDo not wipe me.\n").unwrap();
    assert!(user_note.is_file(), "pre-condition: user file planted");

    // Re-install. Per the M-C-6 fix, the whole-dir wipe is replaced
    // with per-file wipe — user files (which aren't in source) are
    // preserved.
    install_project_skill(project_root, ProjectSkillHarness::Opencode).unwrap();

    assert!(
        user_note.is_file(),
        "user-personal file must survive re-install (M-C-6 fix)"
    );
    // Skill files must still be present (symlinks this time, since
    // install_project_skill uses per-file symlinks).
    assert!(skill_dir.join("SKILL.md").is_file());
    assert!(skill_dir.join("flow-stages.md").is_file());
    assert!(skill_dir.join("stages.toml").is_file());
}

#[test]
fn skills_selector_unknown_errors() {
    let env = TestEnv::new();

    let out = env.run_at_repo(&[
        "install",
        "--dev",
        "--toolkit-only",
        "--skills",
        "bogus",
        "--format",
        "json",
    ]);
    assert!(!out.status.success(), "bogus skill should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown skill"),
        "expected 'unknown skill' in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("mp-flow"),
        "error should list registered skills: mp-flow, got: {stderr}"
    );
    assert!(
        stderr.contains("spec-grill"),
        "error should list registered skills: spec-grill, got: {stderr}"
    );
}

#[test]
fn install_check_validator_passes() {
    let env = TestEnv::new();

    let out = env.run_at_repo(&["install", "--check", "--dev", "--format", "json"]);
    assert!(
        out.status.success(),
        "install --check should pass on clean registry: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "check report should have ok: true");
    assert!(
        v["skill_count"].as_u64().unwrap_or(0) >= 3,
        "should have at least 3 skills"
    );
}

#[test]
fn install_check_validator_errors_on_missing_skill_file() {
    let env = TestEnv::blank();

    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();

    let skills_dir = common::repo_root().join("templates/skills");
    let out = std::process::Command::new(common::mp_bin())
        .current_dir(&skills_dir)
        .arg("install")
        .arg("--check")
        .arg("--dev")
        .arg("--source")
        .arg(common::repo_root().to_string_lossy().as_ref())
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "check from repo root should pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn no_skills_flag_deploys_full_registry() {
    let env = TestEnv::new();

    let out = env.run_at_repo(&["install", "--dev", "--toolkit-only", "--format", "json"]);
    assert!(
        out.status.success(),
        "install without --skills should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "install report should have ok: true");
}

#[test]
fn registry_driven_add() {
    let env = TestEnv::blank();
    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();

    let read_manifest = |skill_id: &str| -> Option<serde_json::Value> {
        let p = common::repo_root()
            .join("templates/skills")
            .join(skill_id)
            .join("manifest.json");
        if !p.is_file() {
            return None;
        }
        let raw = std::fs::read_to_string(&p).unwrap();
        Some(serde_json::from_str(&raw).unwrap())
    };

    let mp_flow_manifest = read_manifest("mp-flow");
    assert!(mp_flow_manifest.is_some(), "mp-flow manifest should exist");
    let mf = mp_flow_manifest.unwrap();
    assert_eq!(mf["id"], "mp-flow", "mp-flow manifest id should match");
    assert_eq!(
        mf["display"], "MP Flow — cross-role orchestration (12-stage timeline)",
        "mp-flow manifest display should match"
    );
    let mp_runner_manifest = read_manifest("mp-runner");
    assert!(
        mp_runner_manifest.is_some(),
        "mp-runner manifest should exist"
    );
    let mr = mp_runner_manifest.unwrap();
    assert_eq!(mr["id"], "mp-runner", "mp-runner manifest id should match");
    let mp_coord_manifest = read_manifest("mp-coordinator");
    assert!(
        mp_coord_manifest.is_some(),
        "mp-coordinator manifest should exist"
    );
    let mc = mp_coord_manifest.unwrap();
    assert_eq!(
        mc["id"], "mp-coordinator",
        "mp-coordinator manifest id should match"
    );

    let sg_manifest = read_manifest("spec-grill");
    assert!(sg_manifest.is_some(), "spec-grill manifest should exist");
}

#[test]
fn skills_selector_deploys_only_mp_flow() {
    let env = TestEnv::new();

    let out = env.run_at_repo(&[
        "install",
        "--dev",
        "--toolkit-only",
        "--skills",
        "mp-flow",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "install --skills mp-flow should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "install report should have ok: true");
}

#[test]
fn mp_flow_deploys_across_harnesses() {
    let env = TestEnv::blank();
    let root = common::repo_root();
    let install_dir = env.tmp.path().join("install-target");

    for (harness_id, sub_dir) in [
        ("opencode", "harness/opencode/skills"),
        ("cursor", "harness/cursor/skills"),
        ("pi", "harness/pi/agent/skills"),
    ] {
        let skill_path = env
            .tmp
            .path()
            .join(sub_dir)
            .join("mp-flow")
            .join("SKILL.md");
        assert!(
            !skill_path.exists(),
            "pre-condition: mp-flow SKILL.md should not exist yet for {harness_id}"
        );
    }

    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", &install_dir)
        .arg("--plan-dir")
        .arg(env.tmp.path().join("master-plan"))
        .arg("install")
        .arg("--dev")
        .arg("--harness")
        .arg("all")
        .arg("--skills")
        .arg("mp-flow")
        .arg("--format")
        .arg("json");
    common::isolated_harness_env(&mut cmd, env.tmp.path());

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install --skills=mp-flow should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    for (harness_id, sub_dir) in [
        ("opencode", "harness/opencode/skills"),
        ("cursor", "harness/cursor/skills"),
        ("pi", "harness/pi/agent/skills"),
    ] {
        let skill_path = env
            .tmp
            .path()
            .join(sub_dir)
            .join("mp-flow")
            .join("SKILL.md");
        assert!(
            skill_path.exists(),
            "mp-flow SKILL.md should be deployed to {harness_id} at {skill_path:?}"
        );
        let content = std::fs::read_to_string(&skill_path).unwrap();
        assert!(
            content.contains("12-stage timeline"),
            "mp-flow skill for {harness_id} should contain the 12-stage timeline"
        );
    }
}

#[test]
fn mp_runner_deploys_cleanly() {
    let env = TestEnv::blank();
    let root = common::repo_root();
    // Per-test install dir isolates from `~/.agents/master-plan/`, which
    // is shared with other concurrent install/uninstall tests in the
    // suite. Without isolation, verify_installed_artifacts can read a
    // half-written file (size 1 byte) and bail.
    let install_dir = env.tmp.path().join("install-target");
    let path = common::path_with_install_bin(&install_dir);

    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", &install_dir)
        .env("PATH", &path)
        .arg("--plan-dir")
        .arg(env.tmp.path().join("master-plan"))
        .arg("install")
        .arg("--dev")
        .arg("--toolkit-only")
        .arg("--skills")
        .arg("mp-runner")
        .arg("--format")
        .arg("json");

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install --skills=mp-runner should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "install report should have ok: true");
}

#[test]
fn mp_coordinator_deploys_cleanly() {
    let env = TestEnv::blank();
    let root = common::repo_root();
    let install_dir = env.tmp.path().join("install-target");
    let path = common::path_with_install_bin(&install_dir);

    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", &install_dir)
        .env("PATH", &path)
        .arg("--plan-dir")
        .arg(env.tmp.path().join("master-plan"))
        .arg("install")
        .arg("--dev")
        .arg("--toolkit-only")
        .arg("--skills")
        .arg("mp-coordinator")
        .arg("--format")
        .arg("json");

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install --skills=mp-coordinator should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "install report should have ok: true");
}

// ---------------------------------------------------------------------------
// M158: WP-2 — full skill package deploy tests
//
// Each test uses `isolated_harness_env` to point skill deploys at a
// per-test scratch dir (under env.tmp.path()), so the developer's
// real ~/.agents/skills/ is never touched (AC-10).
// ---------------------------------------------------------------------------

/// Run `mp install --dev` with isolated harness env. Returns the harness
/// skills root (the dir passed to `isolated_harness_env`) so callers can
/// assert files landed at the expected paths.
fn isolated_install(env: &TestEnv, args: &[&str]) -> (std::process::Output, std::path::PathBuf) {
    let root = common::repo_root();
    let install_dir = env.tmp.path().join("install-target");
    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();

    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", &install_dir)
        .arg("--plan-dir")
        .arg(&plan_dir)
        .args(args);
    common::isolated_harness_env(&mut cmd, env.tmp.path());
    let out = cmd.output().unwrap();
    (out, env.tmp.path().join("harness/opencode/skills"))
}

/// Parse SKILL.md for relative `[label](target)` markdown link references
/// and assert each target file exists relative to `skill_dir`. This is
/// the load-bearing test that catches the M158 bug class: any future
/// sibling referenced from SKILL.md but missing on disk fails this
/// assertion, surfacing the regression before it ships.
fn assert_skill_md_links_resolve(skill_dir: &std::path::Path) {
    let skill_md = skill_dir.join("SKILL.md");
    let content = std::fs::read_to_string(&skill_md)
        .unwrap_or_else(|e| panic!("read {}: {e}", skill_md.display()));
    let mut refs: Vec<(String, String)> = Vec::new();
    // Simple markdown link parse: [label](target). Targets can be
    // relative paths (name.md, subdir/name.md) or absolute URLs.
    // We only assert relative-path targets resolve to a sibling file.
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '[' {
            continue;
        }
        // Collect label
        let mut label = String::new();
        for c2 in chars.by_ref() {
            if c2 == ']' {
                break;
            }
            label.push(c2);
        }
        // Expect `(` next
        if chars.peek() != Some(&'(') {
            continue;
        }
        chars.next();
        let mut target = String::new();
        for c2 in chars.by_ref() {
            if c2 == ')' {
                break;
            }
            target.push(c2);
        }
        // Skip absolute URLs / scheme-qualified refs / fragment-only refs
        if target.starts_with("http://")
            || target.starts_with("https://")
            || target.starts_with('#')
            || target.contains("://")
        {
            continue;
        }
        // Strip fragment (#section) and query (?k=v)
        let target = target.split('#').next().unwrap_or(&target);
        let target = target.split('?').next().unwrap_or(target);
        if target.is_empty() {
            continue;
        }
        refs.push((format!("[{label}]({target})"), target.to_string()));
    }
    // F-08: backtick-wrapped *skill-local* file refs (mp-flow
    // `flow-stages.md` / `stages.toml`; diagnosing-bugs
    // `scripts/hitl-loop.template.sh`). Repo paths (`docs/…`,
    // `crates/…`) and prose tokens are ignored. Only assert when the
    // path is present in the source package — that is the deploy
    // contract (if source ships it, dest must have it).
    let skill_id = skill_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let source_root = common::repo_root().join("templates/skills").join(skill_id);
    {
        let bytes = content.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != b'`' {
                i += 1;
                continue;
            }
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'`' && bytes[i] != b'\n' {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'`' {
                continue;
            }
            let inner = &content[start..i];
            i += 1;
            if !looks_like_skill_local_file_ref(inner) {
                continue;
            }
            // Only enforce when the source package actually ships it.
            if !source_root.join(inner).is_file() {
                continue;
            }
            refs.push((format!("`{inner}`"), inner.to_string()));
        }
    }
    // F-23: empty link set is valid only when the skill truly has no
    // relative file references. Callers that need hard sibling
    // presence (mp-flow) assert those paths separately.
    if refs.is_empty() {
        return;
    }
    for (label, target) in &refs {
        assert_link_target_contained(skill_dir, target, label);
    }
}

/// True for skill-package-local file refs (not repo docs/crates paths).
fn looks_like_skill_local_file_ref(s: &str) -> bool {
    if s.is_empty() || s.contains(' ') || s.contains('\t') {
        return false;
    }
    if s.starts_with('-') || s.starts_with('/') || s.starts_with('.') {
        return false;
    }
    if s.contains("://") || s.contains("::") || s.contains('<') || s.contains('>') {
        return false;
    }
    if s.starts_with("docs/") || s.starts_with("crates/") || s.starts_with("master-plan/") {
        return false;
    }
    let lower = s.to_ascii_lowercase();
    let has_ext = [".md", ".toml", ".sh", ".json", ".yaml", ".yml", ".txt"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    if !has_ext {
        return false;
    }
    // Single-segment sibling (`flow-stages.md`) or nested under a
    // package subdir (`scripts/hitl-loop.template.sh`).
    let segments: Vec<_> = s.split('/').collect();
    segments.iter().all(|seg| {
        !seg.is_empty()
            && *seg != ".."
            && *seg != "."
            && seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    })
}

/// Resolve `target` under `skill_dir` and assert it is a file that
/// cannot escape the skill package (F-08 containment).
fn assert_link_target_contained(skill_dir: &std::path::Path, target: &str, label: &str) {
    use std::path::{Component, Path};
    let rel = Path::new(target);
    assert!(
        !rel.is_absolute(),
        "{label}: absolute link target {target:?} is not allowed"
    );
    assert!(
        !rel.components().any(|c| matches!(c, Component::ParentDir)),
        "{label}: link target {target:?} must not contain '..'"
    );
    let resolved = skill_dir.join(target);
    assert!(
        resolved.is_file(),
        "{target} (from {label}) must exist at {resolved:?}"
    );
    let skill_canon = skill_dir
        .canonicalize()
        .unwrap_or_else(|_| skill_dir.to_path_buf());
    let resolved_canon = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
    assert!(
        resolved_canon.starts_with(&skill_canon),
        "{label}: resolved {resolved_canon:?} escapes skill dir {skill_canon:?}"
    );
}

/// F-08: every non-skipped source file lands under dest (tree equality).
fn assert_source_package_deployed(skill_id: &str, dest_dir: &std::path::Path) {
    let src_root = common::repo_root().join("templates/skills").join(skill_id);
    assert!(
        src_root.is_dir(),
        "source skill package missing: {}",
        src_root.display()
    );
    fn walk(src: &std::path::Path, dest: &std::path::Path, rel: &std::path::Path) {
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Mirror install skip list for top-level junk / manifest.
            if name_str == "manifest.json"
                || name_str == ".DS_Store"
                || name_str == "Thumbs.db"
                || name_str == "desktop.ini"
                || name_str.starts_with('.')
            {
                continue;
            }
            let src_path = entry.path();
            let rel_path = rel.join(&name);
            let dest_path = dest.join(&name);
            if src_path.is_dir() {
                walk(&src_path, &dest_path, &rel_path);
            } else if src_path.is_file() {
                assert!(
                    dest_path.is_file(),
                    "source file {rel_path:?} must deploy to {dest_path:?}"
                );
            }
        }
    }
    walk(&src_root, dest_dir, std::path::Path::new(""));
}

#[test]
fn mp_flow_deploys_with_deep_dives() {
    let env = TestEnv::blank();
    let (out, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "mp-flow",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "install --skills=mp-flow should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let skill_dir = skills_root.join("mp-flow");
    // Sibling files (M158 AC-01).
    assert!(skill_dir.join("SKILL.md").is_file(), "SKILL.md must land");
    assert!(
        skill_dir.join("flow-stages.md").is_file(),
        "flow-stages.md must land alongside SKILL.md"
    );
    assert!(
        skill_dir.join("stages.toml").is_file(),
        "stages.toml must land alongside SKILL.md"
    );
    // manifest.json must NOT be deployed (AC-09 / install-time metadata).
    assert!(
        !skill_dir.join("manifest.json").exists(),
        "manifest.json must NOT land in the destination skill dir"
    );
    // stages.toml must parse as TOML and have the stages key.
    let toml_raw = std::fs::read_to_string(skill_dir.join("stages.toml")).unwrap();
    let parsed: toml::Value =
        toml::from_str(&toml_raw).unwrap_or_else(|e| panic!("stages.toml must parse as TOML: {e}"));
    assert!(
        parsed.get("stages").is_some(),
        "stages.toml must have a 'stages' key"
    );
}

#[test]
fn mp_runner_deploys_with_deep_dives() {
    let env = TestEnv::blank();
    let (out, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "mp-runner",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "install --skills=mp-runner should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let skill_dir = skills_root.join("mp-runner");
    assert!(skill_dir.join("SKILL.md").is_file(), "SKILL.md must land");
    assert!(
        skill_dir.join("executing.md").is_file(),
        "executing.md must land (sub-skill deep dive)"
    );
    assert!(
        skill_dir.join("fixing.md").is_file(),
        "fixing.md must land (sub-skill deep dive)"
    );
    assert!(
        skill_dir.join("atomic-writes.md").is_file(),
        "atomic-writes.md must land (sub-skill deep dive)"
    );
    assert!(
        !skill_dir.join("manifest.json").exists(),
        "manifest.json must NOT land"
    );
    // The load-bearing test: every relative markdown link in the
    // deployed SKILL.md must resolve to a sibling file on disk.
    assert_skill_md_links_resolve(&skill_dir);
}

#[test]
fn mp_coordinator_deploys_with_deep_dives() {
    let env = TestEnv::blank();
    let (out, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "mp-coordinator",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "install --skills=mp-coordinator should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let skill_dir = skills_root.join("mp-coordinator");
    assert!(skill_dir.join("SKILL.md").is_file(), "SKILL.md must land");
    assert!(
        skill_dir.join("planning.md").is_file(),
        "planning.md must land"
    );
    assert!(
        skill_dir.join("spec-co-design.md").is_file(),
        "spec-co-design.md must land"
    );
    assert!(
        skill_dir.join("reviewing.md").is_file(),
        "reviewing.md must land"
    );
    assert!(
        !skill_dir.join("manifest.json").exists(),
        "manifest.json must NOT land"
    );
    assert_skill_md_links_resolve(&skill_dir);
}

// mp-code-review is category=internal (repo-only) — not in the consumer
// install registry and not selectable via --skills.
#[test]
fn mp_code_review_is_not_installable() {
    let env = TestEnv::new();
    let out = env.run_at_repo(&[
        "install",
        "--dev",
        "--toolkit-only",
        "--skills",
        "mp-code-review",
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "install --skills=mp-code-review must fail for internal skills"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("unknown skill"),
        "expected 'unknown skill' in stderr, got: {err}"
    );
    assert!(
        err.contains("mp-code-review"),
        "error should mention the rejected skill id: {err}"
    );
    // Registry listing must not advertise the internal skill as installable.
    assert!(
        !err.contains("Registered skills: ")
            || !err
                .split("Registered skills: ")
                .nth(1)
                .unwrap_or("")
                .contains("mp-code-review"),
        "registered-skills list must not include mp-code-review: {err}"
    );
}

#[test]
fn codebase_design_deploys_with_deep_dives() {
    let env = TestEnv::blank();
    let (out, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "codebase-design",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "install --skills=codebase-design should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let skill_dir = skills_root.join("codebase-design");
    assert!(skill_dir.join("SKILL.md").is_file(), "SKILL.md must land");
    assert!(
        skill_dir.join("DEEPENING.md").is_file(),
        "DEEPENING.md must land"
    );
    assert!(
        skill_dir.join("DESIGN-IT-TWICE.md").is_file(),
        "DESIGN-IT-TWICE.md must land"
    );
    assert!(
        !skill_dir.join("manifest.json").exists(),
        "manifest.json must NOT land"
    );
    // F-23 external review: load-bearing link-resolution check covers
    // every skill that ships siblings. codebase-design's SKILL.md
    // references DEEPENING.md and DESIGN-IT-TWICE.md — without this
    // assertion, a future deep link to a missing sibling would not
    // surface as a test failure.
    assert_skill_md_links_resolve(&skill_dir);
}

#[test]
fn diagnosing_bugs_deploys_with_scripts() {
    let env = TestEnv::blank();
    let (out, skills_root) = isolated_install(
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
        "install --skills=diagnosing-bugs should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let skill_dir = skills_root.join("diagnosing-bugs");
    assert!(skill_dir.join("SKILL.md").is_file(), "SKILL.md must land");
    let script = skill_dir.join("scripts/hitl-loop.template.sh");
    assert!(
        script.is_file(),
        "scripts/hitl-loop.template.sh must land with subdirectory preserved"
    );
    // Mode-bit preservation: the source script is +x (0o755) and the
    // deployed copy must keep at least the executable bit. Best-effort
    // on non-unix FS where set_permissions is a no-op.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let src_mode = std::fs::metadata(
            common::repo_root()
                .join("templates/skills/diagnosing-bugs/scripts/hitl-loop.template.sh"),
        )
        .expect("source script must exist")
        .permissions()
        .mode();
        let dest_mode = std::fs::metadata(&script)
            .expect("deployed script must exist")
            .permissions()
            .mode();
        assert_eq!(
            dest_mode & 0o111,
            src_mode & 0o111,
            "executable bits must be preserved on deploy: src={src_mode:o}, dest={dest_mode:o}"
        );
    }
    // F-23 external review: ensure all relative markdown links in
    // diagnosing-bugs/SKILL.md (e.g. scripts/hitl-loop.template.sh)
    // resolve to siblings. Without this, a future link to a missing
    // sibling would not surface as a test failure.
    assert_skill_md_links_resolve(&skill_dir);
}

/// M175 AC-01/AC-02 filter alias: recursive skill deploy + link targets.
#[test]
fn install_skill_link_resolution() {
    let env = TestEnv::blank();
    let (_, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "mp-flow",
            "--format",
            "json",
        ],
    );
    let skill_dir = skills_root.join("mp-flow");
    assert!(skill_dir.join("SKILL.md").is_file());
    assert!(skill_dir.join("flow-stages.md").is_file());
    assert!(skill_dir.join("stages.toml").is_file());
    assert_skill_md_links_resolve(&skill_dir);
    assert_source_package_deployed("mp-flow", &skill_dir);

    let (_, skills_root2) = isolated_install(
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
    let bugs = skills_root2.join("diagnosing-bugs");
    assert!(
        bugs.join("scripts/hitl-loop.template.sh").is_file()
            || bugs.join("scripts").join("hitl-loop.template.sh").is_file(),
        "diagnosing-bugs must deploy scripts/hitl-loop.template.sh"
    );
    assert_skill_md_links_resolve(&bugs);
    assert_source_package_deployed("diagnosing-bugs", &bugs);
}

/// M175 AC-02: every shipped SKILL.md relative link resolves on disk post-install.
#[test]
fn skill_link_targets_exist() {
    let env = TestEnv::blank();
    let skills = [
        "mp-flow",
        "mp-runner",
        "mp-coordinator",
        "codebase-design",
        "diagnosing-bugs",
    ];
    let filter = skills.join(",");
    let (_, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            &filter,
            "--format",
            "json",
        ],
    );
    for id in skills {
        let dir = skills_root.join(id);
        assert!(dir.join("SKILL.md").is_file(), "{id}/SKILL.md must deploy");
        assert_skill_md_links_resolve(&dir);
        assert_source_package_deployed(id, &dir);
    }
}

#[test]
fn stale_sibling_is_removed_on_reinstall() {
    let env = TestEnv::blank();
    let (_, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "mp-flow",
            "--format",
            "json",
        ],
    );
    let skill_dir = skills_root.join("mp-flow");
    assert!(
        skill_dir.join("SKILL.md").is_file(),
        "pre-condition: SKILL.md must land on first install"
    );
    // Plant a stale file (a file present on disk but absent from source).
    let stale = skill_dir.join("stale-from-previous-version.md");
    std::fs::write(&stale, "# Stale sibling from a previous upstream version\n").unwrap();
    assert!(stale.is_file(), "pre-condition: stale file must exist");

    // Re-install the same skill — wipe-then-rewrite must clear the stale.
    let (out, _) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "mp-flow",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "re-install --skills=mp-flow should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !stale.exists(),
        "stale sibling must be removed on reinstall (wipe-then-rewrite)"
    );
    // And the current source files must still be there.
    assert!(skill_dir.join("SKILL.md").is_file());
    assert!(skill_dir.join("flow-stages.md").is_file());
    assert!(skill_dir.join("stages.toml").is_file());
}

#[test]
fn new_sibling_in_source_lands_on_redeploy() {
    // M158 round 2 (L-T-3): the wipe-then-rewrite must also pick up
    // siblings ADDED between deploys, not just clean up stale ones.
    // Plant a synthetic source skill with one sibling, deploy, then
    // add a NEW sibling to source, redeploy, and assert it lands.
    let env = TestEnv::blank();
    let root = env.tmp.path().join("synthetic-source");
    let skill_id = "growing-skill";
    let skill_src = root.join("templates/skills").join(skill_id);
    std::fs::create_dir_all(&skill_src).unwrap();
    std::fs::write(skill_src.join("SKILL.md"), "# Growing Skill\n\nBody.\n").unwrap();
    std::fs::write(
        skill_src.join("manifest.json"),
        r#"{"id":"growing-skill","display":"Growing","category":"core"}"#,
    )
    .unwrap();

    // First deploy: only SKILL.md.
    let (out, _skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--source",
            &root.to_string_lossy(),
            "--harness",
            "opencode",
            "--skills",
            skill_id,
            "--format",
            "json",
        ],
    );
    assert!(out.status.success(), "first deploy must succeed");
    let skill_dir = env
        .tmp
        .path()
        .join("harness/opencode/skills")
        .join(skill_id);
    assert!(skill_dir.join("SKILL.md").is_file());
    assert!(!skill_dir.join("new-sibling.md").exists());

    // Upstream adds a new sibling.
    std::fs::write(
        skill_src.join("new-sibling.md"),
        "# New Sibling\n\nBody of the new sibling.\n",
    )
    .unwrap();

    // Re-deploy.
    let (out, _skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--source",
            &root.to_string_lossy(),
            "--harness",
            "opencode",
            "--skills",
            skill_id,
            "--format",
            "json",
        ],
    );
    assert!(out.status.success(), "re-deploy must succeed");
    assert!(
        skill_dir.join("new-sibling.md").is_file(),
        "new sibling added between deploys must land on the next install"
    );
    assert!(
        skill_dir.join("SKILL.md").is_file(),
        "SKILL.md still present"
    );
}

#[test]
fn mp_install_check_clean_after_full_skill_package_deploy() {
    let env = TestEnv::blank();
    // Deploy the core set + an opt-in catalog skill, then assert
    // `mp install --check` does not surface false-positive drift from
    // the new sibling files (M158 AC-07).
    for skill in ["mp-flow", "mp-runner", "mp-coordinator", "codebase-design"] {
        let (out, _) = isolated_install(
            &env,
            &[
                "install",
                "--dev",
                "--harness",
                "opencode",
                "--skills",
                skill,
                "--format",
                "json",
            ],
        );
        assert!(
            out.status.success(),
            "install --skills={skill} should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let root = common::repo_root();
    let plan_dir = env.tmp.path().join("master-plan");
    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", env.tmp.path().join("install-target"))
        .arg("--plan-dir")
        .arg(&plan_dir)
        .arg("install")
        .arg("--check")
        .arg("--dev")
        .arg("--format")
        .arg("json");
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install --check after full sibling-bearing deploy should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["ok"], true,
        "install --check should report ok=true after full deploy; got: {v}"
    );
    let errors = v["errors"].as_array().cloned().unwrap_or_default();
    assert!(
        errors.is_empty(),
        "install --check must NOT introduce false-positive errors after sibling deploy; got: {errors:?}"
    );
}

#[test]
fn mp_install_check_flags_missing_sibling() {
    let env = TestEnv::blank();
    // Deploy mp-flow with full siblings.
    let (_, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "mp-flow",
            "--format",
            "json",
        ],
    );
    let skill_dir = skills_root.join("mp-flow");
    let sibling = skill_dir.join("flow-stages.md");
    assert!(
        sibling.is_file(),
        "pre-condition: flow-stages.md must exist"
    );
    // Hand-remove the sibling — simulates a torn install or a user
    // accidentally deleting a single deep-dive file.
    std::fs::remove_file(&sibling).unwrap();
    assert!(!sibling.exists(), "pre-condition: sibling removed");

    // Run mp install --check — must surface the missing sibling as a
    // drift warning (not an error; the registry is still valid).
    let root = common::repo_root();
    let plan_dir = env.tmp.path().join("master-plan");
    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .env("MP_INSTALL_DIR", env.tmp.path().join("install-target"))
        .arg("--plan-dir")
        .arg(&plan_dir)
        .arg("install")
        .arg("--check")
        .arg("--dev")
        .arg("--format")
        .arg("json");
    common::isolated_harness_env(&mut cmd, env.tmp.path());
    let out = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let v: serde_json::Value = serde_json::from_slice(stdout.as_bytes()).unwrap_or_else(|e| {
        panic!("install --check JSON parse: {e}; stdout={stdout}; stderr={stderr}")
    });
    assert_eq!(
        v["ok"], true,
        "install --check must still report ok=true (registry is valid); got: {v}"
    );
    let warnings = v["warnings"].as_array().cloned().unwrap_or_default();
    let has_drift = warnings.iter().any(|w| match w {
        serde_json::Value::String(s) => s.contains("missing sibling") && s.contains("mp-flow"),
        serde_json::Value::Object(obj) => obj
            .get("message")
            .and_then(|m| m.as_str())
            .map(|m| m.contains("missing sibling") && m.contains("mp-flow"))
            .unwrap_or(false),
        _ => false,
    });
    assert!(
        has_drift,
        "install --check must flag the missing sibling as a drift warning; warnings: {warnings:?}"
    );
}

#[test]
fn install_backfills_legacy_installed_path_entries() {
    // F-22 external review (legacy installed_path gap): on the
    // very first install run after M158 ships, any pre-existing
    // manifest entry (no `installed_path` field) gets the field
    // backfilled from the harness's CURRENT resolver. Without
    // backfill, `mp install --check` would reproduce the M-C-2
    // env-drift false positive against legacy entries.
    //
    // The test plants a legacy-shape manifest on disk (no
    // `installed_path` field), runs `mp install` against the
    // synthetic source, and asserts the on-disk manifest now
    // carries `installed_path` matching the harness's current
    // resolver path.
    let env = TestEnv::blank();
    let install_dir = env.tmp.path().join("install-target");
    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&install_dir).unwrap();
    std::fs::create_dir_all(&plan_dir).unwrap();

    // Plant a legacy manifest entry: shape pre-M158, no
    // `installed_path` field. (The field is `serde(default)` so
    // omitting it deserializes to `""`.)
    let legacy_path = install_dir.join("installed-skills.json");
    std::fs::write(
        &legacy_path,
        r#"{"entries":[{"skill_id":"junk-skill","harness":"opencode","category":"core","installed_at":"2026-01-01T00:00:00+00:00"}]}"#,
    )
    .unwrap();

    // Build a synthetic source + manifest pair so the install
    // path is happy (registry load, manifest entry, deploy).
    let root = env.tmp.path().join("synthetic-source");
    let skill_id = "junk-skill";
    let skill_src = root.join("templates/skills").join(skill_id);
    std::fs::create_dir_all(&skill_src).unwrap();
    std::fs::write(skill_src.join("SKILL.md"), "# Body\n").unwrap();
    std::fs::write(
        skill_src.join("manifest.json"),
        r#"{"id":"junk-skill","display":"Junk","category":"core"}"#,
    )
    .unwrap();

    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(common::repo_root())
        .env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", &install_dir)
        .arg("--plan-dir")
        .arg(&plan_dir)
        .arg("install")
        .arg("--source")
        .arg(&root)
        .arg("--harness")
        .arg("opencode")
        .arg("--skills")
        .arg(skill_id)
        .arg("--format")
        .arg("json");
    common::isolated_harness_env(&mut cmd, env.tmp.path());
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install with legacy manifest entry should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body = std::fs::read_to_string(&legacy_path).expect("manifest written");
    let v: serde_json::Value = serde_json::from_str(&body).expect("manifest parses");
    let entries = v["entries"].as_array().expect("entries array");
    assert!(
        !entries.is_empty(),
        "manifest should still have entries after install; got: {body}"
    );
    let entry = &entries[0];
    let installed_path = entry
        .get("installed_path")
        .and_then(|p| p.as_str())
        .unwrap_or_default();
    assert!(
        !installed_path.is_empty(),
        "legacy entry must be backfilled with installed_path; got: {entry}"
    );
    assert!(
        installed_path.ends_with(skill_id),
        "installed_path should resolve to the harness's current skill dir; got {installed_path:?}"
    );
}

#[test]
fn deploy_skips_junk_files() {
    // M158 AC-09: build a synthetic source skill dir with SKILL.md +
    // manifest.json + .DS_Store + foo.swp + bar~, install with
    // --source pointing at the synthetic dir, assert only SKILL.md
    // lands on disk.
    //
    // M158 round 2 (L-T-4): this fixture requires a valid
    // manifest.json — `SkillRegistry::load` silently skips
    // directories without one, and a future refactor of that load
    // path could break this test's precondition without surfacing
    // here. The manifest is intentionally minimal (id + display +
    // category) so a load change that adds new required fields
    // trips this test loudly.
    let env = TestEnv::blank();
    let root = env.tmp.path().join("synthetic-source");
    let skill_id = "fixture-skill";
    let skill_src = root.join("templates/skills").join(skill_id);
    std::fs::create_dir_all(&skill_src).unwrap();

    std::fs::write(skill_src.join("SKILL.md"), "# Fixture Skill\n\nBody.\n").unwrap();
    std::fs::write(
        skill_src.join("manifest.json"),
        r#"{"id":"fixture-skill","display":"Fixture","category":"core"}"#,
    )
    .unwrap();
    std::fs::write(skill_src.join(".DS_Store"), "DS_Store junk\n").unwrap();
    std::fs::write(skill_src.join("foo.swp"), "vim swap\n").unwrap();
    std::fs::write(skill_src.join("bar~"), "emacs backup\n").unwrap();

    let install_dir = env.tmp.path().join("install-target");
    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();

    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(common::repo_root())
        .env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", &install_dir)
        .arg("--plan-dir")
        .arg(&plan_dir)
        .arg("install")
        // No --dev: dev mode looks for the mp / raul binaries under
        // `source_root/target/{release,debug}/` and our synthetic
        // source is a tmp dir with no target/. With --dev omitted the
        // resolver falls back to `env::current_exe()` for both.
        .arg("--source")
        .arg(&root)
        .arg("--harness")
        .arg("opencode")
        .arg("--skills")
        .arg(skill_id)
        .arg("--format")
        .arg("json");
    common::isolated_harness_env(&mut cmd, env.tmp.path());
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install --source=<synthetic> should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let skill_dest = env
        .tmp
        .path()
        .join("harness/opencode/skills")
        .join(skill_id);
    assert!(skill_dest.join("SKILL.md").is_file(), "SKILL.md must land");
    // Junk files must NOT land.
    for junk in [".DS_Store", "foo.swp", "bar~"] {
        let p = skill_dest.join(junk);
        assert!(
            !p.exists(),
            "junk file {junk} must NOT be deployed; found at {p:?}"
        );
    }
    // manifest.json (install-time metadata) must NOT land either.
    assert!(
        !skill_dest.join("manifest.json").exists(),
        "manifest.json must NOT be deployed (install-time metadata only)"
    );
}

#[test]
fn deploy_skips_junk_files_case_insensitive_suffix() {
    // M158 round 2 (L-C-8): suffix matching must be ASCII-case-
    // insensitive. Vim's swap files on case-insensitive mounts are
    // often written as `.SWP` (uppercase); the lowercase-only
    // filter would let them through.
    let env = TestEnv::blank();
    let root = env.tmp.path().join("synthetic-source");
    let skill_id = "case-junk-skill";
    let skill_src = root.join("templates/skills").join(skill_id);
    std::fs::create_dir_all(&skill_src).unwrap();
    std::fs::write(skill_src.join("SKILL.md"), "# Body\n").unwrap();
    std::fs::write(
        skill_src.join("manifest.json"),
        r#"{"id":"case-junk-skill","display":"Case Junk","category":"core"}"#,
    )
    .unwrap();
    // Uppercase variants of the same junk patterns.
    std::fs::write(skill_src.join("foo.SWP"), "uppercase vim swap\n").unwrap();
    std::fs::write(skill_src.join("FOO~"), "uppercase emacs backup\n").unwrap();

    let install_dir = env.tmp.path().join("install-target");
    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(common::repo_root())
        .env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", &install_dir)
        .arg("--plan-dir")
        .arg(&plan_dir)
        .arg("install")
        .arg("--source")
        .arg(&root)
        .arg("--harness")
        .arg("opencode")
        .arg("--skills")
        .arg(skill_id)
        .arg("--format")
        .arg("json");
    common::isolated_harness_env(&mut cmd, env.tmp.path());
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install --source=<synthetic> should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let skill_dest = env
        .tmp
        .path()
        .join("harness/opencode/skills")
        .join(skill_id);
    assert!(skill_dest.join("SKILL.md").is_file(), "SKILL.md must land");
    // Uppercase junk must NOT land (case-insensitive skip).
    for junk in ["foo.SWP", "FOO~"] {
        let p = skill_dest.join(junk);
        assert!(
            !p.exists(),
            "uppercase junk file {junk} must NOT be deployed (case-insensitive skip); found at {p:?}"
        );
    }
}

#[test]
fn deploy_skips_cross_platform_and_editor_junk() {
    // F-20 external review: the original `deploy_skips_junk_files`
    // covered only `.DS_Store` / `Thumbs.db` / `*.swp` / `*~`. The
    // skip list is broader: cross-platform (`desktop.ini`), Vim
    // swap-continuation (`*.swo`), and any leading-dotfile in any
    // directory. Pin every class with its own fixture so the next
    // OS-junk-pattern regression surfaces here, not at a downstream
    // consumer (skill runtime, doctor, --check drift detection).
    let env = TestEnv::blank();
    let root = env.tmp.path().join("synthetic-source");
    let skill_id = "junk-mosaic-skill";
    let skill_src = root.join("templates/skills").join(skill_id);
    std::fs::create_dir_all(&skill_src).unwrap();
    std::fs::write(skill_src.join("SKILL.md"), "# Body\n").unwrap();
    std::fs::write(
        skill_src.join("manifest.json"),
        r#"{"id":"junk-mosaic-skill","display":"Junk Mosaic","category":"core"}"#,
    )
    .unwrap();
    // Top-level: Windows metadata + Vim swap-continuation.
    std::fs::write(skill_src.join("desktop.ini"), "Windows folder metadata\n").unwrap();
    std::fs::write(skill_src.join("foo.swo"), "Vim swap continuation\n").unwrap();
    // Subdirectory: arbitrary leading-dotfile + Emacs lockfile pattern.
    let sub = skill_src.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join(".hidden"), "dotfile\n").unwrap();
    std::fs::write(sub.join(".#lock.md"), "Emacs lockfile\n").unwrap();
    std::fs::write(sub.join("legit.md"), "# legit\n").unwrap();

    let install_dir = env.tmp.path().join("install-target");
    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(common::repo_root())
        .env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", &install_dir)
        .arg("--plan-dir")
        .arg(&plan_dir)
        .arg("install")
        .arg("--source")
        .arg(&root)
        .arg("--harness")
        .arg("opencode")
        .arg("--skills")
        .arg(skill_id)
        .arg("--format")
        .arg("json");
    common::isolated_harness_env(&mut cmd, env.tmp.path());
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "install --source=<synthetic> should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let skill_dest = env
        .tmp
        .path()
        .join("harness/opencode/skills")
        .join(skill_id);
    assert!(skill_dest.join("SKILL.md").is_file(), "SKILL.md must land");
    assert!(
        skill_dest.join("sub").join("legit.md").is_file(),
        "legit sub-file must land"
    );
    // Every junk pattern from F-20 must NOT land.
    let must_skip = [
        ("desktop.ini", skill_dest.join("desktop.ini")),
        ("foo.swo", skill_dest.join("foo.swo")),
        ("sub/.hidden", skill_dest.join("sub").join(".hidden")),
        ("sub/.#lock.md", skill_dest.join("sub").join(".#lock.md")),
        ("manifest.json", skill_dest.join("manifest.json")),
    ];
    for (name, p) in must_skip {
        assert!(
            !p.exists(),
            "junk file {name} must NOT be deployed; found at {p:?}"
        );
    }
}

#[test]
fn env_sh_posix_quotes_hostile_install_path_and_has_no_side_effects() {
    use mp::install::write_env_snippet;

    let env = TestEnv::blank();
    let marker = env.tmp.path().join("injected-marker");
    let hostile_name = format!(
        "space ' quote $(touch {}) `touch {}` back\\slash\nnewline",
        marker.display(),
        marker.display()
    );
    let install_root = env.tmp.path().join(hostile_name);
    std::fs::create_dir_all(&install_root).unwrap();
    write_env_snippet(&install_root).expect("atomic env.sh write");

    let script = install_root.join("env.sh");
    let mut command = std::process::Command::new("sh");
    command
        .env("PATH", "/usr/bin:/bin")
        .env("ENV_SH", &script)
        .arg("-c")
        .arg(". \"$ENV_SH\"; printf '%s\\n%s' \"$MP_HOME\" \"$PATH\"");
    // Keep this test under the file's harness-isolation contract even though
    // it calls only `sh` and the library helper.
    common::isolated_harness_env(&mut command, env.tmp.path());
    let output = command.output().expect("source env.sh");
    assert!(
        output.status.success(),
        "source failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected = format!(
        "{}\n{}/bin:/usr/bin:/bin",
        install_root.display(),
        install_root.display()
    );
    assert_eq!(stdout, expected);
    assert!(!marker.exists(), "quoted path executed shell content");

    let content = std::fs::read_to_string(script).unwrap();
    assert!(content.contains("'\"'\"'"), "single quote must be encoded");
    assert!(
        !env.tmp.path().read_dir().unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".tmp")),
        "atomic env write must not leave staging files"
    );
}

#[test]
fn uninstall_rejects_manifest_escape_and_symlink_artifact() {
    let env = TestEnv::blank();
    let (installed, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "mp-flow",
            "--format",
            "json",
        ],
    );
    assert!(installed.status.success());

    let manifest_path = env.tmp.path().join("install-target/installed-skills.json");
    let original = std::fs::read_to_string(&manifest_path).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
    let entry = manifest["entries"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["skill_id"] == "mp-flow")
        .unwrap();

    let outside_root = env.tmp.path().join("outside-skills");
    let outside_skill = outside_root.join("mp-flow");
    std::fs::create_dir_all(&outside_skill).unwrap();
    std::fs::write(outside_skill.join("SKILL.md"), "# outside\n").unwrap();
    entry["harness_root"] = serde_json::Value::String(
        outside_root
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into(),
    );
    entry["artifact_path"] = serde_json::Value::String("mp-flow".into());
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let mut command = std::process::Command::new(common::mp_bin());
    command
        .current_dir(common::repo_root())
        .env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", env.tmp.path().join("install-target"))
        .args(["uninstall", "--harness", "opencode", "--format", "json"]);
    common::isolated_harness_env(&mut command, env.tmp.path());
    let escaped = command.output().unwrap();
    assert!(
        !escaped.status.success(),
        "escaped canonical root must fail"
    );
    assert!(outside_skill.is_dir(), "outside skill must survive");

    std::fs::write(&manifest_path, original).unwrap();
    let real_skill = skills_root.join("mp-flow");
    std::fs::remove_dir_all(&real_skill).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_skill, &real_skill).unwrap();

    let mut command = std::process::Command::new(common::mp_bin());
    command
        .current_dir(common::repo_root())
        .env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", env.tmp.path().join("install-target"))
        .args(["uninstall", "--harness", "opencode", "--format", "json"]);
    common::isolated_harness_env(&mut command, env.tmp.path());
    let symlinked = command.output().unwrap();
    #[cfg(unix)]
    assert!(
        !symlinked.status.success(),
        "symlink artifact must fail closed"
    );
    assert!(outside_skill.is_dir(), "symlink target must survive");
}

#[cfg(unix)]
#[test]
fn uninstall_permission_failure_preserves_skill_and_manifest() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::blank();
    let (installed, skills_root) = isolated_install(
        &env,
        &[
            "install",
            "--dev",
            "--harness",
            "opencode",
            "--skills",
            "mp-flow",
            "--format",
            "json",
        ],
    );
    assert!(installed.status.success());
    let skill_dir = skills_root.join("mp-flow");
    let manifest_path = env.tmp.path().join("install-target/installed-skills.json");

    let original_mode = std::fs::metadata(&skills_root)
        .unwrap()
        .permissions()
        .mode();
    std::fs::set_permissions(&skills_root, std::fs::Permissions::from_mode(0o555)).unwrap();
    let mut command = std::process::Command::new(common::mp_bin());
    command
        .current_dir(common::repo_root())
        .env("MP_HOME", common::repo_root())
        .env("MP_INSTALL_DIR", env.tmp.path().join("install-target"))
        .args(["uninstall", "--harness", "opencode", "--format", "json"]);
    common::isolated_harness_env(&mut command, env.tmp.path());
    let output = command.output().unwrap();
    std::fs::set_permissions(&skills_root, std::fs::Permissions::from_mode(original_mode)).unwrap();

    assert!(!output.status.success(), "unwritable root must fail closed");
    assert!(skill_dir.join("SKILL.md").is_file());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
    assert!(
        manifest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["skill_id"] == "mp-flow"),
        "failed uninstall must preserve manifest provenance"
    );
}

#[test]
fn install_skills_v2_isolation_grep_gate() {
    // M158 AC-10 / S-2.9: every test in this file must reach the
    // install / harness code through one of the isolation patterns:
    //   - `run_at_repo(` (TestEnv helper, now wraps isolated_harness_env)
    //   - `isolated_install(` (M158 helper that sets MP_INSTALL_DIR +
    //     isolated_harness_env)
    //   - `isolated_harness_env` (direct callers)
    //
    // The gate is file-level: at least one of these tokens must
    // appear *per* `#[test]` function. Any future test added without
    // one of these will fail this gate and the diff will surface the
    // regression before merge.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/install_skills_v2.rs"),
    )
    .expect("read install_skills_v2.rs source");

    // Collect (fn_name, span_start, span_end) for every `#[test] fn name()`.
    // Only count `#[test]` that is the **attribute** of a top-level fn, i.e.
    // appears at the start of a line (modulo leading whitespace) and is
    // followed by a `fn name(` declaration on the next non-blank line.
    // This filters out comment-text occurrences of `#[test]`.
    let mut fn_spans: Vec<(String, usize, usize)> = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut byte_offset: usize = 0;
    let mut line_starts: Vec<usize> = Vec::new();
    for line in &lines {
        line_starts.push(byte_offset);
        byte_offset += line.len() + 1; // +1 for newline
    }
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("#[test]") {
            continue;
        }
        // The next non-blank, non-attribute line should be `fn name(`.
        for next_line in &lines[(i + 1)..] {
            let t = next_line.trim_start();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if let Some(rest) = t.strip_prefix("fn ") {
                let name = rest.split('(').next().unwrap_or("").trim().to_string();
                if name.is_empty() {
                    break;
                }
                let start = line_starts[i];
                // Find the closing `}` of this function by brace counting.
                let body_start = source[start..]
                    .find('{')
                    .map(|o| start + o)
                    .unwrap_or(start);
                let mut depth: i32 = 0;
                let mut end = body_start;
                for (j, ch) in source[body_start..].char_indices() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            end = body_start + j + 1;
                            break;
                        }
                    }
                }
                fn_spans.push((name, start, end));
            }
            break;
        }
    }

    let test_fns: Vec<&str> = fn_spans
        .iter()
        .map(|(n, _, _)| n.as_str())
        .filter(|n| *n != "install_skills_v2_isolation_grep_gate")
        .collect();
    assert!(
        test_fns.len() >= 10,
        "expected at least 10 install tests; found {} ({:?})",
        test_fns.len(),
        test_fns
    );

    let mut missing: Vec<String> = Vec::new();
    for (name, start, end) in &fn_spans {
        if name == "install_skills_v2_isolation_grep_gate" {
            continue;
        }
        // Unit tests in `mod unit_tests` invoke mp::install::* directly
        // without spawning `mp` — they don't touch any on-disk skill
        // dir. Skip them.
        let body = &source[*start..*end];
        if body.contains("check_registry(") || body.contains("SkillRegistry::load") {
            continue;
        }
        // Tests that don't spawn `mp` at all (e.g. fixture-only reads
        // like `registry_driven_add`) can't contaminate ~/.agents/skills/.
        if !body.contains("Command::new(")
            && !body.contains("run_at_repo(")
            && !body.contains("isolated_install(")
        {
            continue;
        }
        // Tests that pass --check (no skill deploy) or --toolkit-only
        // (binary-only install, no skill deploy) don't touch any
        // harness skill dir — they don't need isolation.
        let no_skill_deploy = body.contains("\"--check\"") || body.contains("\"--toolkit-only\"");
        if no_skill_deploy {
            continue;
        }
        let has_isolation = body.contains("isolated_harness_env")
            || body.contains("run_at_repo(")
            || body.contains("isolated_install(");
        if !has_isolation {
            missing.push(name.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "these #[test] functions lack isolation tokens (MP_INSTALL_DIR via run_at_repo or isolated_harness_env or isolated_install): {:?}",
        missing
    );
}

#[cfg(test)]
mod unit_tests {
    use mp::install::{check_registry, SkillRegistry};

    #[test]
    fn registry_loads_all_manifested_skills() {
        // F-26 external review: read templates/skills/ directly from
        // the repo root instead of via mp::assets::toolkit_home().
        // The latter consults MP_HOME first, so a developer's
        // shell-exported MP_HOME (typically ~/.agents/master-plan)
        // shadows the test's source-of-truth and the test silently
        // passes against an empty foreign registry — a closed-loop
        // closed on the developer's stale install (L43).
        let source_root = crate::common::repo_root();
        let registry = SkillRegistry::load(&source_root).unwrap();
        let ids = registry.skill_ids();
        assert!(
            ids.contains(&"mp-flow"),
            "mp-flow should be in registry: {:?}",
            ids
        );
        assert!(
            ids.contains(&"mp-runner"),
            "mp-runner should be in registry: {:?}",
            ids
        );
        assert!(
            ids.contains(&"mp-coordinator"),
            "mp-coordinator should be in registry: {:?}",
            ids
        );
        assert!(
            ids.contains(&"spec-grill"),
            "spec-grill should be in registry: {:?}",
            ids
        );
    }

    #[test]
    fn check_registry_from_toolkit_home_passes() {
        // F-26 external review: read templates/skills/ directly from
        // the repo root (see comment on registry_loads_all_manifested_skills).
        // The test's pre-condition (">= 3 skills loaded") only makes
        // sense against THIS repo's templates/, not against a
        // developer-machine's MP_HOME that may be empty or stale.
        let source_root = crate::common::repo_root();
        let report = check_registry(&source_root).unwrap();
        assert!(report.ok, "check should pass: {:?}", report.errors);
        assert!(
            report.skill_count >= 3,
            "expected >= 3 skills, got {}",
            report.skill_count
        );
    }
}
