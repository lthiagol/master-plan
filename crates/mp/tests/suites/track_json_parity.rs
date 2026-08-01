//! M41: post-human-removal track serialization parity tests.
//!
//! Golden fixtures verify that track JSON output is stable.
//! Regenerate with: `make regen-goldens`

use std::fs;

use mp::model::{TrackFile, TrackItem, TrackMeta};

use crate::common::repo_root;

fn meta() -> TrackMeta {
    TrackMeta {
        kind: "tweak".to_string(),
        title: "Tweaks & Small Fixes".to_string(),
        perpetual: true,
        scope: "repo-wide".to_string(),
        created: "2026-06-01".to_string(),
    }
}

fn item(id: &str, title: &str, status: &str) -> TrackItem {
    TrackItem {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        effort: "S".to_string(),
        problem: format!("Problem for {}", id),
        done_when: format!("Done when {}", id),
        verification: format!("cargo test {}", id),
        steps: Vec::new(),
        evidence: String::new(),
        created: "2026-06-01".to_string(),
        completed: String::new(),
        archived_at: String::new(),
    }
}

fn full_track() -> TrackFile {
    let mut tw01 = item("TW-01", "Fix typo in README", "in-progress");
    tw01.steps = vec!["locate typo".to_string(), "patch line".to_string()];
    tw01.evidence = "commit abc123".to_string();
    let tw02 = item("TW-02", "Empty-steps item", "planned");
    let mut tw03 = item("TW-03", "Archived tweak", "archived");
    tw03.archived_at = "2026-06-10".to_string();
    TrackFile {
        track: meta(),
        items: vec![tw01, tw02, tw03],
    }
}

fn empty_archived_track() -> TrackFile {
    let mut tw01 = item("TW-01", "Fix typo in README", "in-progress");
    tw01.steps = vec!["locate typo".to_string(), "patch line".to_string()];
    tw01.evidence = "commit abc123".to_string();
    let tw02 = item("TW-02", "Empty-steps item", "planned");
    TrackFile {
        track: meta(),
        items: vec![tw01, tw02],
    }
}

fn golden_path(name: &str) -> std::path::PathBuf {
    repo_root().join("tests/fixtures").join(name)
}

#[test]
fn full_track_matches_golden() {
    let out = serde_json::to_string_pretty(&full_track()).expect("serialize");
    let golden =
        fs::read_to_string(golden_path("track-render-golden.json")).expect("golden exists");
    assert_eq!(out.trim(), golden.trim());
}

#[test]
fn empty_archived_matches_golden() {
    let out = serde_json::to_string_pretty(&empty_archived_track()).expect("serialize");
    let golden =
        fs::read_to_string(golden_path("track-render-golden-empty-archived.json")).expect("golden");
    assert_eq!(out.trim(), golden.trim());
}
