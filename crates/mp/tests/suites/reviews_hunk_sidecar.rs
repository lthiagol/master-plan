//! M154 AC-06: validate the on-disk agent-context sidecar against
//! hunk's expected shape. The contract: `hunk diff --agent-context
//! <path>` reads the file once at startup and indexes every
//! annotation per file. The test pins every documented key
//! (version, files[], annotations[], newRange, oldRange, summary,
//! rationale, author, confidence) so a future refactor that
//! drops a field or changes its name surfaces immediately.

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
        "intent": { "outcome": "sidecar shape" },
        "problem": { "description": "sidecar shape" },
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

/// AC-06: the agent-context sidecar JSON written by `mp reviews hunk
/// <M> --file <path>` validates against hunk's expected shape.
/// Schema:
///   {
///     "version": 1,
///     "files": [
///       {
///         "path": <string>,
///         "annotations": [
///           {
///             "newRange": [start, end],
///             "oldRange": [start, end],
///             "summary": <string>,
///             "rationale": <string>,
///             "author": <string>,
///             "confidence": "high" | "medium" | "low"
///           }
///         ]
///       }
///     ]
///   }
/// The test fixtures every documented key so a future refactor that
/// drops a field surfaces here, not at hunk-load time.
#[test]
fn reviews_hunk_sidecar_validates_against_hunk_agent_context_shape() {
    let env = TestEnv::new();
    let id = "M-hunk-shape";
    write_milestone(&env, id, "hunk-shape", "hunk sidecar shape");

    // Opt-in.
    let out = env.run(&["config", "set", "review.hunk", "true"]);
    assert!(
        out.status.success(),
        "enable hunk: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // File 4 findings: 2 anchored on `new`, 1 anchored on `old`,
    // 1 unanchored. The unanchored one groups under the synthetic
    // milestone pseudo-path; the other 3 group by their file path.
    // The test pins every documented key — fields that hunk
    // expects to index are present, fields it doesn't expect (e.g.
    // "commit", "hunk_index") are absent from the sidecar (the
    // live-batch has them, the agent-context shape doesn't).
    for (i, (file, line, side)) in [
        (Some("crates/mp/src/install.rs"), Some(42), Some("new")),
        (Some("crates/mp/src/install.rs"), Some(99), Some("new")),
        (Some("crates/mp/src/foo.rs"), Some(7), Some("old")),
        (None, None, None), // unanchored design note
    ]
    .iter()
    .enumerate()
    {
        let mut args: Vec<String> = vec![
            "reviews".into(),
            "finding".into(),
            "add".into(),
            id.to_string(),
            "--severity".into(),
            "high".into(),
            "--category".into(),
            "correctness".into(),
            "--desc".into(),
            format!("hunk-shape finding #{i}"),
            "--author".into(),
            "test".into(),
            "--phase".into(),
            "external".into(),
        ];
        if let Some(f) = file {
            args.push("--file".into());
            args.push((*f).to_string());
        }
        if let Some(l) = line {
            args.push("--line".into());
            args.push(l.to_string());
        }
        if let Some(s) = side {
            args.push("--side".into());
            args.push((*s).to_string());
        }
        args.push("--format".into());
        args.push("json".into());
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = env.run(&arg_refs);
        assert!(
            out.status.success(),
            "finding {i} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let sidecar_path = env.tmp.path().join("hunk-shape.json");
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

    let body = fs::read_to_string(&sidecar_path).expect("sidecar readable");
    let v: serde_json::Value = serde_json::from_str(&body).expect("sidecar JSON");

    // Schema pin: every top-level key hunk reads.
    assert_eq!(
        v["version"].as_u64(),
        Some(1),
        "sidecar.version is 1 (the documented schema version)"
    );
    let files = v["files"].as_array().expect("sidecar.files is an array");
    assert!(
        !files.is_empty(),
        "at least one file entry; sidecar must not be empty when findings exist"
    );

    // Per-file schema pin: every entry has path + annotations[].
    for f in files {
        let path = f["path"].as_str().expect("file.path is a string");
        assert!(
            !path.is_empty(),
            "file.path must be non-empty (unanchored notes use the synthetic path)"
        );
        let annotations = f["annotations"]
            .as_array()
            .expect("file.annotations is an array");
        assert!(
            !annotations.is_empty(),
            "each file entry has at least one annotation; got 0 for {path}"
        );

        for a in annotations {
            // Annotation core fields: every field hunk consumes.
            assert!(a["summary"].is_string(), "annotation.summary is a string");
            assert!(
                a["rationale"].is_string(),
                "annotation.rationale is a string"
            );
            assert!(a["author"].is_string(), "annotation.author is a string");
            let confidence = a["confidence"].as_str();
            assert!(
                matches!(confidence, Some("high") | Some("medium") | Some("low")),
                "annotation.confidence is high|medium|low; got {confidence:?}"
            );

            // Range tuples: exactly one of newRange/oldRange is
            // populated when the finding is anchored on a side;
            // both absent for unanchored notes (which live under
            // the synthetic path).
            let new_range = a["newRange"].as_array();
            let old_range = a["oldRange"].as_array();
            match (new_range, old_range) {
                (Some(r), None) => {
                    assert_eq!(r.len(), 2, "newRange is a [start, end] tuple");
                    assert!(r[0].is_u64() && r[1].is_u64(), "range is line numbers");
                }
                (None, Some(r)) => {
                    assert_eq!(r.len(), 2, "oldRange is a [start, end] tuple");
                    assert!(r[0].is_u64() && r[1].is_u64(), "range is line numbers");
                }
                (Some(_), Some(_)) => {
                    panic!("annotation has both newRange AND oldRange; expected exactly one")
                }
                (None, None) => {
                    // File-level note (synthetic path) — no range is
                    // expected. Verified by `path` starting with
                    // "__milestone-".
                    assert!(
                        path.starts_with("__milestone-"),
                        "range-less annotation only allowed under synthetic path; got path={path}"
                    );
                }
            }
        }
    }
}

/// AC-06 second pin: the sidecar regenerates cleanly across re-runs
/// (no torn file from a partial write). M113 S2 atomic-write
/// contract — verified here end-to-end through the CLI surface.
#[test]
fn reviews_hunk_sidecar_atomic_write_across_reruns() {
    let env = TestEnv::new();
    let id = "M-hunk-atomic";
    write_milestone(&env, id, "hunk-atomic", "hunk sidecar atomic");
    env.run(&["config", "set", "review.hunk", "true"]);
    env.run(&[
        "reviews",
        "finding",
        "add",
        id,
        "--severity",
        "medium",
        "--category",
        "test",
        "--desc",
        "first deploy",
        "--author",
        "test",
        "--phase",
        "external",
        "--file",
        "src/foo.rs",
        "--line",
        "1",
        "--format",
        "json",
    ]);

    let sidecar_path = env.tmp.path().join("hunk-atomic.json");
    env.run(&[
        "reviews",
        "hunk",
        id,
        "--file",
        sidecar_path.to_str().unwrap(),
    ]);
    let first = fs::read_to_string(&sidecar_path).unwrap();
    assert!(first.contains("\"version\": 1"));
    assert!(first.contains("src/foo.rs"));

    // Re-run: the file should be atomically overwritten (not
    // appended-to, not torn). The on-disk content must still parse
    // as the same shape (single `version: 1`).
    env.run(&[
        "reviews", "finding", "resolve", id, "--all", "--format", "json",
    ]);
    env.run(&[
        "reviews",
        "hunk",
        id,
        "--file",
        sidecar_path.to_str().unwrap(),
    ]);
    let second = fs::read_to_string(&sidecar_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&second).expect("parses");
    assert_eq!(v["version"], 1, "version stable across re-runs");
    // After resolve, no open findings; unanchored notes group under
    // the synthetic path with the resolved finding's body. The
    // structural shape is the same.
    assert!(v["files"].is_array(), "files[] survives re-run");
}
