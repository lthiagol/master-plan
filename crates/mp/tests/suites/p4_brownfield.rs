use std::fs;

use crate::common::TestEnv;

#[test]
fn specs_list_show_init() {
    let env = TestEnv::new();

    let init_spec = env.run(&[
        "specs", "init", "api", "--title", "HTTP API", "--format", "json",
    ]);
    assert!(
        init_spec.status.success(),
        "{}",
        String::from_utf8_lossy(&init_spec.stderr)
    );

    let list = env.run(&["specs", "list", "--format", "json"]);
    assert!(list.status.success());
    let domains: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(domains.as_array().unwrap().len(), 1);
    assert_eq!(domains[0]["id"], "api");

    let show = env.run(&["specs", "show", "api", "--format", "json"]);
    assert!(show.status.success());
    let spec: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(spec["domain"]["version"], 1);
}

#[test]
fn brownfield_scan_and_doctor_detected() {
    let env = TestEnv::blank();
    fs::create_dir_all(env.tmp.path().join("src/api")).expect("src");
    fs::create_dir_all(env.tmp.path().join("tests")).expect("tests");
    fs::write(
        env.tmp.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\n",
    )
    .expect("cargo");
    fs::write(
        env.tmp.path().join("tests/api_rate.rs"),
        "// rate limit tests\n",
    )
    .expect("test");
    fs::write(env.tmp.path().join(".env.example"), "API_RATE_LIMIT=100\n").expect("env");
    assert!(env
        .run(&["init", "--profile", "full", "--format", "json"])
        .status
        .success());

    let scan = env.run(&[
        "brownfield",
        "scan",
        "--domain",
        "api",
        "--query",
        "rate limit",
        "--format",
        "json",
    ]);
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&scan.stdout).unwrap();
    assert_eq!(report["domain"], "api");
    assert!(!report["signals"].as_array().unwrap().is_empty());

    // CI runners ship without `herdr` on PATH; doctor gates
    // `report.ok` on the herdr shape check, so the bare
    // `env.run(["doctor", ...])` would exit non-zero under CI
    // even though the brownfield detection contract holds.
    // Stub a herdr that satisfies the `which_herdr` +
    // `agent start --help` / `pane split --help` shape probes
    // so the test stays self-contained.
    let path = crate::common::fake_herdr::install_fake_herdr_for_doctor(&env);
    let doctor = env.run_with_env(
        &[("PATH", &path)],
        &["doctor", "--project", "--format", "json"],
    );
    assert!(doctor.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(doc["detected"]["brownfield_likely"], true);
}

#[test]
fn delta_merge_on_milestone_complete() {
    let env = TestEnv::new();

    let init_spec = env.run(&["specs", "init", "api", "--format", "json"]);
    assert!(init_spec.status.success());

    let spec_path = env.tmp.path().join("master-plan/specs/api.json");
    let mut spec_content = fs::read_to_string(&spec_path).expect("spec");
    let mut spec: serde_json::Value = serde_json::from_str(&spec_content).expect("spec json");
    spec["requirements"] = serde_json::json!([{
        "id": "REQ-01",
        "statement": "All routes require auth unless public.",
        "scenarios": ["SC-01"],
    }]);
    spec["scenarios"] = serde_json::json!([{
        "id": "SC-01",
        "title": "Protected route",
        "priority": "P1",
        "given": "No token",
        "when": "GET /api/user",
        "then": "401",
    }]);
    spec_content = format!("{}\n", serde_json::to_string_pretty(&spec).unwrap());
    fs::write(&spec_path, spec_content).expect("write spec");

    let milestone = serde_json::json!({
        "milestone": {
            "id": "04",
            "title": "Rate limit API",
            "slug": "rate-limit",
            "spec_status": "implemented",
            "execution_status": "in-progress",
            "depends_on": [],
            "effort": "M",
            "risk": "low",
            "change_kind": "delta",
            "priority": "normal",
            "created": "2026-06-17",
            "updated": "2026-06-17",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "Add rate limiting to API routes." },
        "problem": { "description": "No rate limits today." },
        "scope": { "in_scope": ["Rate limit middleware"], "out_of_scope": ["Billing limits", "Per-user quotas"] },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "Rate limit enforced",
            "verification": "manual: tests pass",
            "status": "passed",
            "evidence": "test",
        }],
        "verification": { "date": "", "branch": "", "evidence": "" },
        "delta": {
            "domain": "api",
            "base_version": 1,
            "added": [{
                "id": "REQ-02",
                "statement": "API returns 429 when rate limit exceeded.",
                "scenarios": ["SC-01"],
            }],
            "modified": [{
                "target": "REQ-01",
                "before": "All routes require auth unless public.",
                "after": "All routes require auth unless public; rate limits apply per IP.",
            }],
        },
    });
    fs::write(
        env.tmp
            .path()
            .join("master-plan/milestones/04-rate-limit.json"),
        format!("{}\n", serde_json::to_string_pretty(&milestone).unwrap()),
    )
    .expect("milestone");

    let complete = env.run(&[
        "milestone",
        "complete",
        "04",
        "--evidence",
        "merged",
        "--format",
        "json",
    ]);
    assert!(
        complete.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&complete.stdout),
        String::from_utf8_lossy(&complete.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&complete.stdout).unwrap();
    assert_eq!(out["domain"]["version"], 2);

    let show = env.run(&["specs", "show", "api", "--format", "json"]);
    let spec: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(spec["domain"]["version"], 2);
    let reqs = spec["requirements"].as_array().unwrap();
    assert_eq!(reqs.len(), 2);
    assert!(reqs
        .iter()
        .any(|r| r["id"] == "REQ-02" && r["statement"].as_str().unwrap().contains("429")));
    assert!(reqs
        .iter()
        .any(|r| r["id"] == "REQ-01" && r["statement"].as_str().unwrap().contains("rate limits")));
}

#[test]
fn delta_g13_blocks_complete_on_version_mismatch() {
    let env = TestEnv::new();

    assert!(env
        .run(&["specs", "init", "api", "--format", "json"])
        .status
        .success());

    let spec_path = env.tmp.path().join("master-plan/specs/api.json");
    let content = fs::read_to_string(&spec_path).expect("spec");
    fs::write(
        &spec_path,
        content.replace("\"version\": 1", "\"version\": 2"),
    )
    .expect("bump");

    let milestone = serde_json::json!({
        "milestone": {
            "id": "04",
            "title": "Stale delta",
            "slug": "stale",
            "spec_status": "implemented",
            "execution_status": "in-progress",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "delta",
            "priority": "normal",
            "created": "2026-06-17",
            "updated": "2026-06-17",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "Change something." },
        "problem": { "description": "Stale base version." },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "ok",
            "verification": "test",
            "status": "passed",
            "evidence": "",
        }],
        "verification": { "date": "", "branch": "", "evidence": "" },
        "delta": {
            "domain": "api",
            "base_version": 1,
            "added": [{
                "id": "REQ-01",
                "statement": "New requirement.",
            }],
        },
    });
    fs::write(
        env.tmp.path().join("master-plan/milestones/04-stale.json"),
        format!("{}\n", serde_json::to_string_pretty(&milestone).unwrap()),
    )
    .expect("milestone");

    let complete = env.run(&["milestone", "complete", "04", "--format", "json"]);
    assert!(!complete.status.success());
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&complete.stdout),
        String::from_utf8_lossy(&complete.stderr)
    );
    assert!(output.contains("G13"));
}
