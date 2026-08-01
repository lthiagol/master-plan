use crate::common::TestEnv;

#[test]
fn changelog_init_scaffolds_file() {
    let env = TestEnv::new();
    let out = env.run(&["changelog", "init", "--format", "json"]);
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let path = env.tmp.path().join("CHANGELOG.md");
    assert!(path.is_file(), "CHANGELOG.md should exist");
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("# Changelog"), "should have title");
    assert!(
        content.contains("## [Unreleased]"),
        "should have Unreleased section"
    );
    assert!(content.contains("### Added"), "should have Added section");
    assert!(content.contains("### Fixed"), "should have Fixed section");
}

#[test]
fn changelog_init_fails_if_exists() {
    let env = TestEnv::new();
    env.run(&["changelog", "init", "--format", "json"]);
    let second = env.run(&["changelog", "init", "--format", "json"]);
    assert!(!second.status.success(), "second init should fail");
}
