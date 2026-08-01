mod common;

use std::fs;

use common::{repo_root, TestEnv};
use walkdir::WalkDir;

#[test]
fn fixture_helper_copies_to_temp_and_preserves_source_snapshot() {
    let env = TestEnv::from_fixture("walkthrough-oauth");
    let source = repo_root().join("tests/fixtures/projects/walkthrough-oauth");

    fs::write(env.tmp.path().join(".mp-write.lock"), b"temp-only").unwrap();
    fs::write(env.tmp.path().join("activity.json"), b"temp-only").unwrap();
    fs::create_dir_all(env.tmp.path().join(".mp-txn/staging")).unwrap();
    fs::write(
        env.tmp.path().join(".mp-txn/staging/recovery-marker"),
        b"temp-only",
    )
    .unwrap();

    assert!(!source.join(".mp-write.lock").exists());
    assert!(!source.join("activity.json").exists());
    assert!(!source.join(".mp-txn").exists());

    drop(env);
}

#[test]
fn mutable_fixture_suites_cannot_bypass_shared_helper() {
    let tests = repo_root().join("crates/mp/tests");
    let mut violations = Vec::new();

    for entry in WalkDir::new(&tests) {
        let entry = entry.unwrap();
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|ext| ext.to_str()) != Some("rs")
        {
            continue;
        }
        let relative = entry.path().strip_prefix(&tests).unwrap();
        if relative == std::path::Path::new("common/mod.rs")
            || relative == std::path::Path::new("fixture_isolation.rs")
            || relative == std::path::Path::new("suites/validate_fixture.rs")
        {
            continue;
        }

        let source = fs::read_to_string(entry.path()).unwrap();
        for (index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("tests/fixtures/projects")
                || line.contains("fn cp_r")
                || line.contains("fn copy_fixture")
            {
                violations.push(format!("{}:{}: {}", relative.display(), index + 1, line));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "fixture suites must use TestEnv::from_fixture:\n{}",
        violations.join("\n")
    );
}

#[test]
fn tracked_project_fixtures_contain_no_generated_write_artifacts() {
    let fixtures = repo_root().join("tests/fixtures/projects");
    let mut artifacts = Vec::new();

    for entry in WalkDir::new(&fixtures) {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy();
        if name == ".mp-write.lock"
            || name == "activity.json"
            || name == ".mp-txn"
            || name.contains("staging")
            || name.contains("recovery")
        {
            artifacts.push(entry.path().display().to_string());
        }
    }

    assert!(
        artifacts.is_empty(),
        "tracked fixtures contain generated write artifacts:\n{}",
        artifacts.join("\n")
    );
}
