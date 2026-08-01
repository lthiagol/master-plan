//! M184 S5: no Lane::Tweaks / production LANE_TWEAKS usage remains
//! under crates/raul/src/ (constant definition itself is allowed).

#[test]
fn no_lane_tweaks_in_production_sources() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut lane_hits = Vec::new();
    let mut lane_const_uses = Vec::new();
    fn walk(dir: &std::path::Path, lane_hits: &mut Vec<String>, const_hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, lane_hits, const_hits);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let body = std::fs::read_to_string(&path).unwrap();
            let rel = path.display().to_string();
            if body.contains("Lane::Tweaks") {
                lane_hits.push(rel.clone());
            }
            // LANE_TWEAKS may only appear in lanes.rs (canonical def + tests).
            if body.contains("LANE_TWEAKS") {
                let is_lanes = path.file_name().is_some_and(|n| n == "lanes.rs");
                if !is_lanes {
                    const_hits.push(rel);
                }
            }
        }
    }
    walk(&root, &mut lane_hits, &mut lane_const_uses);
    assert!(
        lane_hits.is_empty(),
        "Lane::Tweaks must not appear under src/; found {lane_hits:?}"
    );
    assert!(
        lane_const_uses.is_empty(),
        "LANE_TWEAKS must only live in lanes.rs; found {lane_const_uses:?}"
    );
}
