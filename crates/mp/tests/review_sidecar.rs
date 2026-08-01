//! M173 S4: `mp review sidecar <milestone> [--finding F-XX]
//! --output <path>`. Tests cover: (a) sidecar file lands at the
//! requested path with the documented hunk shape, (b) `--finding`
//! filters to a single finding, (c) an unknown finding id errors
//! loudly, (d) the config-gate (`[review] hunk = false`) blocks
//! the command.

mod common;

use std::fs;

use common::TestEnv;

/// Write a milestone JSON to the test plan dir, plus an empty
/// reviews.json so the export has a stable input shape. The
/// milestone carries one open finding at a known file/line so the
/// sidecar shape assertions are stable.
fn write_minimal_milestone_with_finding(
    env: &TestEnv,
    id: &str,
    slug: &str,
    finding_id: &str,
    finding_path: &str,
    finding_line: u32,
) {
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let body = serde_json::json!({
        "milestone": {
            "id": id,
            "title": "review-sidecar test",
            "slug": slug,
            "lifecycle": "in-progress",
            "spec_status": "ready",
            "execution_status": "in-progress",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "priority": "normal",
            "created": "2026-07-16",
            "updated": "2026-07-16",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "sidecar test" },
        "problem": { "description": "sidecar test" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "x",
            "verification": "manual: test",
            "status": "pending",
            "evidence": "",
        }],
        "findings": [{
            "id": finding_id,
            "severity": "high",
            "category": "L6",
            "description": "gate parity regression",
            "summary": "bulk bypasses the gate",
            "rationale": "differs from single-id",
            "author": "reviewer",
            "status": "open",
            "fixed_in": "",
            "created": "2026-07-16",
            "resolved": "",
            "phase": "external",
            "anchor": {
                "path": finding_path,
                "side": "new",
                "new_range": { "start_line": finding_line, "end_line": finding_line },
            },
        }],
    });
    let json = serde_json::to_string_pretty(&body).unwrap();
    fs::write(dir.join(format!("{id}-{slug}.json")), format!("{json}\n")).unwrap();

    // Empty reviews.json — no comments for the sidecar.
    let reviews_dir = env.tmp.path().join("master-plan");
    fs::write(
        reviews_dir.join("reviews.json"),
        r#"{"reviews": [], "comments": []}"#,
    )
    .unwrap();

    // Enable review.hunk via the `mp config set` command. The gate
    // is a JSON-config field (`config.json`); `mp config set` is
    // the documented surface for write paths.
    let out = env.run(&["config", "set", "review.hunk", "true"]);
    assert!(
        out.status.success(),
        "enable review.hunk: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Run `mp review sidecar` against a test plan dir. Returns
/// `(output, sidecar_path)`.
fn run_sidecar(env: &TestEnv, args: &[&str]) -> (std::process::Output, std::path::PathBuf) {
    let sidecar_path = env.tmp.path().join("sidecar.json");
    let root = common::repo_root();
    let plan_dir = env.tmp.path().join("master-plan");
    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .arg("--plan-dir")
        .arg(&plan_dir)
        .args(args);
    let out = cmd.output().expect("spawn mp");
    (out, sidecar_path)
}

/// AC-04: `mp review sidecar <id> --output <path>` writes a
/// hunk-compatible sidecar at the given path with the documented
/// shape (`version` + `files[].annotations[]`).
#[test]
fn review_sidecar_writes_hunk_shape_to_output_path() {
    let env = TestEnv::blank();
    write_minimal_milestone_with_finding(&env, "170", "sidecar-test", "F-01", "src/foo.rs", 12);

    let sidecar_path = env.tmp.path().join("sidecar.json");
    let sidecar_path_str = sidecar_path.to_string_lossy().to_string();

    let (out, sidecar_path_returned) = run_sidecar(
        &env,
        &["review", "sidecar", "170", "--output", &sidecar_path_str],
    );
    let sidecar_path = sidecar_path_returned;
    assert!(
        out.status.success(),
        "review sidecar must succeed: stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(sidecar_path.is_file(), "sidecar.json must be written");

    let body = fs::read_to_string(&sidecar_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&body).expect("sidecar is valid JSON");

    // The hunk agent-context shape: { version, files: [{ path, annotations: [...] }] }
    assert_eq!(v["version"], 1, "sidecar.version must be 1");
    let files = v["files"].as_array().expect("files[]");
    assert!(!files.is_empty(), "sidecar must carry at least one file");
    let file0 = &files[0];
    assert_eq!(file0["path"], "src/foo.rs");
    let annotations = file0["annotations"].as_array().expect("annotations[]");
    assert_eq!(annotations.len(), 1);
    let ann = &annotations[0];
    assert_eq!(ann["summary"], "bulk bypasses the gate");
    assert_eq!(ann["confidence"], "high", "severity high → confidence high");
    let range = ann["newRange"].as_array().expect("newRange");
    assert_eq!(range[0].as_u64().unwrap(), 12);
    assert_eq!(range[1].as_u64().unwrap(), 12);
}

/// `--finding F-XX` filters the sidecar to a single finding. With
/// one finding on the milestone, the sidecar carries exactly one
/// annotation. We extend the milestone to two findings and assert
/// the filter reduces the count to one.
#[test]
fn review_sidecar_filters_to_one_finding() {
    let env = TestEnv::blank();
    // Two findings: F-01 at src/foo.rs:12 and F-02 at src/bar.rs:7.
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let body = serde_json::json!({
        "milestone": {
            "id": "171", "title": "filter test", "slug": "sidecar-filter",
            "lifecycle": "in-progress", "spec_status": "ready",
            "execution_status": "in-progress", "depends_on": [], "effort": "S",
            "risk": "low", "priority": "normal", "created": "2026-07-16",
            "updated": "2026-07-16", "blocked_at": "", "block_reason": "", "blocked_by": "",
        },
        "intent": { "outcome": "x" },
        "problem": { "description": "x" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{
            "id": "AC-01", "description": "x", "verification": "manual: test",
            "status": "pending", "evidence": "",
        }],
        "findings": [
            {
                "id": "F-01", "severity": "high", "category": "L6",
                "description": "finding one", "summary": "first",
                "rationale": "", "author": "r",
                "status": "open", "fixed_in": "", "created": "2026-07-16",
                "resolved": "",
                "phase": "external",
                "anchor": { "path": "src/foo.rs", "side": "new",
                            "new_range": { "start_line": 12, "end_line": 12 } },
            },
            {
                "id": "F-02", "severity": "low", "category": "L13",
                "description": "finding two", "summary": "second",
                "rationale": "", "author": "r",
                "status": "open", "fixed_in": "", "created": "2026-07-16",
                "resolved": "",
                "phase": "external",
                "anchor": { "path": "src/bar.rs", "side": "new",
                            "new_range": { "start_line": 7, "end_line": 7 } },
            },
        ],
    });
    let json = serde_json::to_string_pretty(&body).unwrap();
    fs::write(dir.join("171-sidecar-filter.json"), format!("{json}\n")).unwrap();

    let plan_dir = env.tmp.path().join("master-plan");
    fs::write(
        plan_dir.join("reviews.json"),
        r#"{"reviews": [], "comments": []}"#,
    )
    .unwrap();
    let out = env.run(&["config", "set", "review.hunk", "true"]);
    assert!(
        out.status.success(),
        "enable review.hunk: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let sidecar_path = env.tmp.path().join("sidecar.json");
    let sidecar_path_str = sidecar_path.to_string_lossy().to_string();
    let (out, _sidecar_path_returned) = run_sidecar(
        &env,
        &[
            "review",
            "sidecar",
            "171",
            "--finding",
            "F-01",
            "--output",
            &sidecar_path_str,
        ],
    );
    assert!(
        out.status.success(),
        "review sidecar with --finding must succeed: stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body = fs::read_to_string(&sidecar_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let files = v["files"].as_array().unwrap();
    // Total annotations across all files must equal 1 (F-01 only).
    let total: usize = files
        .iter()
        .map(|f| f["annotations"].as_array().unwrap().len())
        .sum();
    assert_eq!(total, 1, "--finding must reduce annotations to 1");
    // The single annotation is F-01 (summary "first"), on src/foo.rs.
    let f0 = &files[0];
    assert_eq!(f0["path"], "src/foo.rs");
    assert_eq!(f0["annotations"][0]["summary"], "first");
}

/// `--finding F-XX` for a finding id that doesn't exist on the
/// milestone errors loudly with the available-ids hint.
#[test]
fn review_sidecar_unknown_finding_errors_loudly() {
    let env = TestEnv::blank();
    write_minimal_milestone_with_finding(&env, "172", "sidecar-unknown", "F-01", "src/foo.rs", 12);

    let (out, _sidecar_path) = run_sidecar(
        &env,
        &[
            "review",
            "sidecar",
            "172",
            "--finding",
            "F-99",
            "--output",
            env.tmp.path().join("sidecar.json").to_str().unwrap(),
        ],
    );
    assert!(
        !out.status.success(),
        "unknown finding id must cause non-zero exit"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("F-99"),
        "error must mention the unknown finding id; got: {combined}"
    );
    assert!(
        combined.contains("172"),
        "error must mention the milestone id; got: {combined}"
    );
}

/// The config-gate (`[review] hunk = false`) blocks the sidecar
/// command with a clear error message — same contract as the
/// existing `mp reviews hunk --file`.
#[test]
fn review_sidecar_respects_review_hunk_config_gate() {
    let env = TestEnv::blank();
    write_minimal_milestone_with_finding(&env, "173", "sidecar-gate", "F-01", "src/foo.rs", 12);

    // Flip the config gate to false.
    let out = env.run(&["config", "set", "review.hunk", "false"]);
    assert!(
        out.status.success(),
        "disable review.hunk: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (out, _sidecar_path) = run_sidecar(
        &env,
        &[
            "review",
            "sidecar",
            "173",
            "--output",
            env.tmp.path().join("sidecar.json").to_str().unwrap(),
        ],
    );
    assert!(
        !out.status.success(),
        "review sidecar must fail when [review] hunk = false"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("hunk = false") || combined.contains("review.hunk"),
        "error must point at the config gate; got: {combined}"
    );
}
