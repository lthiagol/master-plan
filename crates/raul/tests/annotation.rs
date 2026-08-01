use std::fs;
use std::path::PathBuf;

/// Gate test: raul never writes annotations.toml directly.
/// All annotation writes must go through `mp annotation` delegation.
#[test]
fn raul_has_no_annotations_toml_write() {
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();

    let forbidden = [
        "annotations.toml",
        "save_annotations",
        "write_annotations",
        "AnnotationFile",
        "enforce_annotations",
    ];

    fn walk(dir: &std::path::Path, forbidden: &[&str], violations: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, forbidden, violations);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let content = fs::read_to_string(&path).unwrap();
                for (line_no, line) in content.lines().enumerate() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("//") || trimmed.starts_with("//!") {
                        continue;
                    }
                    if trimmed.contains("mp annotation") || trimmed.contains("\"annotation\"") {
                        continue;
                    }
                    // TUI load_annotations is a runner helper name, not a write.
                    if trimmed.contains("load_annotations") {
                        continue;
                    }
                    for pattern in forbidden {
                        if trimmed.contains(pattern) {
                            violations.push(format!(
                                "{}:{}: '{}' (contains '{}')",
                                path.display(),
                                line_no + 1,
                                trimmed,
                                pattern
                            ));
                        }
                    }
                }
            }
        }
    }
    walk(&src_dir, &forbidden, &mut violations);

    for v in &violations {
        eprintln!("VIOLATION: {}", v);
    }
    assert!(
        violations.is_empty(),
        "raul source contains annotations.toml write/logic patterns:\n{}",
        violations.join("\n")
    );
}

#[test]
fn raul_annotation_delegates_to_mp() {
    // M164: CLI commands/annotation.rs removed; TUI path is runner_helpers.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/tui/runner_helpers.rs");
    let content = fs::read_to_string(&path).unwrap();

    assert!(
        content.contains("\"annotation\""),
        "raul TUI annotation helpers must shell out to mp annotation"
    );
    for pattern in ["fs::write", "atomic_write", "File::create"] {
        assert!(
            !content.contains(pattern),
            "runner_helpers must not contain '{pattern}' (direct file write)"
        );
    }
}

#[test]
fn ac11_annotation_handles_malformed_stdout() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/tui/runner_helpers.rs");
    let content = fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("run_raw_allow_failure") || content.contains("parse_mp_ok_response"),
        "annotation helpers must handle mp failures gracefully"
    );
}

#[test]
fn malformed_mp_stdout_returns_error_not_panic() {
    let err =
        raul::tui::runner::parse_mp_ok_response(b"not valid json{{{", b"", "annotation create");
    assert!(err.is_err(), "malformed stdout must return Err, not panic");
    let msg = err.unwrap_err().to_string();
    assert!(
        msg.contains("annotation create"),
        "error should name the action: {msg}"
    );
}

#[test]
fn mp_ok_false_returns_descriptive_error() {
    let err = raul::tui::runner::parse_mp_ok_response(
        br#"{"ok":false,"error":"milestone not approvable"}"#,
        b"",
        "milestone approve",
    );
    assert!(err.is_err());
    let msg = err.unwrap_err().to_string();
    assert!(msg.contains("milestone approve"));
    assert!(
        msg.contains("not approvable"),
        "should surface mp error field: {msg}"
    );
}
