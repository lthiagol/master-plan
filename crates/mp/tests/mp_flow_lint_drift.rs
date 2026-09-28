//! M231: the mp-flow role-binding table is lint-locked to `stages.toml`.
//!
//! `scripts/mp_flow_lint.py` is what keeps a stage rename in one file
//! from silently drifting the other. Before this contract existed the
//! lint only checked that `SKILL.md` had a `## <name>` section per
//! `[[stages]]` entry, so the role-binding table could say
//! `Define outcome` where the manifest said `Draft` and the lint still
//! passed — which is exactly what shipped (7 of 12 Name cells drifted).
//!
//! These tests drive the real script over a fixture pair, so the gate
//! runs the same code path `make mp-flow-lint` runs:
//!
//!   good/    — canonical table; the lint must exit 0.
//!   drifted/ — the pre-fix table shape; the lint must exit non-zero and
//!              name the stage, the field, and the expected vs actual
//!              value for each mismatch.
//!
//! A drifting fixture would fail the `good` case; a check that stopped
//! comparing would fail the `drifted` case. Both directions are needed
//! for the gate to mean anything.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Repo root, resolved from this test crate's `CARGO_MANIFEST_DIR`.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn fixture_dir(name: &str) -> PathBuf {
    repo_root()
        .join("tests")
        .join("fixtures")
        .join("mp-flow-lint")
        .join(name)
}

/// Run `scripts/mp_flow_lint.py` against a fixture pair via the same
/// `--skill` / `--manifest` overrides the negative fixture relies on.
fn run_lint(dir: &Path) -> Output {
    Command::new("python3")
        .arg(repo_root().join("scripts").join("mp_flow_lint.py"))
        .arg("--skill")
        .arg(dir.join("SKILL.md"))
        .arg("--manifest")
        .arg(dir.join("stages.toml"))
        .current_dir(repo_root())
        .output()
        .expect("spawn python3 scripts/mp_flow_lint.py")
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// The canonical fixture pair passes. Without this, a check that failed
/// everything would satisfy the drifted case below.
#[test]
fn good_fixture_passes_the_lint() {
    let dir = fixture_dir("good");
    let out = run_lint(&dir);
    assert!(
        out.status.success(),
        "good fixture must pass, got exit {:?}\n--- stderr ---\n{}",
        out.status.code(),
        stderr_of(&out)
    );
    assert!(
        stdout_of(&out).contains("12 role-binding rows match the manifest"),
        "pass output should report the row check ran; got:\n{}",
        stdout_of(&out)
    );
}

/// The drifted table is rejected — the whole point of the gate.
#[test]
fn drifted_fixture_is_rejected_by_the_lint() {
    let dir = fixture_dir("drifted");
    let out = run_lint(&dir);
    assert!(
        !out.status.success(),
        "drifted fixture must fail the lint, but it exited 0:\n{}",
        stdout_of(&out)
    );
}

/// The diagnostic names the stage, the field, and expected vs actual —
/// not a generic "table out of date" message.
#[test]
fn drifted_fixture_reports_per_row_diagnostics() {
    let dir = fixture_dir("drifted");
    let out = run_lint(&dir);
    let stderr = stderr_of(&out);
    assert!(
        stderr.contains("mp-flow-lint: FAIL"),
        "expected a FAIL banner; got:\n{stderr}"
    );

    // Every drifted Name cell: the stage, the field, expected, actual.
    for (stage, expected, actual) in [
        ("1", "Draft", "Define outcome"),
        ("2", "Groom", "Interview & shape"),
        ("3", "Specify", "Write acceptance"),
        ("4", "Approve", "Approve spec"),
        ("7", "Complete", "Mark complete"),
        ("9", "Remediate", "Remediate findings"),
        ("12", "Hand off", "Hand-off"),
    ] {
        let want = format!(
            "role-binding table: stage {stage} Name mismatch: expected '{expected}', found '{actual}'"
        );
        assert!(
            stderr.contains(&want),
            "missing per-row diagnostic:\n  want: {want}\n  got:\n{stderr}"
        );
    }

    // The Owner cell is checked too, not just the names.
    let owner = "role-binding table: stage 6 (Self-review) Owner mismatch: expected 'runner', found 'coordinator'";
    assert!(
        stderr.contains(owner),
        "owner drift must be reported:\n  want: {owner}\n  got:\n{stderr}"
    );
}

/// `--json` exposes the parsed rows so a consumer can diff them
/// programmatically, not just read prose.
#[test]
fn json_output_reports_rows_for_both_fixtures() {
    let script = repo_root().join("scripts").join("mp_flow_lint.py");

    let good = Command::new("python3")
        .arg(&script)
        .arg("--skill")
        .arg(fixture_dir("good").join("SKILL.md"))
        .arg("--manifest")
        .arg(fixture_dir("good").join("stages.toml"))
        .arg("--json")
        .current_dir(repo_root())
        .output()
        .expect("spawn lint (good)");
    let good_json: serde_json::Value =
        serde_json::from_str(&stdout_of(&good)).expect("good fixture emits valid JSON");
    assert_eq!(good_json["ok"], serde_json::json!(true));
    assert_eq!(good_json["row_count"], serde_json::json!(12));
    assert_eq!(good_json["errors"].as_array().map(Vec::len), Some(0));
    let rows = good_json["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 12, "the table has 12 rows");
    for row in rows {
        assert_eq!(row["ok"], serde_json::json!(true), "row must match: {row}");
    }

    let drifted = Command::new("python3")
        .arg(&script)
        .arg("--skill")
        .arg(fixture_dir("drifted").join("SKILL.md"))
        .arg("--manifest")
        .arg(fixture_dir("drifted").join("stages.toml"))
        .arg("--json")
        .current_dir(repo_root())
        .output()
        .expect("spawn lint (drifted)");
    let drifted_json: serde_json::Value =
        serde_json::from_str(&stdout_of(&drifted)).expect("drifted fixture emits valid JSON");
    assert_eq!(drifted_json["ok"], serde_json::json!(false));
    assert_eq!(drifted_json["row_count"], serde_json::json!(12));
    assert_eq!(
        drifted_json["errors"].as_array().map(Vec::len),
        Some(8),
        "7 renamed Name cells + 1 flipped Owner cell"
    );
    let rows = drifted_json["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 12, "all 12 rows are still parsed");
    let bad: Vec<&serde_json::Value> = rows.iter().filter(|r| r["ok"] != true).collect();
    assert_eq!(bad.len(), 8, "8 rows disagree with the manifest");
}

/// The shipped skill passes against the shipped manifest — the pair
/// `make mp-flow-lint` gates in CI. Without the `--skill`/`--manifest`
/// overrides, so the default paths are what gets exercised.
#[test]
fn shipped_skill_matches_shipped_manifest() {
    let out = Command::new("python3")
        .arg(repo_root().join("scripts").join("mp_flow_lint.py"))
        .current_dir(repo_root())
        .output()
        .expect("spawn lint (repo defaults)");
    assert!(
        out.status.success(),
        "templates/skills/mp-flow must stay in sync with stages.toml; \
         got exit {:?}\n--- stderr ---\n{}",
        out.status.code(),
        stderr_of(&out)
    );
}
