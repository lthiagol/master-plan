use crate::common::TestEnv;
use std::fs;

#[test]
fn from_repo_surfaces_goals_from_markdown() {
    let env = TestEnv::blank();
    fs::create_dir_all(env.tmp.path().join("src")).expect("src");
    fs::write(
        env.tmp.path().join("Cargo.toml"),
        "[package]\nname = \"demo-app\"\nversion = \"0.1.0\"\n",
    )
    .expect("cargo");

    // Create a status.md with goals and non-goals
    fs::write(
        env.tmp.path().join("status.md"),
        r#"# Project Status

## Goals
- Implement OAuth login for third-party authentication
- Add offline mode with local storage sync
- Support dark mode across all views

## Non-goals
- Real-time collaboration is out of scope for v1
- Mobile app support deferred to v2

## Backlog
- Export to PDF with custom templates
- Keyboard shortcut customization
"#,
    )
    .expect("status.md");

    let out = env.run_with_env(
        &[],
        &[
            "init",
            "--profile",
            "full",
            "--from-repo",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let bootstrap = &json["bootstrap"];
    let suggestions: Vec<&str> = bootstrap["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();

    let all = suggestions.join("\n");
    assert!(
        all.contains("OAuth login"),
        "should surface goal: OAuth login\n{all}"
    );
    assert!(
        all.contains("offline mode"),
        "should surface goal: offline mode\n{all}"
    );
    assert!(
        all.contains("dark mode"),
        "should surface goal: dark mode\n{all}"
    );
    assert!(
        all.contains("Real-time collaboration"),
        "should surface non-goal: Real-time collaboration\n{all}"
    );
    assert!(
        all.contains("Export to PDF"),
        "should surface backlog: Export to PDF\n{all}"
    );
}

#[test]
fn from_repo_surfaces_goals_from_readme() {
    let env = TestEnv::blank();
    fs::create_dir_all(env.tmp.path().join("src")).expect("src");
    fs::write(
        env.tmp.path().join("Cargo.toml"),
        "[package]\nname = \"demo-app\"\nversion = \"0.1.0\"\n",
    )
    .expect("cargo");

    // README with a Goals section
    fs::write(
        env.tmp.path().join("README.md"),
        r#"# Demo App

## Goals
- Fast search across all documents
- Minimal memory footprint
- Cross-platform support (Linux, macOS, Windows)
"#,
    )
    .expect("README.md");

    let out = env.run_with_env(
        &[],
        &[
            "init",
            "--profile",
            "full",
            "--from-repo",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let suggestions: Vec<&str> = json["bootstrap"]["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    let all = suggestions.join("\n");
    assert!(
        all.contains("Fast search"),
        "should surface goal from README\n{all}"
    );
}

#[test]
fn from_repo_no_markdown_no_candidates() {
    let env = TestEnv::blank();
    let out = env.run_with_env(
        &[],
        &[
            "init",
            "--profile",
            "full",
            "--from-repo",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let suggestions: Vec<&str> = json["bootstrap"]["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    let has_candidate = suggestions.iter().any(|s| s.contains("(from markdown)"));
    assert!(
        !has_candidate,
        "no markdown candidates when no markdown files present"
    );
}
