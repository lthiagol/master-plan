//! M104 AC-02 (B-44): running `migrate_plan_lifecycle` on a fixture with
//! gate-relevant milestones in VALID legacy shape must NOT change the gate
//! set that `validate` fires. Migration is a relabel, not a behavior change
//! for already-consistent state. (For inconsistent legacy state, the
//! migration NORMALIZES it, which is the intended behavior — see the
//! `normalizes_inconsistent_state_and_quiets_g1` test.)
//!
//! Strategy:
//!   1. Construct a fixture with milestones in valid legacy shape (spec_status
//!      ≥ ready for in-progress, spec_status=verified for done, etc.) whose
//!      gate set fires W44 (done + unreviewed) and a couple of ACs/SCs.
//!   2. Capture the validate-code multiset before migration.
//!   3. Run `migrate_plan_lifecycle`.
//!   4. Capture the validate-code multiset after migration.
//!   5. Assert the two multisets are equal.
//!
//! Note: `mp milestone create` writes to the plan.json index (and the `sync`
//! layer would re-derive the index from the migrated files). We construct the
//! files directly to avoid the unrelated index-sync machinery and isolate the
//! gate-parity invariant.

use crate::common::TestEnv;
use mp::migrate::migrate_plan_lifecycle;
use mp::validate::validate_plan;
use mp_model::{AcceptanceCriterion, MilestoneFile, OpenQuestion, Step, WorkPackage};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn write_milestone(dir: &Path, m: &MilestoneFile) {
    let path = dir.join(format!("{}-{}.json", m.milestone.id, m.milestone.slug));
    let raw = serde_json::to_string_pretty(m).expect("serialize");
    std::fs::write(&path, format!("{raw}\n")).expect("write");
}

fn validate_code_multiset(plan_dir: &Path) -> BTreeMap<String, usize> {
    let ctx = mp::paths::PlanContext::discover(
        Some(plan_dir.to_path_buf()),
        Some(plan_dir.parent().unwrap().to_path_buf()),
    )
    .expect("PlanContext::discover");
    let report = validate_plan(&ctx).expect("validate_plan ok");
    let mut codes: BTreeMap<String, usize> = BTreeMap::new();
    for issue in report.errors.iter().chain(report.warnings.iter()) {
        *codes.entry(issue.code.clone()).or_insert(0) += 1;
    }
    codes
}

/// Build a minimal milestone with the legacy fields populated. Used to
/// construct fixtures in valid legacy shape that produce a specific gate
/// signature across migration.
fn legacy_milestone(
    id: &str,
    slug: &str,
    spec_status: &str,
    execution_status: &str,
    has_implementation_plan: bool,
) -> MilestoneFile {
    let mut m = MilestoneFile::default();
    m.milestone.id = id.to_string();
    m.milestone.slug = slug.to_string();
    m.milestone.title = format!("{id} {slug}");
    m.milestone.lifecycle = String::new(); // not migrated yet
    m.milestone.spec_status = spec_status.to_string();
    m.milestone.execution_status = execution_status.to_string();
    m.milestone.effort = "S".to_string();
    m.milestone.risk = "low".to_string();
    m.milestone.priority = "normal".to_string();
    m.milestone.created = "2026-07-04".to_string();
    m.milestone.updated = "2026-07-04".to_string();
    m.intent.outcome = "outcome".to_string();
    m.problem.description = "problem".to_string();
    m.scope.in_scope = vec!["x".to_string()];
    m.scope.out_of_scope = vec!["a".to_string(), "b".to_string()];
    m.acceptance_criteria = vec![AcceptanceCriterion {
        id: "AC-01".to_string(),
        description: "AC1".to_string(),
        verification: "echo ok".to_string(),
        evidence: "fixture".to_string(),
        status: "passed".to_string(),
    }];
    if has_implementation_plan {
        m.work_packages = vec![WorkPackage {
            id: "WP1".to_string(),
            name: "WP1".to_string(),
            goal: "do the thing".to_string(),
            rollback: "n/a".to_string(),
            steps: vec![Step {
                evidence: String::new(),
                id: "S1".to_string(),
                action: "do".to_string(),
                covers_ac: vec!["AC-01".to_string()],
                depends_on_steps: vec![],
                done_when: "done".to_string(),
                files: vec![],
                tests: "manual: fixture".to_string(),
                work_package: "WP1".to_string(),
                status: "done".to_string(),
                claimed_at: String::new(),
                claimed_by: String::new(),
                lease_expires_at: String::new(),
                order: 1,
            }],
        }];
    }
    m
}

/// Fixture: M1 done + unreviewed (W44), M2 ready with open question (G2),
/// M3 verified complete (no in-progress gate, no G1), M4 in-progress but spec
/// ready (no G1 — valid state). Migration should preserve the gate set.
fn build_valid_parity_fixture(plan_dir: &Path) {
    let m1 = {
        // done + verified → W44 fires (no reviews file in tmp)
        let mut m = legacy_milestone("01", "alpha", "verified", "done", true);
        m.verification.branch = "main".into();
        m.verification.date = "2026-07-04".into();
        m.verification.evidence = "fixture".into();
        m
    };
    let m2 = {
        // ready + open question → G2 fires
        let mut m = legacy_milestone("02", "beta", "ready", "planned", true);
        m.acceptance_criteria.clear();
        m.acceptance_criteria.push(AcceptanceCriterion {
            id: "AC-01".into(),
            description: "AC1".into(),
            verification: "echo ok".into(),
            evidence: "fixture".into(),
            status: "passed".into(),
        });
        m.open_questions.push(OpenQuestion {
            id: "Q-01".into(),
            question: "?".into(),
            answer: String::new(),
            status: "open".into(),
        });
        m
    };
    let m3 = legacy_milestone("03", "gamma", "verified", "done", true);
    let m4 = legacy_milestone("04", "delta", "ready", "in-progress", true);

    std::fs::create_dir_all(plan_dir.join("milestones")).unwrap();
    write_milestone(&plan_dir.join("milestones"), &m1);
    write_milestone(&plan_dir.join("milestones"), &m2);
    write_milestone(&plan_dir.join("milestones"), &m3);
    write_milestone(&plan_dir.join("milestones"), &m4);
}

#[test]
fn validate_gate_set_is_unchanged_after_lifecycle_migration() {
    let env = TestEnv::blank();
    let plan_dir: PathBuf = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    build_valid_parity_fixture(&plan_dir);

    let before = validate_code_multiset(&plan_dir);
    // Sanity: at least W44 (M1 done + unreviewed) and G2 (M2 open question)
    // should fire pre-migration, so the comparison is meaningful.
    assert!(
        before.contains_key("W44"),
        "pre-migration should include W44 (sanity): {before:?}"
    );
    assert!(
        before.contains_key("G2"),
        "pre-migration should include G2 (sanity): {before:?}"
    );

    let report = migrate_plan_lifecycle(&plan_dir).expect("migrate_plan_lifecycle ok");
    assert!(report.migrated >= 4, "4 milestones migrated: {report:?}");
    assert!(
        report.decode_errors.is_empty(),
        "no decode errors: {report:?}"
    );

    // Confirm lifecycle is populated and legacy fields cleared everywhere.
    let migrated: Vec<MilestoneFile> = std::fs::read_dir(plan_dir.join("milestones"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| {
            let raw = std::fs::read_to_string(e.path()).unwrap();
            serde_json::from_str::<MilestoneFile>(&raw).unwrap()
        })
        .collect();
    assert_eq!(migrated.len(), 4);
    for m in &migrated {
        assert!(
            m.milestone.spec_status.is_empty(),
            "{}: spec_status must be cleared",
            m.milestone.id
        );
        assert!(
            m.milestone.execution_status.is_empty(),
            "{}: execution_status must be cleared",
            m.milestone.id
        );
        assert!(
            !m.milestone.lifecycle.is_empty(),
            "{}: lifecycle must be populated",
            m.milestone.id
        );
    }

    let after = validate_code_multiset(&plan_dir);
    assert_eq!(
        before, after,
        "gate-code multiset must match pre-/post-migration for parity"
    );
}

#[test]
fn normalizes_inconsistent_state_and_quiets_g1() {
    // Mirror of the gate-g1-fail intent: legacy M has spec_status=interview
    // + execution_status=in-progress (inconsistent). Pre-migration → G1.
    // Post-migration: lifecycle="in-progress" → effective spec_status="ready"
    // → no G1. This is the intended normalization; parity is only required
    // for VALID legacy states (see the test above).
    let env = TestEnv::blank();
    let plan_dir: PathBuf = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(plan_dir.join("milestones")).unwrap();
    let mut m = legacy_milestone("03", "invalid", "interview", "in-progress", true);
    m.acceptance_criteria.clear();
    m.acceptance_criteria.push(AcceptanceCriterion {
        id: "AC-01".into(),
        description: "n/a".into(),
        verification: "manual".into(),
        evidence: "".into(),
        status: "pending".into(),
    });
    write_milestone(&plan_dir.join("milestones"), &m);

    let before = validate_code_multiset(&plan_dir);
    assert!(
        before.contains_key("G1"),
        "pre-migration G1 must fire on the inconsistent state: {before:?}"
    );

    let report = migrate_plan_lifecycle(&plan_dir).expect("migrate");
    assert!(report.migrated >= 1);

    let after = validate_code_multiset(&plan_dir);
    assert!(
        !after.contains_key("G1"),
        "post-migration G1 should be quiet (normalized): {after:?}"
    );
}

// ---------------------------------------------------------------------------
// M124 (M104 ER-3) regression pins for the two rerouted call sites
// ---------------------------------------------------------------------------
//
// Background: M104 (B-44) introduced `effective_spec_status` /
// `effective_execution_status` helpers, but two of the readers were left
// reading the raw `execution_status` / `spec_status` fields. After the
// deferred bulk lifecycle migration (M100 follow-up) clears those legacy
// fields, the raw reads return empty strings and the gates fire false G7/G8
// errors. M124 reroutes both sites through the helpers. These tests pin
// the reroute end-to-end on migrated milestones:
//   1. `validate_milestone_start_execution` (gates.rs:194-207) builds
//      `done_ids` via `effective_execution_status`. Before the fix this
//      returned empty strings for migrated deps and produced a false G8
//      "dependency not done" when starting work on the dependent
//      milestone.
//   2. `set_execution_status` done-arm (milestone.rs:747-753) checks
//      `effective_spec_status == "verified"` instead of the raw field.
//      Before the fix a migrated milestone with lifecycle=complete (raw
//      spec_status empty) tripped the gate and refused the transition.

fn migrated_complete_milestone(id: &str, slug: &str) -> MilestoneFile {
    let mut m = MilestoneFile::default();
    m.milestone.id = id.to_string();
    m.milestone.slug = slug.to_string();
    m.milestone.title = format!("{id} {slug}");
    m.milestone.lifecycle = "complete".to_string();
    // legacy fields cleared (post-migration shape)
    m.milestone.spec_status.clear();
    m.milestone.execution_status.clear();
    m.milestone.effort = "S".to_string();
    m.milestone.risk = "low".to_string();
    m.milestone.priority = "normal".to_string();
    m.milestone.created = "2026-07-04".to_string();
    m.milestone.updated = "2026-07-04".to_string();
    m.milestone.depends_on = Vec::new();
    m.intent.outcome = "outcome".to_string();
    m.problem.description = "problem".to_string();
    m.scope.in_scope = vec!["x".to_string()];
    m.scope.out_of_scope = vec!["a".to_string(), "b".to_string()];
    m
}

/// Pin the `validate_milestone_start_execution` reroute (gates.rs site):
/// on a migrated dependency (legacy fields cleared, lifecycle=complete),
/// G8 must NOT fire when starting the dependent milestone.
#[test]
fn gates_g8_survives_lifecycle_migration_done_ids_route() {
    let env = TestEnv::blank();
    let plan_dir: PathBuf = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(plan_dir.join("milestones")).unwrap();

    // M-dep: migrated, lifecycle=complete (raw execution_status="")
    let mut dep = migrated_complete_milestone("01", "dep");
    dep.acceptance_criteria.push(AcceptanceCriterion {
        id: "AC-01".into(),
        description: "dep AC".into(),
        verification: "manual".into(),
        evidence: "fixture".into(),
        status: "passed".into(),
    });
    dep.work_packages.push(WorkPackage {
        id: "WP1".into(),
        name: "WP1".into(),
        goal: "done".into(),
        rollback: "n/a".into(),
        steps: vec![Step {
            id: "S1".into(),
            action: "do".into(),
            covers_ac: vec!["AC-01".into()],
            depends_on_steps: vec![],
            done_when: "done".into(),
            files: vec![],
            tests: "manual".into(),
            work_package: "WP1".into(),
            status: "done".into(),
            claimed_at: String::new(),
            claimed_by: String::new(),
            lease_expires_at: String::new(),
            order: 1,
            evidence: String::new(),
        }],
    });
    dep.verification.branch = "main".into();
    dep.verification.date = "2026-07-04".into();
    dep.verification.evidence = "fixture".into();
    write_milestone(&plan_dir.join("milestones"), &dep);

    // M-main: legacy-shape, approved, in-progress, depends_on=01
    let mut main = legacy_milestone("02", "main", "ready", "in-progress", true);
    main.milestone.depends_on = vec!["01".to_string()];
    main.acceptance_criteria.clear();
    main.acceptance_criteria.push(AcceptanceCriterion {
        id: "AC-01".into(),
        description: "main AC".into(),
        verification: "manual".into(),
        evidence: "fixture".into(),
        status: "passed".into(),
    });
    write_milestone(&plan_dir.join("milestones"), &main);

    let ctx = mp::paths::PlanContext::discover(
        Some(plan_dir.clone()),
        Some(plan_dir.parent().unwrap().to_path_buf()),
    )
    .expect("PlanContext::discover");
    let raw = std::fs::read_to_string(plan_dir.join("milestones").join("02-main.json")).unwrap();
    let main_doc: MilestoneFile = serde_json::from_str(&raw).unwrap();

    let errors = mp::validate::validate_milestone_start_execution(&ctx, &main_doc);
    assert!(
        errors.is_empty(),
        "validate_milestone_start_execution must NOT fire G8 on a migrated (lifecycle=complete) dep; \
         rerouted site is gates.rs done_ids via effective_execution_status. \
         errors={errors:?}"
    );
    assert!(
        !errors.iter().any(|e| e.code == "G8"),
        "explicit G8 check: found {:?}",
        errors.iter().find(|e| e.code == "G8")
    );
}

/// Pin the `set_execution_status` done-arm reroute (milestone.rs site):
/// on a migrated milestone (legacy fields cleared, lifecycle=complete),
/// `set_execution_status("done")` must succeed without the false
/// "execution_status done requires spec_status verified" error.
#[test]
fn set_execution_status_done_arm_routes_through_effective_spec_status() {
    // TestEnv::new() runs `mp init --profile full` so the plan.json
    // index exists — `set_execution_status` writes via sync::sync_plan,
    // which reads the index.
    let env = TestEnv::new();
    let plan_dir: PathBuf = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(plan_dir.join("milestones")).unwrap();

    let mut m = migrated_complete_milestone("03", "done-arm");
    m.acceptance_criteria.push(AcceptanceCriterion {
        id: "AC-01".into(),
        description: "AC".into(),
        verification: "manual".into(),
        evidence: "fixture".into(),
        status: "passed".into(),
    });
    write_milestone(&plan_dir.join("milestones"), &m);

    let ctx = mp::paths::PlanContext::discover(
        Some(plan_dir.clone()),
        Some(plan_dir.parent().unwrap().to_path_buf()),
    )
    .expect("PlanContext::discover");

    // Drive the public API. Pre-fix this fails with
    // "execution_status done requires spec_status verified (use milestone complete)".
    mp::milestone::set_execution_status(&ctx, "03", "done").expect(
        "set_execution_status(done) must succeed on lifecycle=complete (M104 ER-3 reroute)",
    );

    // Confirm the legacy execution_status was written but the underlying
    // effective state still resolves to done via lifecycle (defensive pin).
    let raw =
        std::fs::read_to_string(plan_dir.join("milestones").join("03-done-arm.json")).unwrap();
    let updated: MilestoneFile = serde_json::from_str(&raw).unwrap();
    assert_eq!(updated.milestone.execution_status, "done");
    assert_eq!(
        mp::validate::effective_execution_status(&updated),
        "done",
        "post-set-execution-status the milestone must still resolve to done via effective_execution_status"
    );
}
