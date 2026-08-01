//! Rewrite committed JSON golden fixtures from current `mp` / model output.
//!
//! Invoked via `make regen-goldens` (not part of the default test suite).
//! After a deliberate schema change, regenerate then re-run the golden
//! compare tests to confirm the new fixtures match.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use mp::model::{TrackFile, TrackItem, TrackMeta};
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn mp_bin(root: &Path) -> PathBuf {
    if let Ok(p) = std::env::var("MP_BIN") {
        return PathBuf::from(p);
    }
    let release = root.join("target/release/mp");
    if release.is_file() {
        return release;
    }
    let debug = root.join("target/debug/mp");
    if debug.is_file() {
        return debug;
    }
    panic!(
        "mp binary not found (set MP_BIN or run `make build` first); looked under {}",
        root.join("target").display()
    );
}

fn capture_json(mp: &Path, cwd: &Path, home: &Path, args: &[&str]) -> String {
    let out = Command::new(mp)
        .current_dir(cwd)
        .env("MP_HOME", home)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn mp {}: {e}", args.join(" ")));
    assert!(
        out.status.success(),
        "mp {} failed:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

fn regen_json_shape(root: &Path, mp: &Path) {
    let dir = root.join("tests/fixtures/json-shape");
    fs::create_dir_all(&dir).expect("create json-shape dir");

    let tmp = TempDir::new().expect("tempdir");
    capture_json(
        mp,
        tmp.path(),
        root,
        &["init", "--profile", "full", "--format", "json"],
    );

    let captures: &[(&str, &[&str])] = &[
        ("status.json", &["status", "--format", "json"]),
        ("path.json", &["path", "--format", "json"]),
        (
            "list-milestones.json",
            &["list", "milestones", "--format", "json"],
        ),
        ("inbox.json", &["inbox", "--format", "json"]),
        ("config.json", &["config", "show", "--format", "json"]),
    ];
    for (name, args) in captures {
        let body = capture_json(mp, tmp.path(), root, args);
        let path = dir.join(name);
        fs::write(&path, body).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        println!("  wrote {}", path.display());
    }
}

fn track_meta() -> TrackMeta {
    TrackMeta {
        kind: "tweak".to_string(),
        title: "Tweaks & Small Fixes".to_string(),
        perpetual: true,
        scope: "repo-wide".to_string(),
        created: "2026-06-01".to_string(),
    }
}

fn track_item(id: &str, title: &str, status: &str) -> TrackItem {
    TrackItem {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        effort: "S".to_string(),
        problem: format!("Problem for {id}"),
        done_when: format!("Done when {id}"),
        verification: format!("cargo test {id}"),
        steps: Vec::new(),
        evidence: String::new(),
        created: "2026-06-01".to_string(),
        completed: String::new(),
        archived_at: String::new(),
    }
}

fn full_track() -> TrackFile {
    let mut tw01 = track_item("TW-01", "Fix typo in README", "in-progress");
    tw01.steps = vec!["locate typo".to_string(), "patch line".to_string()];
    tw01.evidence = "commit abc123".to_string();
    let tw02 = track_item("TW-02", "Empty-steps item", "planned");
    let mut tw03 = track_item("TW-03", "Archived tweak", "archived");
    tw03.archived_at = "2026-06-10".to_string();
    TrackFile {
        track: track_meta(),
        items: vec![tw01, tw02, tw03],
    }
}

fn empty_archived_track() -> TrackFile {
    let mut tw01 = track_item("TW-01", "Fix typo in README", "in-progress");
    tw01.steps = vec!["locate typo".to_string(), "patch line".to_string()];
    tw01.evidence = "commit abc123".to_string();
    let tw02 = track_item("TW-02", "Empty-steps item", "planned");
    TrackFile {
        track: track_meta(),
        items: vec![tw01, tw02],
    }
}

fn regen_track(root: &Path) {
    let fixtures = root.join("tests/fixtures");
    let full = serde_json::to_string_pretty(&full_track()).expect("serialize full");
    let empty = serde_json::to_string_pretty(&empty_archived_track()).expect("serialize empty");
    let full_path = fixtures.join("track-render-golden.json");
    let empty_path = fixtures.join("track-render-golden-empty-archived.json");
    fs::write(&full_path, full).expect("write track golden");
    fs::write(&empty_path, empty).expect("write track empty golden");
    println!("  wrote {}", full_path.display());
    println!("  wrote {}", empty_path.display());
}

fn main() -> ExitCode {
    let root = repo_root();
    let mp = mp_bin(&root);
    println!("regen-goldens: mp={}", mp.display());
    println!("json-shape fixtures:");
    regen_json_shape(&root, &mp);
    println!("track fixtures:");
    regen_track(&root);
    println!("done. Review git diff under tests/fixtures/, then re-run golden compare tests.");
    ExitCode::SUCCESS
}
