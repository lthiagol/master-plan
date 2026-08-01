//! M167 WP1 S2 / AC-07: ensure `Action::ToggleTabFocus`, `Keybinds::toggle_tab_focus`,
//! and `app.tab_bar_focused` no longer leak into user code. Spec / CHANGELOG
//! mentions are tolerated; this scan greps every Rust source file under
//! `crates/raul/` (excluding this test binary) for any reference.

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &["ToggleTabFocus", "toggle_tab_focus", "tab_bar_focused"];

#[test]
fn no_focus_toggle_remnants() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let crates_src = workspace_root.join("crates/raul/src");
    let this_test = PathBuf::from(file!());

    let mut offenders: Vec<String> = Vec::new();
    walk(&crates_src, &this_test, &mut offenders);
    assert!(
        offenders.is_empty(),
        "M167 AC-07: forbidden focus-toggle identifiers still present in raul user code:\n  {}",
        offenders.join("\n  ")
    );
}

fn walk(root: &Path, this_test: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            walk(&path, this_test, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if path == this_test {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        for line in contents.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for needle in FORBIDDEN {
                if line.contains(needle) {
                    let rel = path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .display()
                        .to_string();
                    out.push(format!("{rel}: {line}"));
                    break;
                }
            }
        }
    }
}
