//! D-011 clause 1: --format json outputs are stable across changes.
//! Golden fixtures committed in tests/fixtures/json-shape/.
//! Regenerate with: `make regen-goldens`

use std::fs;

use crate::common::TestEnv;

fn golden_dir() -> std::path::PathBuf {
    crate::common::repo_root().join("tests/fixtures/json-shape")
}

fn golden_path(name: &str) -> std::path::PathBuf {
    golden_dir().join(name)
}

fn capture_json(env: &TestEnv, args: &[&str]) -> String {
    let out = env.run(args);
    assert!(
        out.status.success(),
        "{} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

fn assert_golden(env: &TestEnv, name: &str, args: &[&str]) {
    let actual = capture_json(env, args);
    let path = golden_path(name);
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("golden file not found: {}", path.display()));
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "JSON output for `{}` differs from golden. Regenerate with `make regen-goldens`",
        args.join(" "),
    );
}

#[test]
fn status_shape_stable() {
    let env = TestEnv::new();
    assert_golden(&env, "status.json", &["status", "--format", "json"]);
}

#[test]
fn path_shape_stable() {
    let env = TestEnv::new();
    assert_golden(&env, "path.json", &["path", "--format", "json"]);
}

#[test]
fn list_milestones_shape_stable() {
    let env = TestEnv::new();
    assert_golden(
        &env,
        "list-milestones.json",
        &["list", "milestones", "--format", "json"],
    );
}

#[test]
fn inbox_shape_stable() {
    let env = TestEnv::new();
    assert_golden(&env, "inbox.json", &["inbox", "--format", "json"]);
}

#[test]
fn config_shape_stable() {
    let env = TestEnv::new();
    assert_golden(&env, "config.json", &["config", "show", "--format", "json"]);
}
