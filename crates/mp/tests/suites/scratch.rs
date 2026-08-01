use crate::common::TestEnv;
use std::fs;

#[test]
fn scratch_path_prints_dir() {
    let env = TestEnv::new();

    let out = env.run(&["scratch", "path", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let path_str = v["path"].as_str().unwrap();
    assert!(path_str.ends_with(".mp-scratch"));
    assert!(fs::metadata(path_str).is_ok());
}

#[test]
fn scratch_new_creates_subdir() {
    let env = TestEnv::new();

    let out = env.run(&["scratch", "new", "test-labels", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let path_str = v["path"].as_str().unwrap();
    let scratch_str = v["scratch_dir"].as_str().unwrap();
    assert!(path_str.starts_with(scratch_str));
    assert!(path_str.contains("test-labels"));
    assert!(fs::metadata(path_str).is_ok());
}

#[test]
fn scratch_path_idempotent() {
    let env = TestEnv::new();

    // Call twice, both should succeed
    let out1 = env.run(&["scratch", "path", "--format", "json"]);
    assert!(out1.status.success());
    let out2 = env.run(&["scratch", "path", "--format", "json"]);
    assert!(out2.status.success());
    let v1: serde_json::Value = serde_json::from_slice(&out1.stdout).unwrap();
    let v2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(v1["path"].as_str().unwrap(), v2["path"].as_str().unwrap());
}

#[test]
fn scratch_with_fields_projection() {
    let env = TestEnv::new();

    let out = env.run(&["scratch", "path", "--fields", "path"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.get("path").is_some());
    assert!(v.get("other").is_none());
}
