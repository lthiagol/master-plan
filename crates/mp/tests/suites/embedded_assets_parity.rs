//! M29: every embedded template and schema must be byte-identical to its
//! on-disk source in the repo. Drives the comparison from the filesystem tree
//! (the source of truth) and resolves each file through the embedded registry.

use std::fs;
use std::path::Path;

use mp::assets::embedded_asset;

use crate::common::repo_root;

#[test]
fn embedded_assets_match_disk() {
    let mut checked = 0;
    let mut mismatches = Vec::new();

    for tree in ["templates", "schemas"] {
        let root = repo_root().join(tree);
        walk(&root, &root, &mut |rel| {
            checked += 1;
            let disk = fs::read_to_string(root.join(rel)).expect("read disk file");
            match embedded_asset(&format!("{tree}/{rel}")) {
                Some(emb) => {
                    if emb != disk {
                        mismatches.push(format!("{tree}/{rel}"));
                    }
                }
                None => mismatches.push(format!("{tree}/{rel} (NOT EMBEDDED)")),
            }
        });
    }

    assert!(
        checked > 0,
        "expected to check at least one template or schema, got {checked}"
    );
    assert!(
        mismatches.is_empty(),
        "embedded assets diverge from disk ({}): {}",
        mismatches.len(),
        mismatches.join(", ")
    );
}

fn walk<F: FnMut(&str)>(repo_file: &Path, root: &Path, f: &mut F) {
    if repo_file.is_file() {
        let rel = repo_file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        f(&rel);
        return;
    }
    if repo_file.is_dir() {
        for entry in fs::read_dir(repo_file).unwrap() {
            let entry = entry.unwrap();
            walk(&entry.path(), root, f);
        }
    }
}
