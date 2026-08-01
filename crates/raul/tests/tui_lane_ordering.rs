//! M184 AC-01: Lane::ordered() is exactly 7 lanes; Tweaks is gone.

use raul::tui::app::Lane;

#[test]
fn ordered_is_seven_without_tweaks() {
    let lanes = Lane::ordered();
    assert_eq!(
        lanes,
        vec![
            Lane::Overview,
            Lane::Milestones,
            Lane::Path,
            Lane::Backlog,
            Lane::Ideas,
            Lane::Watch,
            Lane::Settings,
        ]
    );
    assert_eq!(lanes.len(), 7);
}

#[test]
fn source_has_no_lane_tweaks_variant() {
    // Pin AC-01: Lane::Tweaks must not appear in production sources.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let body = std::fs::read_to_string(&path).unwrap();
                if body.contains("Lane::Tweaks") {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    walk(&root, &mut hits);
    assert!(
        hits.is_empty(),
        "Lane::Tweaks must not appear under crates/raul/src/; found in {hits:?}"
    );
}
