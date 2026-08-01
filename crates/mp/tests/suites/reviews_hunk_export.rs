//! M154 AC-03 + AC-04 + AC-06: `mp reviews hunk <M>` round-trip.
//!
//! Three contracts pinned here:
//! - Live batch (default stdout channel) emits the documented hunk
//!   `comment apply` shape: `{comments: [{filePath, newLine, summary,
//!   rationale, author}]}`.
//! - Sidecar (`--file <path>`) emits the agent-context shape:
//!   `{version, files: [{path, annotations: [{newRange, oldRange,
//!   summary, rationale, author, confidence}]}]}` with `version: 1`.
//! - Apply fallback (no live hunk session) prints the batch + a hint
//!   and exits 0 (per AC-04).

use std::fs;

use crate::common::TestEnv;

fn write_milestone(env: &TestEnv, id: &str, slug: &str, title: &str) {
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let milestone = serde_json::json!({
        "milestone": {
            "id": id,
            "title": title,
            "slug": slug,
            "spec_status": "ready",
            "execution_status": "planned",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "priority": "normal",
            "created": "2026-07-15",
            "updated": "2026-07-15",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "hunk export smoke" },
        "problem": { "description": "hunk export smoke" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "x",
            "verification": "manual: test",
            "status": "pending",
            "evidence": "",
            "covers_ac": [],
            "claimed_by": "",
            "claimed_at": "",
            "lease_expires_at": ""
        }],
        "design_decisions": [],
        "findings": [],
        "intent_outcome": "x",
        "open_questions": [],
        "comments": [],
        "steps": [],
        "verification": {"date": "", "branch": "", "evidence": ""},
        "comments_v2": []
    });
    fs::write(
        dir.join(format!("{id}-{slug}.json")),
        format!("{}\n", serde_json::to_string_pretty(&milestone).unwrap()),
    )
    .unwrap();
}

fn enable_review_hunk(env: &TestEnv) {
    let out = env.run(&["config", "set", "review.hunk", "true"]);
    assert!(
        out.status.success(),
        "enable review.hunk: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// AC-03: with `[review] hunk = true`, `mp reviews hunk <M>` emits
/// the live batch on stdout. The batch's `comments[]` carries the
/// documented shape: `filePath`, `newLine`, `summary`, `rationale`,
/// `author`. The export is gated on the config flag — a project
/// without the flag gets a clear error instead of silent output.
#[test]
fn reviews_hunk_emits_live_batch_when_config_flag_set() {
    let env = TestEnv::new();
    let id = "M-hunk-batch";
    write_milestone(&env, id, "hunk-batch", "hunk batch");

    // 1. Default-off: a `hunk` invocation errors with the
    //    "set review.hunk = true" hint (NOT a silent zero-comment
    //    batch — that would mask a misconfigured project).
    let out = env.run(&["reviews", "hunk", id, "--format", "json"]);
    assert!(
        !out.status.success(),
        "hunk must error when review.hunk is off"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("review.hunk = false") || stderr.contains("set `review.hunk = true`"),
        "expected gate message; got: {stderr}"
    );

    // 2. Opt-in: enable the flag, file a finding with --file/--line,
    //    re-run. The batch surfaces the finding's comment entry.
    enable_review_hunk(&env);
    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        id,
        "--severity",
        "high",
        "--category",
        "correctness",
        "--desc",
        "long description that becomes the live batch summary",
        "--author",
        "test",
        "--phase",
        "external",
        "--file",
        "crates/mp/src/install.rs",
        "--line",
        "42",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "finding add: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = env.run(&["reviews", "hunk", id, "--format", "json"]);
    assert!(
        out.status.success(),
        "hunk live-batch: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let comments = v["comments"]
        .as_array()
        .expect("live batch carries {comments: [...]}");
    assert_eq!(comments.len(), 1, "one anchored finding");
    let c = &comments[0];
    assert_eq!(c["filePath"], "crates/mp/src/install.rs");
    assert_eq!(c["newLine"], 42);
    assert_eq!(
        c["summary"],
        "long description that becomes the live batch summary"
    );
    assert_eq!(c["author"], "mp", "default hunk_author applied");
}

/// AC-03 round-trip: when both `--anchor` and `--file` are present
/// on a finding, `--anchor` wins (the explicit positional form is
/// canonical per the comment in cmd_finding). Pinned here so a
/// future refactor that swaps the precedence is caught by the test.
#[test]
fn reviews_hunk_anchor_flag_takes_precedence_over_file() {
    let env = TestEnv::new();
    let id = "M-hunk-prec";
    write_milestone(&env, id, "hunk-prec", "hunk precedence");
    enable_review_hunk(&env);

    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        id,
        "--severity",
        "low",
        "--category",
        "test",
        "--desc",
        "anchor-vs-file precedence",
        "--author",
        "test",
        "--phase",
        "external",
        // anchor format: path:commit:new_range:old_range:hunk_index:side
        "--anchor",
        "crates/mp/src/explicit.rs::10-10::0:new",
        "--file",
        "crates/mp/src/file-flag.rs",
        "--line",
        "1",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "finding add: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = env.run(&["reviews", "hunk", id, "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let c = &v["comments"][0];
    assert_eq!(
        c["filePath"], "crates/mp/src/explicit.rs",
        "--anchor wins over --file"
    );
    assert_eq!(
        c["newLine"], 10,
        "--anchor line takes precedence over --line 1"
    );
}

/// AC-04: `--apply` is the print-and-hint fallback path when no
/// hunk session is running. The exit code is 0 (per AC-04 — the
/// command must not error when no session is live). The hint names
/// the documented pipe target (`hunk session comment apply --stdin`)
/// so the operator knows the next step.
#[test]
fn reviews_hunk_apply_with_no_live_session_prints_batch_and_hint() {
    let env = TestEnv::new();
    let id = "M-hunk-apply";
    write_milestone(&env, id, "hunk-apply", "hunk apply fallback");
    enable_review_hunk(&env);

    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        id,
        "--severity",
        "medium",
        "--category",
        "correctness",
        "--desc",
        "needs reviewer eyes",
        "--author",
        "test",
        "--phase",
        "external",
        "--file",
        "src/foo.rs",
        "--line",
        "7",
        "--format",
        "json",
    ]);
    assert!(out.status.success());

    let out = env.run(&["reviews", "hunk", id, "--apply", "--format", "json"]);
    // AC-04: with no live session the command prints the batch + hint
    // and exits 0. (Future hunk-side IPC will replace this; the
    // test stays green either way because the fallback is
    // documented.)
    assert!(
        out.status.success(),
        "apply with no live session must exit 0; got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["hint"],
        "no live `hunk session` detected — batch printed to stdout; pipe it to `hunk session comment apply --stdin` once a session is open"
    );
    // AC-04 + external F-07: the batch itself must be present so
    // operators can re-pipe without a second export call. A count-
    // only response lied about "batch printed to stdout".
    let comments = v["comments"]
        .as_array()
        .expect("apply no-session path must include comments[] batch");
    assert_eq!(comments.len(), 1, "one finding in the apply batch");
    assert_eq!(comments[0]["filePath"], "src/foo.rs");
    assert_eq!(comments[0]["newLine"], 7);
    assert_eq!(v["comments_emitted"], 1);
}

/// AC-03: `--strict` drops unanchored entries from the live batch.
/// External F-06: the pre-fix guard was inverted (`strict &&
/// sidecar_path.is_none()` bailed on the only channel where --strict
/// should work). This test pins both the happy path and the
/// `--strict --file` rejection.
#[test]
fn reviews_hunk_strict_filters_unanchored_from_live_batch() {
    let env = TestEnv::new();
    let id = "M-hunk-strict";
    write_milestone(&env, id, "hunk-strict", "hunk strict");
    enable_review_hunk(&env);

    // Anchored finding — survives --strict.
    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        id,
        "--severity",
        "high",
        "--category",
        "correctness",
        "--desc",
        "anchored note",
        "--author",
        "test",
        "--phase",
        "external",
        "--file",
        "src/anchored.rs",
        "--line",
        "3",
        "--format",
        "json",
    ]);
    assert!(out.status.success());

    // Unanchored finding — dropped under --strict.
    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        id,
        "--severity",
        "low",
        "--category",
        "design",
        "--desc",
        "milestone-level note",
        "--author",
        "test",
        "--phase",
        "external",
        "--format",
        "json",
    ]);
    assert!(out.status.success());

    // Without --strict: both entries.
    let out = env.run(&["reviews", "hunk", id, "--format", "json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["comments"].as_array().unwrap().len(),
        2,
        "default live batch keeps unanchored notes"
    );

    // With --strict: only the anchored entry.
    let out = env.run(&["reviews", "hunk", id, "--strict", "--format", "json"]);
    assert!(
        out.status.success(),
        "--strict on live batch must succeed; got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let comments = v["comments"].as_array().expect("comments[]");
    assert_eq!(comments.len(), 1, "--strict drops unanchored");
    assert_eq!(comments[0]["filePath"], "src/anchored.rs");
    assert_eq!(comments[0]["newLine"], 3);

    // --strict + --file is rejected (sidecar has its own unanchored
    // policy; silent no-op would surprise operators).
    let sidecar = env.tmp.path().join("strict-sidecar.json");
    let out = env.run(&[
        "reviews",
        "hunk",
        id,
        "--strict",
        "--file",
        sidecar.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(
        !out.status.success(),
        "--strict --file must error, not silently ignore --strict"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--strict only applies to the live batch"),
        "expected strict+file rejection message; got: {stderr}"
    );
}

/// AC-06: `--file <path>` writes the agent-context sidecar to disk.
/// The on-disk shape is loadable by `hunk diff --agent-context
/// <path>` (validated by the unit tests in `reviews::hunk` against
/// the documented keys). Re-running overwrites cleanly (atomic-write
/// contract per M113 S2).
#[test]
fn reviews_hunk_writes_agent_context_sidecar_to_path() {
    let env = TestEnv::new();
    let id = "M-hunk-sidecar";
    write_milestone(&env, id, "hunk-sidecar", "hunk sidecar");
    enable_review_hunk(&env);

    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        id,
        "--severity",
        "high",
        "--category",
        "correctness",
        "--desc",
        "sidecar entry",
        "--author",
        "test",
        "--phase",
        "external",
        "--file",
        "crates/mp/src/install.rs",
        "--line",
        "99",
        "--format",
        "json",
    ]);
    assert!(out.status.success());

    let sidecar_path = env.tmp.path().join("hunk-agent-context.json");
    let out = env.run(&[
        "reviews",
        "hunk",
        id,
        "--file",
        sidecar_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "hunk --file: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        sidecar_path.exists(),
        "sidecar must be written to {}",
        sidecar_path.display()
    );

    let body = fs::read_to_string(&sidecar_path).expect("sidecar readable");
    let v: serde_json::Value = serde_json::from_str(&body).expect("sidecar JSON");
    assert_eq!(
        v["version"], 1,
        "sidecar carries schema version (per AC-06)"
    );
    let files = v["files"].as_array().expect("files[] in sidecar");
    assert_eq!(files.len(), 1, "one file with the anchored finding");
    let f = &files[0];
    assert_eq!(f["path"], "crates/mp/src/install.rs");
    let annotations = f["annotations"].as_array().expect("annotations[]");
    assert_eq!(annotations.len(), 1);
    let a = &annotations[0];
    assert_eq!(a["newRange"], serde_json::json!([99, 99]));
    assert_eq!(a["summary"], "sidecar entry");
    assert_eq!(a["author"], "mp");
    assert_eq!(a["confidence"], "high");

    // Re-run: the sidecar is overwritten cleanly. No torn file.
    let out = env.run(&[
        "reviews",
        "hunk",
        id,
        "--file",
        sidecar_path.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "re-run overwrites sidecar");
}

/// AC-06: unanchored findings (no --file) surface in the sidecar
/// under a synthetic per-milestone pseudo-path so they don't
/// disappear from the export. The synthetic path is the load-
/// bearing case for design-decision / scope notes that aren't tied
/// to a single line of code.
#[test]
fn reviews_hunk_sidecar_groups_unanchored_under_synthetic_path() {
    let env = TestEnv::new();
    let id = "M-hunk-synth";
    write_milestone(&env, id, "hunk-synth", "hunk synth");
    enable_review_hunk(&env);

    let out = env.run(&[
        "reviews",
        "finding",
        "add",
        id,
        "--severity",
        "low",
        "--category",
        "design",
        "--desc",
        "milestone-level note",
        "--author",
        "test",
        "--phase",
        "external",
        "--format",
        "json",
    ]);
    assert!(out.status.success());

    let sidecar_path = env.tmp.path().join("hunk-synth.json");
    let out = env.run(&[
        "reviews",
        "hunk",
        id,
        "--file",
        sidecar_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(out.status.success());

    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    let files = v["files"].as_array().unwrap();
    assert!(
        files
            .iter()
            .any(|f| f["path"].as_str().unwrap_or("").starts_with("__milestone-")),
        "unanchored notes group under synthetic path; got paths: {:?}",
        files.iter().map(|f| f["path"].as_str()).collect::<Vec<_>>()
    );
}
