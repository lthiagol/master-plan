//! Consolidated `mp milestone create` test matrix (B-47 / M114 S3). Pre-consolidation
//! this lived in `milestone_create.rs`, `milestone_create_example.rs`, and
//! `milestone_create_stdin.rs` — three suites covering the same code path with
//! minor input variation. Folded into one parameterized matrix so additions
//! land in one place. Use a home-grown matrix macro to keep zero new deps
//! (the project's existing M114 AC-03 contract pins this).
//!
//! Coverage:
//!   * happy-path round-trip with all structured fields (`--json` arg)
//!   * example-template (`--example` flag prints a JSON scaffold)
//!   * stdin-input (`--json @-` reads body from stdin)
//!   * minimal-fields happy path
//!
//! All three inputs land in the same `milestone::create_milestone` codepath
//! (see `commands/milestone.rs::MilestoneCmd::Create`), so a single matrix
//! covers the same code without overlap.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::common::{lib_api, mp_bin, repo_root, TestEnv};

fn pipe_stdin(env: &TestEnv, args: &[&str], stdin_data: &str) -> std::process::Output {
    let child = Command::new(mp_bin())
        .current_dir(env.tmp.path())
        .env("MP_HOME", repo_root())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mp");
    if let Err(error) = child
        .stdin
        .as_ref()
        .unwrap()
        .write_all(stdin_data.as_bytes())
    {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe,
            "write stdin: {error}"
        );
    }
    child.wait_with_output().expect("wait")
}

#[test]
fn milestone_create_and_approve_round_trip() {
    let env = TestEnv::new();

    let create_json = r#"{
        "title": "OAuth Login",
        "depends_on": [],
        "effort": "M",
        "risk": "med",
        "intent": { "outcome": "User can sign in with OAuth." },
        "problem": { "description": "Auth required." },
        "scope": {
            "in_scope": ["OAuth"],
            "out_of_scope": ["Password login", "SAML"]
        },
        "acceptance_criteria": [
            {
                "description": "OAuth flow completes",
                "verification": "cargo test oauth"
            }
        ]
    }"#;

    let create = lib_api::run(
        &env,
        &[
            "milestone",
            "create",
            "--json",
            create_json,
            "--format",
            "json",
        ],
    );
    assert!(
        create.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).expect("create json");
    assert_eq!(created["ok"], true);
    let id = created["milestone"]["id"].as_str().expect("milestone id");

    let show = lib_api::run(&env, &["show", "milestone", id, "--format", "json"]);
    assert!(show.status.success());
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).expect("show json");
    assert_eq!(shown["milestone"]["spec_status"], "draft");

    let approve = lib_api::run(&env, &["milestone", "approve", id, "--format", "json"]);
    assert!(
        approve.status.success(),
        "{}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let approved: serde_json::Value =
        serde_json::from_slice(&approve.stdout).expect("approve json");
    assert_eq!(approved["milestone"]["spec_status"], "ready");

    let milestone_dir = env.tmp.path().join("master-plan/milestones");
    let files: Vec<_> = std::fs::read_dir(&milestone_dir)
        .expect("milestones dir")
        .map(|e| e.expect("entry").path())
        .collect();
    assert_eq!(files.len(), 1);
    let raw = std::fs::read_to_string(&files[0]).expect("milestone file");
    assert!(!raw.contains("[[work_packages.steps]]"));
    for marker in [
        "[behavior]",
        "[context]",
        "[requirements]",
        "success_criteria = []",
    ] {
        assert!(
            !raw.contains(marker),
            "new milestone file must not scaffold ceremony section {marker}"
        );
    }
}

#[test]
fn create_example_emits_valid_template() {
    let env = TestEnv::new();
    let out = lib_api::run(
        &env,
        &["milestone", "create", "--example", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "example failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !json["title"].as_str().unwrap_or("").is_empty(),
        "example should have a title"
    );
    assert!(
        !json["intent"]["outcome"].as_str().unwrap_or("").is_empty(),
        "example should have intent.outcome"
    );
    assert!(
        !json["problem"]["description"]
            .as_str()
            .unwrap_or("")
            .is_empty(),
        "example should have problem.description"
    );
    assert!(
        !json["scope"]["in_scope"].as_array().unwrap().is_empty(),
        "example should have in_scope"
    );
    assert!(
        !json["scope"]["out_of_scope"].as_array().unwrap().is_empty(),
        "example should have out_of_scope"
    );
    assert!(
        !json["acceptance_criteria"].as_array().unwrap().is_empty(),
        "example should have acceptance_criteria"
    );
}

#[test]
fn milestone_create_stdin_populates_structured_fields() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "Auth Module",
        "intent": { "outcome": "Users can log in with OAuth" },
        "problem": { "description": "No auth system exists" },
        "scope": {
            "in_scope": ["OAuth login", "Session management"],
            "out_of_scope": ["Password reset", "MFA"]
        },
        "acceptance_criteria": [
            { "description": "OAuth redirect works", "verification": "cargo test oauth" },
            { "description": "Session persists", "verification": "cargo test session" }
        ]
    }"#;

    let out = pipe_stdin(
        &env,
        &["milestone", "create", "--json", "@-", "--format", "json"],
        json,
    );
    assert!(
        out.status.success(),
        "stdin create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let milestone = &result["milestone"];
    assert_eq!(milestone["title"], "Auth Module");

    // Verify structured fields persisted
    let id = milestone["id"].as_str().unwrap();
    let show = lib_api::run(&env, &["show", "milestone", id, "--format", "json"]);
    assert!(show.status.success());
    let show_json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        show_json["intent"]["outcome"],
        "Users can log in with OAuth"
    );
    assert_eq!(show_json["problem"]["description"], "No auth system exists");
    assert_eq!(show_json["scope"]["in_scope"][0], "OAuth login");
    assert_eq!(
        show_json["acceptance_criteria"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn milestone_create_stdin_with_minimal_fields() {
    let env = TestEnv::new();
    let json = r#"{
        "title": "Minimal Milestone",
        "intent": { "outcome": "A minimal milestone" },
        "problem": { "description": "Testing minimal fields" },
        "scope": {
            "in_scope": ["Something"],
            "out_of_scope": ["Nothing else", "TBD"]
        },
        "acceptance_criteria": [
            { "description": "Minimal works", "verification": "echo ok" }
        ]
    }"#;

    let out = pipe_stdin(
        &env,
        &["milestone", "create", "--json", "@-", "--format", "json"],
        json,
    );
    assert!(
        out.status.success(),
        "minimal stdin create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["milestone"]["title"], "Minimal Milestone");
}

fn minimal_create_json(extra: &str) -> String {
    format!(
        r#"{{
            "title": "Input parity {extra}",
            "intent": {{ "outcome": "All forms agree" }},
            "problem": {{ "description": "Input paths used to diverge" }},
            "scope": {{
                "in_scope": ["parity"],
                "out_of_scope": ["other commands", "other formats"]
            }},
            "acceptance_criteria": [
                {{ "description": "works", "verification": "echo ok" }}
            ]
        }}"#
    )
}

#[test]
fn milestone_create_valid_payload_has_parity_across_all_input_forms() {
    let env = TestEnv::new();
    for (index, form) in ["inline", "at-file", "file", "stdin"]
        .into_iter()
        .enumerate()
    {
        let payload = minimal_create_json(&format!("{index}"));
        let path = env.tmp.path().join(format!("{form}.json"));
        std::fs::write(&path, &payload).unwrap();
        let output = match form {
            "inline" => lib_api::run(
                &env,
                &[
                    "milestone",
                    "create",
                    "--json",
                    &payload,
                    "--format",
                    "json",
                ],
            ),
            "at-file" => lib_api::run(
                &env,
                &[
                    "milestone",
                    "create",
                    "--json",
                    &format!("@{}", path.display()),
                    "--format",
                    "json",
                ],
            ),
            "file" => lib_api::run(
                &env,
                &[
                    "milestone",
                    "create",
                    "--file",
                    path.to_str().unwrap(),
                    "--format",
                    "json",
                ],
            ),
            "stdin" => pipe_stdin(
                &env,
                &["milestone", "create", "--json", "@-", "--format", "json"],
                &payload,
            ),
            _ => unreachable!(),
        };
        assert!(
            output.status.success(),
            "{form} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn milestone_create_unknown_keys_fail_identically_across_all_input_forms() {
    let env = TestEnv::new();
    let payload = r#"{"title":"bad","unknown_key":true}"#;
    let path = env.tmp.path().join("invalid.json");
    std::fs::write(&path, payload).unwrap();
    let outputs = [
        lib_api::run(
            &env,
            &["milestone", "create", "--json", payload, "--format", "json"],
        ),
        lib_api::run(
            &env,
            &[
                "milestone",
                "create",
                "--json",
                &format!("@{}", path.display()),
                "--format",
                "json",
            ],
        ),
        lib_api::run(
            &env,
            &[
                "milestone",
                "create",
                "--file",
                path.to_str().unwrap(),
                "--format",
                "json",
            ],
        ),
        pipe_stdin(
            &env,
            &["milestone", "create", "--json", "@-", "--format", "json"],
            payload,
        ),
    ];
    let mut messages = Vec::new();
    for output in outputs {
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("unknown_key"), "{stderr}");
        messages.push(stderr.replace(&path.display().to_string(), "<path>"));
    }
    assert!(
        messages.windows(2).all(|pair| pair[0] == pair[1]),
        "input forms must report identical key-validation errors: {messages:#?}"
    );
}

#[test]
fn milestone_create_all_payload_sources_enforce_size_and_containment() {
    let oversized = " ".repeat(mp::json_input::MAX_JSON_INPUT_BYTES as usize + 1);
    let error = mp::milestone::read_create_input(None, None, Some(&oversized))
        .unwrap_err()
        .to_string();
    assert!(error.contains("inline JSON exceeds"), "{error}");

    let env = TestEnv::new();
    let stdin = pipe_stdin(
        &env,
        &["milestone", "create", "--json", "@-", "--format", "json"],
        &oversized,
    );
    assert!(!stdin.status.success());
    assert!(
        String::from_utf8_lossy(&stdin.stderr).contains("standard input exceeds"),
        "{}",
        String::from_utf8_lossy(&stdin.stderr)
    );

    let outside = tempfile::TempDir::new().unwrap();
    let outside_path = outside.path().join("outside.json");
    std::fs::write(&outside_path, minimal_create_json("outside")).unwrap();
    for output in [
        lib_api::run(
            &env,
            &[
                "milestone",
                "create",
                "--json",
                &format!("@{}", outside_path.display()),
                "--format",
                "json",
            ],
        ),
        lib_api::run(
            &env,
            &[
                "milestone",
                "create",
                "--file",
                outside_path.to_str().unwrap(),
                "--format",
                "json",
            ],
        ),
    ] {
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("escapes project root"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
