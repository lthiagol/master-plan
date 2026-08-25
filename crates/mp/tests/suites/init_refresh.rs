//! M194 AC-04: `mp init --refresh` rewrites the project's
//! `master-plan/AGENTS.md` from the current binary's embedded
//! template. Default: confirmation prompt; `--yes` to skip.
//! Scope is AGENTS.md only (per Q-02: `config.json` /
//! `plan.json` drift is a separate doctor check).

use std::fs;

use crate::common::{mp_bin, repo_root};
use tempfile::TempDir;

fn run_init(args: &[&str]) -> std::process::Output {
    std::process::Command::new(mp_bin())
        .args(args)
        .env("MP_HOME", repo_root())
        .output()
        .expect("mp init")
}

#[test]
fn refresh_rewrites_master_plan_agents_md_when_yes() {
    // F-07 (external review) coverage: this test runs in a
    // non-TTY context (nextest has piped stdin) with `--yes`,
    // covering the matrix cell `{ non-TTY, --yes, expect
    // rewrite }`. The matrix as a whole:
    //
    // | stdin's TTY | --yes | expected outcome          | test                            |
    // |-------------|-------|---------------------------|---------------------------------|
    // | yes         | no    | prompt → user choice      | (interactive, F-06 unit tests)  |
    // | no          | no    | cancel (exit non-zero)    | refresh_in_non_interactive...   |
    // | yes         | yes   | rewrite AGENTS.md         | this test                       |
    // | no          | yes   | rewrite AGENTS.md         | this test (nextest = non-tty)   |
    //
    // The fourth cell collapses into this test because nextest
    // always runs with piped stdin.
    let project = TempDir::new().expect("project");
    // Initial init to set up the plan dir + a stale AGENTS.md
    let initial = run_init(&[
        "init",
        "--project-root",
        project.path().to_str().unwrap(),
        "--profile",
        "full",
        "--format",
        "json",
    ]);
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let agents_path = project.path().join("master-plan").join("AGENTS.md");
    assert!(agents_path.is_file(), "AGENTS.md should be created by init");

    // Stale it with a known marker
    fs::write(&agents_path, "stale content from a prior template version").unwrap();

    // --refresh --yes rewrites from the current binary's template
    let out = run_init(&[
        "init",
        "--project-root",
        project.path().to_str().unwrap(),
        "--format",
        "json",
        "--refresh",
        "--yes",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("init --refresh output should be JSON");
    assert_eq!(
        v.get("refresh").and_then(|r| r.as_str()),
        Some("rewritten"),
        "refresh status must be `rewritten`; got: {stdout}"
    );
    assert_eq!(
        v.get("target")
            .and_then(|t| t.as_str())
            .map(|s| s.ends_with("master-plan/AGENTS.md")),
        Some(true),
        "refresh target must be master-plan/AGENTS.md; got: {stdout}"
    );

    let content = fs::read_to_string(&agents_path).unwrap();
    assert_ne!(
        content, "stale content from a prior template version",
        "--refresh must replace the prior content"
    );
    assert!(
        content.contains("master-plan"),
        "refreshed AGENTS.md should reference master-plan; got: {content:?}"
    );
    assert!(
        content.contains("~/.agents/master-plan/"),
        "refreshed AGENTS.md should use toolkit-absolute doc paths (M194 AC-02); got: {content:?}"
    );
}

#[test]
fn refresh_requires_existing_plan_dir() {
    let project = TempDir::new().expect("project");
    // No `mp init` has been run; master-plan/ does not exist.
    let out = run_init(&[
        "init",
        "--project-root",
        project.path().to_str().unwrap(),
        "--format",
        "json",
        "--refresh",
        "--yes",
    ]);
    assert!(
        !out.status.success(),
        "--refresh on a non-init'd project must fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("mp init --refresh requires an existing plan directory"),
        "error must mention the missing plan dir; got: {stderr}"
    );
}

#[test]
fn refresh_does_not_touch_config_or_plan_json() {
    // Q-02: refresh is intentionally narrow (AGENTS.md only).
    // `config.json` and `plan.json` must be byte-for-byte
    // unchanged across a refresh, while AGENTS.md must be
    // replaced from the current binary's embedded template.
    let project = TempDir::new().expect("project");
    let initial = run_init(&[
        "init",
        "--project-root",
        project.path().to_str().unwrap(),
        "--profile",
        "full",
        "--format",
        "json",
    ]);
    assert!(initial.status.success());
    let plan_dir = project.path().join("master-plan");
    let config_before = fs::read(plan_dir.join("config.json")).unwrap();
    let plan_before = fs::read(plan_dir.join("plan.json")).unwrap();
    // Mutate AGENTS.md with a known marker so a no-op
    // refresh would still be detected (the init content and
    // the refresh content come from the same template, so
    // comparing them directly is not a useful invariant).
    fs::write(plan_dir.join("AGENTS.md"), "STALE_MARKER_DO_NOT_KEEP").unwrap();

    let out = run_init(&[
        "init",
        "--project-root",
        project.path().to_str().unwrap(),
        "--format",
        "json",
        "--refresh",
        "--yes",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_after = fs::read(plan_dir.join("config.json")).unwrap();
    let plan_after = fs::read(plan_dir.join("plan.json")).unwrap();
    let agents_after = fs::read_to_string(plan_dir.join("AGENTS.md")).unwrap();
    assert_eq!(
        config_before, config_after,
        "--refresh must not touch config.json"
    );
    assert_eq!(
        plan_before, plan_after,
        "--refresh must not touch plan.json"
    );
    assert_ne!(
        agents_after, "STALE_MARKER_DO_NOT_KEEP",
        "--refresh must replace the stale AGENTS.md content"
    );
    assert!(
        agents_after.contains("master-plan"),
        "refreshed AGENTS.md should reference master-plan; got: {agents_after:?}"
    );
}

#[test]
fn refresh_in_non_interactive_context_requires_yes() {
    // When stdin is not a TTY (CI, pipe), `--refresh`
    // without `--yes` must refuse rather than hang on a
    // confirmation prompt.
    let project = TempDir::new().expect("project");
    let initial = run_init(&[
        "init",
        "--project-root",
        project.path().to_str().unwrap(),
        "--profile",
        "full",
        "--format",
        "json",
    ]);
    assert!(initial.status.success());

    // Stale-marker the AGENTS.md so a no-op refresh would still
    // be detectable (the cancel path must not touch the file).
    let plan_dir = project.path().join("master-plan");
    fs::write(plan_dir.join("AGENTS.md"), "STALE_MARKER_DO_NOT_KEEP").unwrap();

    let out = std::process::Command::new(mp_bin())
        .args([
            "init",
            "--project-root",
            project.path().to_str().unwrap(),
            "--format",
            "json",
            "--refresh",
        ])
        .env("MP_HOME", repo_root())
        // Pipe stdin so it's not a TTY.
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("mp init --refresh (non-tty)");

    // F-03 (external review): cancellation is a non-zero
    // exit code. Scripts that check `if mp init --refresh;
    // then` must NOT see a false positive — the file was
    // not rewritten. The JSON payload still reports the
    // cancellation reason so callers (or raul) can read it.
    assert!(
        !out.status.success(),
        "non-tty without --yes must exit non-zero (F-03); got: status={:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("init --refresh output should be JSON");
    assert_eq!(
        v.get("ok").and_then(|o| o.as_bool()),
        Some(false),
        "non-tty without --yes must report ok: false; got: {stdout}"
    );
    assert_eq!(
        v.get("refresh").and_then(|r| r.as_str()),
        Some("cancelled"),
        "non-tty without --yes must report `cancelled`; got: {stdout}"
    );
    // The file must remain unchanged.
    let agents_after = fs::read_to_string(plan_dir.join("AGENTS.md")).unwrap();
    assert_eq!(
        agents_after, "STALE_MARKER_DO_NOT_KEEP",
        "non-tty without --yes must not rewrite AGENTS.md"
    );
}
