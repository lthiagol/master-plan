//! M97 — `mp note add` ergonomics tests.
//!
//! Covers: `--body @file` (AC-07), `--body @-` stdin (AC-08), and clear
//! error on `--to <invalid>` listing valid destinations (AC-09). Also guards
//! the inline-body regression, the missing-file error, and empty-stdin edge.

use crate::common::TestEnv;

#[test]
fn note_add_body_from_file() {
    let env = TestEnv::new();
    let body_path = env.tmp.path().join("body.md");
    std::fs::write(
        &body_path,
        "# Heading\n\nmarkdown with `backticks` and **bold**",
    )
    .unwrap();

    let body_arg = format!("@{}", body_path.display());
    let out = env.run(&["note", "add", "--title", "from file", "--body", &body_arg]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
    let idea_id = v["idea_id"].as_str().unwrap().to_string();

    // Verify the body round-tripped into the idea (markdown intact).
    let out = env.run(&["idea", "show", &idea_id]);
    assert!(out.status.success());
    let idea: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let body = idea["idea"]["body"]
        .as_str()
        .or_else(|| idea["body"].as_str())
        .expect("idea body field present");
    assert!(
        body.contains("backticks") && body.contains("Heading"),
        "body should round-trip markdown: {body}"
    );
}

#[test]
fn note_add_body_from_stdin() {
    let env = TestEnv::new();

    // Spawn the process and feed stdin ourselves (the TestEnv helper shells
    // out without a way to pipe stdin).
    use std::process::Stdio;
    let mut child = std::process::Command::new(crate::common::mp_bin())
        .args([
            "note",
            "add",
            "--title",
            "from stdin",
            "--body",
            "@-",
            "--project-root",
            env.tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mp");

    use std::io::Write;
    let stdin = child.stdin.as_mut().expect("stdin");
    stdin
        .write_all(b"streamed body content with `code`")
        .unwrap();
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
    let idea_id = v["idea_id"].as_str().unwrap().to_string();

    let out = env.run(&["idea", "show", &idea_id]);
    assert!(out.status.success());
    let idea: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let body = idea["idea"]["body"]
        .as_str()
        .or_else(|| idea["body"].as_str())
        .expect("idea body field present");
    assert!(
        body.contains("streamed body content"),
        "stdin body should round-trip: {body}"
    );
}

#[test]
fn note_add_invalid_to_lists_destinations() {
    let env = TestEnv::new();

    let out = env.run(&["note", "add", "--title", "x", "--to", "nowhere"]);
    assert!(
        !out.status.success(),
        "invalid --to must exit non-zero: {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("idea"),
        "stderr should list 'idea' as a valid destination: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("valid destinations"),
        "stderr should mention valid destinations: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Regression guards for the pre-existing inline path (no `@`).
// ---------------------------------------------------------------------------

#[test]
fn note_add_inline_body_still_works() {
    // Bodies without a leading `@` must be passed through verbatim — this is
    // the pre-M97 path and must not regress now that `@file`/`@-` exist.
    let env = TestEnv::new();
    let out = env.run(&[
        "note",
        "add",
        "--title",
        "inline",
        "--body",
        "plain inline body, no leading at-sign",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let idea_id = v["idea_id"].as_str().unwrap().to_string();

    let out = env.run(&["idea", "show", &idea_id]);
    assert!(out.status.success());
    let idea: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let body = idea["idea"]["body"]
        .as_str()
        .or_else(|| idea["body"].as_str())
        .expect("idea body field present");
    assert_eq!(body, "plain inline body, no leading at-sign");
}

#[test]
fn note_add_body_missing_file_errors_clearly() {
    let env = TestEnv::new();
    let body_arg = format!("@{}/does-not-exist.md", env.tmp.path().display());
    let out = env.run(&["note", "add", "--title", "x", "--body", &body_arg]);
    assert!(
        !out.status.success(),
        "missing body file must exit non-zero: {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("could not read body file"),
        "stderr should explain the unreadable body file: {stderr}"
    );
}

#[test]
fn note_add_body_empty_stdin_is_not_an_error() {
    // `@-` with no input must not hang or panic — it just produces an empty body.
    use std::process::Stdio;
    let env = TestEnv::new();
    let mut child = std::process::Command::new(crate::common::mp_bin())
        .args([
            "note",
            "add",
            "--title",
            "empty stdin",
            "--body",
            "@-",
            "--project-root",
            env.tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mp");
    // Close stdin immediately (empty input).
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "empty stdin body must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["ok"].as_bool().unwrap());
}

// ---------------------------------------------------------------------------
// F-03 (M97 review): --body-file is the unambiguous path for file/stdin
// bodies. It must accept content that legitimately starts with '@', which the
// legacy --body @<path> form would misread as a file path.
// ---------------------------------------------------------------------------

#[test]
fn note_add_body_file_accepts_at_prefixed_content() {
    // A body whose text starts with '@' is the F-03 escape case: impossible via
    // --body (it's read as @<path>), must work via --body-file.
    let env = TestEnv::new();
    let body_path = env.tmp.path().join("at-body.md");
    std::fs::write(&body_path, "@username ping and @channel fyi").unwrap();

    let out = env.run(&[
        "note",
        "add",
        "--title",
        "at-body",
        "--body-file",
        body_path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let idea_id = v["idea_id"].as_str().unwrap().to_string();

    let out = env.run(&["idea", "show", &idea_id]);
    assert!(out.status.success());
    let idea: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let body = idea["idea"]["body"]
        .as_str()
        .or_else(|| idea["body"].as_str())
        .unwrap();
    assert!(
        body.starts_with("@username ping") && body.contains("@channel"),
        "body should preserve leading '@' verbatim: {body}"
    );
}

#[test]
fn note_add_body_file_reads_stdin_with_dash() {
    use std::io::Write;
    let env = TestEnv::new();
    let mut child = std::process::Command::new(crate::common::mp_bin())
        .args([
            "note",
            "add",
            "--title",
            "stdin-file",
            "--body-file",
            "-",
            "--project-root",
            env.tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn mp");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"body from stdin via --body-file")
        .unwrap();
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let idea_id = v["idea_id"].as_str().unwrap().to_string();

    let out = env.run(&["idea", "show", &idea_id]);
    assert!(out.status.success());
    let idea: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let body = idea["idea"]["body"]
        .as_str()
        .or_else(|| idea["body"].as_str())
        .unwrap();
    assert!(
        body.contains("body from stdin via --body-file"),
        "stdin body round-trip: {body}"
    );
}

#[test]
fn note_add_body_and_body_file_are_mutually_exclusive() {
    let env = TestEnv::new();
    let out = env.run(&[
        "note",
        "add",
        "--title",
        "x",
        "--body",
        "inline",
        "--body-file",
        "/dev/null",
    ]);
    assert!(
        !out.status.success(),
        "passing both --body and --body-file must error: {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("mutually exclusive"),
        "stderr should explain the conflict: {stderr}"
    );
}

#[test]
fn note_add_body_file_missing_errors_with_path() {
    let env = TestEnv::new();
    let missing = env.tmp.path().join("no-such-file.md");
    let out = env.run(&[
        "note",
        "add",
        "--title",
        "x",
        "--body-file",
        missing.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--body-file") && stderr.contains("could not read"),
        "stderr should name --body-file and the read failure: {stderr}"
    );
}

#[test]
fn note_add_body_file_expands_tilde() {
    // F-04: a quoted '~/...' must expand via $HOME. We point HOME at the test
    // tmpdir so the test never touches the real home directory.
    use std::process::Command;
    let env = TestEnv::new();
    let home = env.tmp.path().to_path_buf();
    let body_path = home.join("note.md");
    std::fs::write(&body_path, "tilde-expanded body").unwrap();

    let out = Command::new(crate::common::mp_bin())
        .args([
            "note",
            "add",
            "--title",
            "tilde",
            "--body-file",
            "~/note.md",
            "--project-root",
            env.tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("HOME", home)
        .output()
        .expect("run mp");
    assert!(
        out.status.success(),
        "tilde expansion should resolve --body-file ~/note.md: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let idea_id = v["idea_id"].as_str().unwrap().to_string();

    let out = env.run(&["idea", "show", &idea_id]);
    assert!(out.status.success());
    let idea: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let body = idea["idea"]["body"]
        .as_str()
        .or_else(|| idea["body"].as_str())
        .unwrap();
    assert_eq!(body, "tilde-expanded body");
}
