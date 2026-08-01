use std::fs;
use std::path::PathBuf;

fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn scan_dir(dir: &std::path::Path, violations: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, violations);
        } else if path.extension().is_some_and(|e| e == "rs") {
            scan_file(&path, violations);
        }
    }
}

fn scan_file(path: &std::path::Path, violations: &mut Vec<String>) {
    let content = fs::read_to_string(path).unwrap();
    let forbidden = ["annotations.toml", "AnnotationFile", "enforce_annotations"];

    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("//!") {
            continue;
        }
        // Allow mentions in mp annotation shell-out commands or tests
        if trimmed.contains("mp annotation")
            || trimmed.contains("\"annotation\"")
            || trimmed.contains("runner.run")
            || trimmed.contains("runner.run_stdin")
            || trimmed.contains("reader.run")
        {
            continue;
        }
        for pattern in &forbidden {
            if trimmed.contains(pattern) {
                violations.push(format!(
                    "{}:{}: '{}' (contains '{}')",
                    path.file_name().unwrap().to_string_lossy(),
                    line_no + 1,
                    trimmed,
                    pattern
                ));
            }
        }
    }
}

/// Gate test: raul source (including tui/) has NO annotations.toml write path.
/// All annotation writes must go through `mp annotation` delegation.
#[test]
fn tui_has_no_annotations_toml_write() {
    let tui_dir = src_dir().join("tui");
    if !tui_dir.exists() {
        return;
    }

    let mut violations = Vec::new();
    scan_dir(&tui_dir, &mut violations);

    for v in &violations {
        eprintln!("VIOLATION: {}", v);
    }
    assert!(
        violations.is_empty(),
        "raul tui source contains annotations.toml write/logic patterns:\n{}",
        violations.join("\n")
    );
}

/// Gate test: all raul source files reference annotations only via mp shell-out.
#[test]
fn raul_full_tree_no_annotation_file_writes() {
    let mut violations = Vec::new();
    scan_dir(&src_dir(), &mut violations);

    for v in &violations {
        eprintln!("VIOLATION: {}", v);
    }
    assert!(
        violations.is_empty(),
        "raul source contains annotations.toml write/logic patterns:\n{}",
        violations.join("\n")
    );
}

/// Gate test: raul source file references to annotations go through mp shell-out.
#[test]
fn raul_tui_annotation_actions_delegate_to_mp() {
    // M136: the data-loading helpers (incl. annotation shell-outs) moved
    // from `runner.rs` to `runner_helpers.rs`. Both files participate in
    // the gate: `runner.rs` re-exports `runner_helpers::*`, but the
    // shell-out sites themselves live in the helper module.
    let runner_path = src_dir().join("tui").join("runner.rs");
    let helpers_path = src_dir().join("tui").join("runner_helpers.rs");
    if !runner_path.exists() && !helpers_path.exists() {
        return;
    }

    let runner_content = fs::read_to_string(&runner_path).unwrap_or_default();
    let helpers_content = fs::read_to_string(&helpers_path).unwrap_or_default();

    let combined = format!("{runner_content}\n{helpers_content}");

    // Must delegate create to mp annotation via run_stdin
    assert!(
        combined.contains("run_stdin(\"annotation\""),
        "TUI runner must delegate annotation create to mp via run_stdin"
    );

    // Must delegate resolve to mp annotation
    assert!(
        combined.contains("run_raw(\"annotation\", &[\"resolve\""),
        "TUI runner must delegate annotation resolve to mp via run_raw"
    );

    // Must delegate reopen to mp annotation
    assert!(
        combined.contains("run_raw(\"annotation\", &[\"reopen\""),
        "TUI runner must delegate annotation reopen to mp via run_raw"
    );

    // Must not contain any direct file write operations in either file
    let write_patterns = [
        "fs::write",
        "atomic_write",
        "File::create",
        "write!(",
        "save_annotation",
    ];
    for pattern in &write_patterns {
        assert!(
            !combined.contains(pattern),
            "raul tui runner must not contain '{}' (direct file write)",
            pattern
        );
    }
}
