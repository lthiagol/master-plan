use crate::common::TestEnv;
use std::io::Write;

fn setup_changelog(env: &TestEnv, content: &str) {
    let path = env.tmp.path().join("CHANGELOG.md");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn changelog_show_prints_full_file() {
    let env = TestEnv::new();
    setup_changelog(
        &env,
        "# Changelog\n\n## v1.0.0\n\nFirst release.\n\n## v0.9.0\n\nBeta.\n",
    );

    let out = env.run(&["changelog", "show", "--format", "json"]);
    assert!(
        out.status.success(),
        "changelog show failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let changelog = json["changelog"].as_str().unwrap();
    assert!(
        changelog.contains("v1.0.0"),
        "full changelog should contain v1.0.0"
    );
    assert!(
        changelog.contains("v0.9.0"),
        "full changelog should contain v0.9.0"
    );
}

#[test]
fn changelog_show_version_slices() {
    let env = TestEnv::new();
    setup_changelog(&env, "# Changelog\n\n## v1.0.0\n\nFirst release.\n\n## v1.1.0\n\nSecond release.\n\n## v1.2.0\n\nThird release.\n");

    let out = env.run(&[
        "changelog",
        "show",
        "--version",
        "1.1.0",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "changelog show --version failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let changelog = json["changelog"].as_str().unwrap();
    assert!(
        changelog.contains("v1.1.0"),
        "sliced changelog should contain target version"
    );
    assert!(
        changelog.contains("Second release"),
        "sliced changelog should contain its content"
    );
    assert!(
        !changelog.contains("v1.2.0"),
        "sliced changelog should not contain next version"
    );
    assert!(
        !changelog.contains("v1.0.0"),
        "sliced changelog should not contain prior version"
    );
}

#[test]
fn changelog_show_version_with_v_prefix() {
    let env = TestEnv::new();
    setup_changelog(&env, "# Changelog\n\n## v1.0.0\n\nFirst release.\n");

    let out = env.run(&[
        "changelog",
        "show",
        "--version",
        "v1.0.0",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json["changelog"].as_str().unwrap().contains("v1.0.0"));
}
