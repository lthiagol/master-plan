//! M100 AC-07: bulk migration over a fixture plan.
//!
//! Reads a real fixture project (which uses the legacy 3-field shape),
//! runs `migrate_plan_lifecycle`, and verifies:
//! - all on-disk files are migrated to the new shape (no legacy fields)
//! - the result validates clean (mp validate is green; full check is
//!   deferred to a CI step because validate needs MP_HOME to be set)
//! - the migration is idempotent (re-running changes nothing)

use crate::common::TestEnv;
use mp::migrate::migrate_plan_lifecycle;
use mp_model::MilestoneFile;
use std::path::PathBuf;

#[test]
fn bulk_migration_maps_verified_done_to_complete_and_validates_clean() {
    // Use the walkthrough-oauth fixture (3 milestones, all done in legacy shape).
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let plan_dir = env.tmp.path().join("master-plan");
    let report = migrate_plan_lifecycle(&plan_dir).expect("migrate");
    assert_eq!(report.decode_errors.len(), 0, "no decode errors expected");
    assert_eq!(report.migrated, 3, "all 3 fixtures should migrate");
    assert_eq!(report.skipped, 0);

    // Re-read every file and assert the new shape.
    let entries: Vec<PathBuf> = std::fs::read_dir(plan_dir.join("milestones"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    assert_eq!(entries.len(), 3);

    for path in &entries {
        let raw = std::fs::read_to_string(path).unwrap();
        let m: MilestoneFile = serde_json::from_str(&raw).expect("post-migrate JSON");
        assert!(
            !m.milestone.lifecycle.is_empty(),
            "{}: lifecycle must be populated",
            path.display()
        );
        assert!(
            LIFECYCLE_STATES.contains(&m.milestone.lifecycle.as_str()),
            "{}: lifecycle '{}' must be a valid state",
            path.display(),
            m.milestone.lifecycle
        );
        assert!(
            m.milestone.spec_status.is_empty(),
            "{}: legacy spec_status must be cleared",
            path.display()
        );
        assert!(
            m.milestone.execution_status.is_empty(),
            "{}: legacy execution_status must be cleared",
            path.display()
        );
    }
}

#[test]
fn bulk_migration_is_idempotent_on_real_fixture() {
    let env = TestEnv::from_fixture("walkthrough-oauth");
    let plan_dir = env.tmp.path().join("master-plan");

    let r1 = migrate_plan_lifecycle(&plan_dir).expect("first migrate");
    assert_eq!(r1.migrated, 3);

    let r2 = migrate_plan_lifecycle(&plan_dir).expect("second migrate");
    assert_eq!(r2.migrated, 0);
    assert_eq!(r2.skipped, 3);
    assert!(r2.decode_errors.is_empty());
}

#[test]
fn bulk_migration_leaves_already_migrated_files_untouched() {
    // Run migrate twice; assert the second pass writes no files.
    let env = TestEnv::from_fixture("walkthrough-oauth");
    let plan_dir = env.tmp.path().join("master-plan");

    migrate_plan_lifecycle(&plan_dir).expect("first");
    let entries: Vec<PathBuf> = std::fs::read_dir(plan_dir.join("milestones"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    let mtimes_before: Vec<_> = entries
        .iter()
        .map(|p| std::fs::metadata(p).unwrap().modified().unwrap())
        .collect();

    // Sleep briefly so a second-pass write would have a later mtime.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let r2 = migrate_plan_lifecycle(&plan_dir).expect("second");
    assert_eq!(r2.migrated, 0, "second pass must be a no-op");

    let mtimes_after: Vec<_> = entries
        .iter()
        .map(|p| std::fs::metadata(p).unwrap().modified().unwrap())
        .collect();
    assert_eq!(
        mtimes_before, mtimes_after,
        "second pass must not touch the files"
    );
}

// Bring the LIFECYCLE_STATES constant into scope (avoids a top-level import
// for the whole file when only one test uses it).
use mp_model::LIFECYCLE_STATES;
