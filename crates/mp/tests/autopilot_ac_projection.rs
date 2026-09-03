//! M207 / S5 / AC-05: revisioned AC projection synchronized from
//! canonical milestone criterion state.
//!
//! Asserts:
//! - `project_ac_status` returns `Written` for a fresh (milestone, AC)
//!   pair and persists into session.json.
//! - The next write with the *same* `source_revision` and same
//!   payload is a no-op (`NoChange`).
//! - A write with a *different* `source_revision` is rejected
//!   (`StaleRevision`); the stored projection is preserved.
//! - Stale / conflicting writes do not silently create a second
//!   authority — the projection is unchanged.

mod common;

use common::TestEnv;
use mp::autopilot::ac_projection::{
    canonical_revision, project_ac_status, AcProjection, AcStatus, ProjectionKey,
    ProjectionWriteOutcome,
};
use mp::autopilot::session::{load_session, sample_session_for_tests, save_session, AutopilotSession};
use mp::paths::PlanContext;
use std::path::Path;

fn ctx_in(dir: &Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

#[test]
fn projection_writes_new_entry() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    // Drop the sample's seeded projection so the test starts from
    // a clean projection slate.
    session.ac_projections.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    // Library-level projection write.
    let mut loaded = load_session(&ctx, "alpha").unwrap();
    let key = ProjectionKey::new("207", "AC-01");
    let next = AcProjection {
        ac_id: "AC-01".into(),
        status: AcStatus::Passed,
        evidence: Some("ok".into()),
        source_revision: "rev-1".into(),
        projected_at: None,
    };
    let outcome = project_ac_status(&mut loaded, key, next);
    assert_eq!(outcome, ProjectionWriteOutcome::Written);
    save_session(&ctx, "alpha", &loaded).unwrap();

    // Disk reflects the new projection.
    let reloaded = load_session(&ctx, "alpha").unwrap();
    let proj = reloaded
        .ac_projections
        .get("207")
        .and_then(|m| m.get("AC-01"))
        .expect("AC-01 must be projected");
    assert_eq!(proj.status, AcStatus::Passed);
    assert_eq!(proj.source_revision, "rev-1");
}

#[test]
fn projection_no_change_when_revision_and_status_unchanged() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.ac_projections.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    let mut loaded = load_session(&ctx, "alpha").unwrap();
    let key = ProjectionKey::new("207", "AC-01");
    let next = AcProjection {
        ac_id: "AC-01".into(),
        status: AcStatus::Passed,
        evidence: Some("ok".into()),
        source_revision: "rev-1".into(),
        projected_at: None,
    };
    project_ac_status(&mut loaded, key.clone(), next.clone());
    let second = project_ac_status(&mut loaded, key, next);
    assert_eq!(second, ProjectionWriteOutcome::NoChange);
}

#[test]
fn stale_revision_does_not_overwrite_stored_projection() {
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session = sample_session_for_tests("alpha");
    session.ac_projections.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    let mut loaded = load_session(&ctx, "alpha").unwrap();
    let key = ProjectionKey::new("207", "AC-01");
    let first = AcProjection {
        ac_id: "AC-01".into(),
        status: AcStatus::Passed,
        evidence: Some("ok".into()),
        source_revision: "rev-1".into(),
        projected_at: None,
    };
    project_ac_status(&mut loaded, key.clone(), first.clone());

    // Different source_revision → stale. Stored projection is preserved.
    let stale = AcProjection {
        ac_id: "AC-01".into(),
        status: AcStatus::Failed,
        evidence: Some("nope".into()),
        source_revision: "rev-2".into(),
        projected_at: None,
    };
    let outcome = project_ac_status(&mut loaded, key, stale);
    match outcome {
        ProjectionWriteOutcome::StaleRevision { stored, attempted } => {
            assert_eq!(stored.as_str(), "rev-1");
            assert_eq!(attempted.as_str(), "rev-2");
        }
        other => panic!("expected StaleRevision, got {other:?}"),
    }

    // Save and re-load to confirm the stored projection survived.
    save_session(&ctx, "alpha", &loaded).unwrap();
    let reloaded = load_session(&ctx, "alpha").unwrap();
    let stored = reloaded
        .ac_projections
        .get("207")
        .and_then(|m| m.get("AC-01"))
        .unwrap();
    assert_eq!(stored.source_revision, "rev-1");
    assert_eq!(stored.status, AcStatus::Passed);
}

#[test]
fn canonical_revision_is_stable_for_same_state() {
    let rev_a = canonical_revision(
        "seed",
        "207",
        &[("AC-01", AcStatus::Passed), ("AC-02", AcStatus::Pending)],
    );
    let rev_b = canonical_revision(
        "seed",
        "207",
        &[("AC-01", AcStatus::Passed), ("AC-02", AcStatus::Pending)],
    );
    assert_eq!(rev_a, rev_b);
}

#[test]
fn canonical_revision_changes_on_state_change() {
    let rev_a = canonical_revision("seed", "207", &[("AC-01", AcStatus::Pending)]);
    let rev_b = canonical_revision("seed", "207", &[("AC-01", AcStatus::Passed)]);
    assert_ne!(rev_a, rev_b);
}

#[test]
fn project_ac_status_writes_through_session_io() {
    // Library API: project + save_session + load_session
    // round-trip preserves the projection.
    let env = TestEnv::new();
    let ctx = ctx_in(env.tmp.path());
    let mut session: AutopilotSession = sample_session_for_tests("alpha");
    session.ac_projections.clear();
    save_session(&ctx, "alpha", &session).unwrap();

    let mut loaded = load_session(&ctx, "alpha").unwrap();
    let next = AcProjection {
        ac_id: "AC-01".into(),
        status: AcStatus::Passed,
        evidence: Some("ok".into()),
        source_revision: "rev-A".into(),
        projected_at: Some("2026-01-01T00:00:00Z".into()),
    };
    project_ac_status(
        &mut loaded,
        ProjectionKey::new("207", "AC-01"),
        next,
    );
    save_session(&ctx, "alpha", &loaded).unwrap();

    let reloaded = load_session(&ctx, "alpha").unwrap();
    let stored = reloaded
        .ac_projections
        .get("207")
        .and_then(|m| m.get("AC-01"))
        .expect("AC-01 must be persisted");
    assert_eq!(stored.source_revision, "rev-A");
    assert_eq!(stored.status, AcStatus::Passed);
    assert_eq!(stored.projected_at.as_deref(), Some("2026-01-01T00:00:00Z"));
}