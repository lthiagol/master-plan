//! M185 AC-01: 7 lanes; Grooming gone from ordered and sources.

use raul::tui::app::Lane;

#[test]
fn ordered_is_seven_without_grooming() {
    assert_eq!(
        Lane::ordered(),
        vec![
            Lane::Overview,
            Lane::Milestones,
            Lane::Path,
            Lane::Backlog,
            Lane::Ideas,
            Lane::Autopilot,
            Lane::Settings,
        ]
    );
}

#[test]
fn no_lane_grooming_in_src() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, hits);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let body = std::fs::read_to_string(&p).unwrap();
                if body.contains("Lane::Grooming") {
                    hits.push(p.display().to_string());
                }
            }
        }
    }
    walk(&root, &mut hits);
    assert!(hits.is_empty(), "Lane::Grooming remnants: {hits:?}");
}
