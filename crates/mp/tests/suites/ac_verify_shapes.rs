//! M106 AC-02 (S6): snapshot test for AC and step verifier envelope shapes.
//!
//! After the WP2 unification (private `run_one` runner shared between AC
//! and step verifiers), the externally observable JSON wire format must be
//! byte-identical to the pre-refactor shape. This test pins that.
//!
//! On first run (no golden file present), or with
//! `AC_VERIFY_SHAPES_UPDATE_GOLDEN=1`, the golden is written and the test
//! passes with a notice. Subsequent runs assert byte-equality against the
//! committed golden.
//!
//! **M109 (C-5): CI MUST NOT set `AC_VERIFY_SHAPES_UPDATE_GOLDEN`.** Setting
//! the env var in CI silently rewrites the golden whenever the assertion
//! fails, defeating the regression purpose of this test. To regenerate the
//! golden deliberately, run locally with the env var set, commit the diff,
//! and unset it before pushing. See `.github/workflows/plan.yml` and any
//! local CI scripts for confirmation that the env var is never set in
//! automated environments.

use mp::ac_verify::{verify_milestone_in, verify_step_tests_in};
use mp_model::{AcceptanceCriterion, MilestoneFile, Step, WorkPackage};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .join("tests")
        .join("fixtures")
        .join("ac_verify_shapes.json")
}

fn fixture_milestone() -> MilestoneFile {
    let mut m = MilestoneFile::default();
    m.milestone.id = "01".into();
    m.milestone.slug = "shapes-fixture".into();
    m.milestone.title = "Snapshot fixture".into();
    m.milestone.effort = "S".into();
    m.milestone.risk = "low".into();
    m.milestone.priority = "normal".into();
    m.milestone.created = "2026-07-04".into();
    m.milestone.updated = "2026-07-04".into();
    m.intent.outcome = "shape-stable".into();
    m.problem.description = "Shape stability".into();
    m.scope.in_scope = vec!["x".into()];
    m.scope.out_of_scope = vec!["a".into(), "b".into()];

    m.acceptance_criteria = vec![
        AcceptanceCriterion {
            id: "AC-PASS".into(),
            description: "passes".into(),
            verification: "echo ac-pass".into(),
            evidence: String::new(),
            status: "pending".into(),
        },
        AcceptanceCriterion {
            id: "AC-FAIL".into(),
            description: "exits non-zero".into(),
            verification: "exit 7".into(),
            evidence: String::new(),
            status: "pending".into(),
        },
        AcceptanceCriterion {
            id: "AC-EMPTY".into(),
            description: "no verification".into(),
            verification: String::new(),
            evidence: String::new(),
            status: "pending".into(),
        },
        AcceptanceCriterion {
            id: "AC-MANUAL".into(),
            description: "prose only".into(),
            verification: "manual: kept manual".into(),
            evidence: String::new(),
            status: "pending".into(),
        },
    ];

    m.work_packages = vec![WorkPackage {
        id: "WP1".into(),
        name: "WP1".into(),
        goal: "do".into(),
        rollback: "n/a".into(),
        steps: vec![],
    }];

    m.steps = vec![
        Step {
            id: "S-PASS".into(),
            action: "passes".into(),
            covers_ac: vec![],
            depends_on_steps: vec![],
            done_when: "done".into(),
            files: vec![],
            tests: "echo step-pass".into(),
            work_package: "WP1".into(),
            status: "pending".into(),
            claimed_at: String::new(),
            claimed_by: String::new(),
            lease_expires_at: String::new(),
            evidence: String::new(),
            order: 1,
        },
        Step {
            id: "S-FAIL".into(),
            action: "fails".into(),
            covers_ac: vec![],
            depends_on_steps: vec![],
            done_when: "fails".into(),
            files: vec![],
            tests: "exit 9".into(),
            work_package: "WP1".into(),
            status: "pending".into(),
            claimed_at: String::new(),
            claimed_by: String::new(),
            lease_expires_at: String::new(),
            evidence: String::new(),
            order: 2,
        },
        Step {
            id: "S-EMPTY".into(),
            action: "empty".into(),
            covers_ac: vec![],
            depends_on_steps: vec![],
            done_when: "".into(),
            files: vec![],
            tests: String::new(),
            work_package: "WP1".into(),
            status: "pending".into(),
            claimed_at: String::new(),
            claimed_by: String::new(),
            lease_expires_at: String::new(),
            evidence: String::new(),
            order: 3,
        },
        Step {
            id: "S-MANUAL".into(),
            action: "manual".into(),
            covers_ac: vec![],
            depends_on_steps: vec![],
            done_when: "".into(),
            files: vec![],
            tests: "manual: kept manual".into(),
            work_package: "WP1".into(),
            status: "pending".into(),
            claimed_at: String::new(),
            claimed_by: String::new(),
            lease_expires_at: String::new(),
            evidence: String::new(),
            order: 4,
        },
    ];
    m
}

fn build_snapshot() -> serde_json::Value {
    let m = fixture_milestone();
    // M107 (S3): the verify_*_in entry points now take a cancellation
    // flag and a child-pid registry. This test doesn't exercise
    // cancellation (it's a snapshot test of envelope shape); pass
    // default-constructed stubs that nothing observes.
    let cancelled = Arc::new(AtomicBool::new(false));
    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let ac_report = verify_milestone_in(&m, None, &cancelled, &child_pids, None);
    let step_report = verify_step_tests_in(&m, None, &cancelled, &child_pids, None);
    serde_json::json!({
        "ac_report": ac_report,
        "step_report": step_report,
        "schema_note": "Envelope shapes are byte-identical to pre-WP2 refactor. If this snapshot changes, ANY field rename or ordering change in AcResult / StepTestsResult / VerifyReport / StepTestsReport is a breaking wire-format change for downstream JSON consumers (rauls review-tests payload, mp milestone complete payload). Update the golden deliberately and review callers."
    })
}

#[test]
fn envelope_shapes_are_stable_against_golden() {
    let snapshot = build_snapshot();
    let serialized = serde_json::to_string_pretty(&snapshot).expect("serialize snapshot");
    let path = golden_path();

    let update = std::env::var_os("AC_VERIFY_SHAPES_UPDATE_GOLDEN").is_some();

    if !path.exists() || update {
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir fixtures");
        std::fs::write(&path, &serialized).expect("write golden");
        eprintln!(
            "Wrote golden ({} bytes) to {}",
            serialized.len(),
            path.display()
        );
        return;
    }

    let existing = std::fs::read_to_string(&path).expect("read golden");
    if existing.trim() != serialized.trim() {
        let diff_start = existing
            .chars()
            .zip(serialized.chars())
            .position(|(a, b)| a != b)
            .unwrap_or(existing.len().min(serialized.len()));
        let preview_end = diff_start.saturating_add(120);
        panic!(
            "Envelope shapes drifted from golden.\n\
             First diff at char {diff_start}.\n\
             Existing [...{prev_end}]: {prev:?}\n\
             Current  [...{cur_end}]: {cur:?}\n\
             To update intentionally: AC_VERIFY_SHAPES_UPDATE_GOLDEN=1 cargo test -p mp --test ac_verify_shapes",
            prev_end = preview_end.min(existing.len()),
            cur_end = preview_end.min(serialized.len()),
            prev = &existing[diff_start.saturating_sub(40).min(existing.len())..preview_end.min(existing.len())],
            cur = &serialized[diff_start.saturating_sub(40).min(serialized.len())..preview_end.min(serialized.len())],
        );
    }
}
