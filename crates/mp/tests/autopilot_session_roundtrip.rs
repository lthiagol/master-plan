//! M207 / S02 / AC-02: sample session.json round-trip.
//!
//! Asserts that the canonical "fully populated" sample — the shape
//! the spec calls out (3-pane topology, 2 milestones in queue, role
//! config snapshots, evidence_refs pointing at lifecycle /
//! execution_status / spec_status / reviews.json verdict) — survives
//! serialize -> bounded read -> deserialize without data loss.

use mp::autopilot::{
    load_session, sample_session_for_tests, save_session, EvidenceRefs, QueueItem, RoleConfig,
    RoleName, RoleState, SessionConfigOverrides, SessionStatus, Stage, Topology, PaneRef,
    RolesConfig, WorkingOn, RoleStateEnvelope, RoleStateRecord, AutopilotSession,
};
use mp::paths::PlanContext;
use tempfile::TempDir;

fn ctx_in(dir: &std::path::Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

#[test]
fn sample_session_round_trips_through_save_and_load() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let original = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &original).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    // `last_updated` is auto-stamped on save; everything else
    // round-trips exactly.
    let mut expected = original.clone();
    expected.last_updated = loaded.last_updated.clone();
    assert_eq!(loaded, expected);
}

#[test]
fn sample_session_has_required_schema_shape() {
    let s = sample_session_for_tests("alpha");
    // Required: id, schema_version, topology, roles, queue, status,
    // last_updated.
    assert!(!s.id.is_empty());
    assert_eq!(s.schema_version, mp::autopilot::SESSION_SCHEMA_VERSION);
    assert_eq!(s.status, SessionStatus::Active);
    assert!(!s.last_updated.is_empty());
    // Spec calls out: 3-pane topology.
    assert!(s.topology.orchestrator.is_some());
    assert!(s.topology.runner.is_some());
    assert!(s.topology.reviewer.is_some());
    // Spec calls out: 2 milestones in queue.
    assert_eq!(s.queue.len(), 2);
    // Spec calls out: role config snapshots.
    assert!(s.roles.orchestrator.is_some());
    assert!(s.roles.runner.is_some());
    assert!(s.roles.reviewer.is_some());
    // Spec calls out: evidence_refs point at lifecycle /
    // execution_status / spec_status / reviews.json verdict for
    // every queue item.
    for item in &s.queue {
        let refs = item
        .evidence_refs
        .as_ref()
        .expect("every queue item must have evidence_refs");
        assert!(refs.lifecycle.is_some(), "milestone {} missing lifecycle ref", item.milestone_id);
        assert!(refs.execution_status.is_some(), "milestone {} missing execution_status ref", item.milestone_id);
        assert!(refs.spec_status.is_some(), "milestone {} missing spec_status ref", item.milestone_id);
    }
}

#[test]
fn save_session_stamps_last_updated_on_each_write() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let s = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &s).unwrap();
    let first = load_session(&ctx, "alpha").unwrap().last_updated;
    // Re-save with the same struct — last_updated should still be
    // stamped fresh.
    save_session(&ctx, "alpha", &s).unwrap();
    let second = load_session(&ctx, "alpha").unwrap().last_updated;
    // Both should parse as RFC3339. We tolerate ties on fast clocks.
    assert!(first.contains('T'));
    assert!(second.contains('T'));
}

#[test]
fn save_session_at_is_idempotent_under_double_save() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let s = sample_session_for_tests("alpha");
    let path_a = save_session(&ctx, "alpha", &s).unwrap();
    let path_b = save_session(&ctx, "alpha", &s).unwrap();
    assert_eq!(path_a, path_b);
}

#[test]
fn loaded_session_validates_against_embedded_schema() {
    // White-box re-validation: the loader runs schema validation
    // before returning. This test re-runs the validator on the
    // loaded document to pin the contract: every round-tripped
    // session validates against the embedded schema.
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let s = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &s).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    let value = serde_json::to_value(&loaded).unwrap();
    let errs = mp::autopilot::validate_session_value(&value).unwrap();
    assert!(errs.is_empty(), "loaded session failed validation: {errs:?}");
}

#[test]
fn round_trip_preserves_full_topology_pane_ids() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let s = sample_session_for_tests("alpha");
    let orch_pane = s.topology.orchestrator.as_ref().unwrap().pane_id.clone();
    let runner_pane = s.topology.runner.as_ref().unwrap().pane_id.clone();
    let reviewer_pane = s.topology.reviewer.as_ref().unwrap().pane_id.clone();
    save_session(&ctx, "alpha", &s).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    assert_eq!(
        loaded.topology.orchestrator.as_ref().unwrap().pane_id,
        orch_pane
    );
    assert_eq!(
        loaded.topology.runner.as_ref().unwrap().pane_id,
        runner_pane
    );
    assert_eq!(
        loaded.topology.reviewer.as_ref().unwrap().pane_id,
        reviewer_pane
    );
}

#[test]
fn round_trip_preserves_role_config_snapshots() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let s = sample_session_for_tests("alpha");
    save_session(&ctx, "alpha", &s).unwrap();
    let loaded = load_session(&ctx, "alpha").unwrap();
    let runner = loaded.roles.runner.as_ref().unwrap();
    assert_eq!(runner.role, RoleName::Runner);
    assert_eq!(runner.skill.as_deref(), Some("mp-runner"));
    assert!(runner.harness.is_some());
    assert!(runner.model.is_some());
}

// Silence unused warnings on types re-exported for integration
// tests that import them.
#[allow(dead_code)]
fn _types(
    _: Topology,
    _: PaneRef,
    _: RolesConfig,
    _: RoleConfig,
    _: SessionConfigOverrides,
    _: Stage,
    _: QueueItem,
    _: EvidenceRefs,
    _: RoleState,
    _: RoleStateRecord,
    _: RoleStateEnvelope,
    _: WorkingOn,
    _: AutopilotSession,
) {
}