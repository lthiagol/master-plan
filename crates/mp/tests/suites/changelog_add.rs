use crate::common::TestEnv;
use std::io::Write;

fn setup_changelog(env: &TestEnv, content: &str) {
    let path = env.tmp.path().join("CHANGELOG.md");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn changelog_add_inserts_entry() {
    let env = TestEnv::new();
    setup_changelog(
        &env,
        "# Changelog\n\n## v1.0.0\n\n### Added\n- First feature\n",
    );

    let out = env.run(&[
        "changelog",
        "add",
        "--version",
        "1.0.0",
        "--section",
        "Added",
        "Second feature",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = std::fs::read_to_string(env.tmp.path().join("CHANGELOG.md")).unwrap();
    assert!(
        content.contains("- Second feature"),
        "entry should be added"
    );
    assert!(
        content.contains("- First feature"),
        "existing entry should remain"
    );
}

#[test]
fn changelog_add_creates_new_version() {
    let env = TestEnv::new();
    setup_changelog(
        &env,
        "# Changelog\n\n## v1.0.0\n\n### Added\n- First feature\n",
    );

    let out = env.run(&[
        "changelog",
        "add",
        "--version",
        "2.0.0",
        "--section",
        "Added",
        "New feature",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "add new version failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = std::fs::read_to_string(env.tmp.path().join("CHANGELOG.md")).unwrap();
    assert!(
        content.contains("## v2.0.0"),
        "new version header should exist"
    );
    assert!(
        content.contains("- New feature"),
        "entry should be in new version"
    );
    // M170 F-08: prior history must survive (F-07 wipe residual).
    assert!(
        content.contains("## v1.0.0"),
        "prior version header must be preserved; got:\n{content}"
    );
    assert!(
        content.contains("- First feature"),
        "prior bullets must be preserved; got:\n{content}"
    );
}

/// M170 F-07 residual: missing version (e.g. `--version unreleased`) must
/// NOT replace the whole CHANGELOG with a lone `## vunreleased` section.
#[test]
fn changelog_add_missing_version_preserves_history() {
    let env = TestEnv::new();
    setup_changelog(
        &env,
        "# Changelog\n\n## v1.0.0\n\n### Added\n- First feature\n\n## v0.9.0\n\n### Fixed\n- Old fix\n",
    );

    let out = env.run(&[
        "changelog",
        "add",
        "--version",
        "unreleased",
        "--section",
        "Fixed",
        "M170 hygiene note",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "add missing version failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = std::fs::read_to_string(env.tmp.path().join("CHANGELOG.md")).unwrap();
    assert!(
        content.contains("## vunreleased") || content.contains("## vUnreleased"),
        "new missing-version header should appear; got:\n{content}"
    );
    assert!(
        content.contains("## v1.0.0") && content.contains("- First feature"),
        "v1.0.0 history must survive; got:\n{content}"
    );
    assert!(
        content.contains("## v0.9.0") && content.contains("- Old fix"),
        "v0.9.0 history must survive; got:\n{content}"
    );
    assert!(
        content.contains("- M170 hygiene note"),
        "new bullet must be present; got:\n{content}"
    );
}

#[test]
fn changelog_add_idempotent() {
    let env = TestEnv::new();
    setup_changelog(
        &env,
        "# Changelog\n\n## v1.0.0\n\n### Added\n- Unique feature\n",
    );

    env.run(&[
        "changelog",
        "add",
        "--version",
        "1.0.0",
        "--section",
        "Added",
        "Unique feature",
        "--format",
        "json",
    ]);

    let content = std::fs::read_to_string(env.tmp.path().join("CHANGELOG.md")).unwrap();
    let count = content.matches("- Unique feature").count();
    assert_eq!(count, 1, "entry should not be duplicated");
}

#[test]
fn changelog_add_creates_section() {
    let env = TestEnv::new();
    setup_changelog(
        &env,
        "# Changelog\n\n## v1.0.0\n\n### Added\n- First feature\n",
    );

    let out = env.run(&[
        "changelog",
        "add",
        "--version",
        "1.0.0",
        "--section",
        "Fixed",
        "Bug fix",
        "--format",
        "json",
    ]);
    assert!(out.status.success());

    let content = std::fs::read_to_string(env.tmp.path().join("CHANGELOG.md")).unwrap();
    assert!(content.contains("### Fixed"), "new section should exist");
    assert!(
        content.contains("- Bug fix"),
        "entry should be in new section"
    );
}
