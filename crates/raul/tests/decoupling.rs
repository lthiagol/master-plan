//! Gate test: raul imports no mp logic/store/validate/CLI modules.
//! raul depends ONLY on mp-model for types.
use std::fs;
use std::path::{Path, PathBuf};

fn raul_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn check_file(path: &Path) -> Vec<String> {
    let content = fs::read_to_string(path).unwrap();
    let mut violations = Vec::new();
    let forbidden = ["use mp::", "extern crate mp", "crate::mp::"];
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("//!") {
            continue;
        }
        for pattern in &forbidden {
            if trimmed.contains(pattern) {
                violations.push(format!("{}: '{}'", path.display(), trimmed));
            }
        }
    }
    violations
}

fn walk_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn raul_imports_no_mp_logic() {
    let mut files = Vec::new();
    walk_rs(&raul_src(), &mut files);
    assert!(
        !files.is_empty(),
        "expected .rs files under {}",
        raul_src().display()
    );
    let mut violations = Vec::new();
    for path in &files {
        violations.extend(check_file(path));
    }
    assert!(
        violations.is_empty(),
        "raul imports mp logic modules (only mp-model allowed):\n{}",
        violations.join("\n")
    );
}

#[test]
fn mp_model_has_no_io_imports() {
    let model_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mp-model/src");
    let mut files = Vec::new();
    walk_rs(&model_src, &mut files);
    let forbidden = ["std::fs", "std::net", "std::process"];
    let mut violations = Vec::new();
    for path in &files {
        let content = fs::read_to_string(path).unwrap();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("//!") {
                continue;
            }
            for pattern in &forbidden {
                if trimmed.contains(pattern) {
                    violations.push(format!("{}: {}", path.display(), trimmed));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "mp-model must stay I/O-free:\n{}",
        violations.join("\n")
    );
}
