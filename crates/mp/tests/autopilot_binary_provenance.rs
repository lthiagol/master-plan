//! M210 / AC-07: session provenance records the executing mp
//! binary path, version, and build/schema compatibility. A stale
//! binary that cannot preserve the M207 schema is rejected
//! before any plan write with an actionable rebuild/install hint.
//!
//! Coverage:
//! - MpBinaryProvenance::current() builds a provenance record
//!   from std::env::current_exe() + CARGO_PKG_VERSION.
//! - check_binary_provenance accepts the first spawn (no
//!   recorded provenance).
//! - check_binary_provenance rejects:
//!     * recorded schema below MIN_SESSION_SCHEMA_VERSION
//!       (SchemaBelowFloor)
//!     * current schema older than recorded (SchemaTooNew)
//!     * binary_path mismatch between recorded and current
//!       (BinaryPathMismatch)
//! - The rejection hint is actionable ("Rebuild mp" /
//!   "`make install`").
//! - The spawn pipeline refuses to write session.json when
//!   check_binary_provenance returns Err.

use mp::autopilot::session::SESSION_SCHEMA_VERSION;
use mp::autopilot::spawn::{
    check_binary_provenance, BinaryProvenanceMismatch, MpBinaryProvenance,
    MIN_SESSION_SCHEMA_VERSION,
};
use tempfile::TempDir;

#[test]
fn current_provenance_records_exe_path_version_and_schema() {
    let p = MpBinaryProvenance::current();
    // The binary path is the current executable (cargo test
    // runs the test binary, not mp itself, but std::env::current_exe
    // is populated either way).
    assert!(!p.binary_path.is_empty(), "binary_path must be populated");
    // Version comes from CARGO_PKG_VERSION (mp's crate version).
    assert!(!p.version.is_empty(), "version must be populated");
    // Build kind is "dev" or "release".
    assert!(matches!(p.build_kind.as_str(), "dev" | "release"));
    // Schema version matches the current SESSION_SCHEMA_VERSION.
    assert_eq!(p.schema_version, SESSION_SCHEMA_VERSION);
    // recorded_at is an RFC3339 string.
    assert!(p.recorded_at.contains('T'));
}

#[test]
fn check_binary_provenance_accepts_first_spawn() {
    // No recorded provenance — first spawn.
    let current = MpBinaryProvenance::current();
    check_binary_provenance(None, &current).expect("first spawn must accept");
}

#[test]
fn check_binary_provenance_accepts_matching_record() {
    let recorded = MpBinaryProvenance::current();
    let current = MpBinaryProvenance::current();
    check_binary_provenance(Some(&recorded), &current).expect("matching record must accept");
}

#[test]
fn check_binary_provenance_rejects_below_floor() {
    let recorded = MpBinaryProvenance {
        binary_path: "/old/mp".into(),
        version: "0.9.0".into(),
        schema_version: MIN_SESSION_SCHEMA_VERSION.saturating_sub(1),
        build_kind: "release".into(),
        recorded_at: "2026-01-01T00:00:00Z".into(),
    };
    let current = MpBinaryProvenance::current();
    let err = check_binary_provenance(Some(&recorded), &current).unwrap_err();
    match *err {
        BinaryProvenanceMismatch::SchemaBelowFloor { floor, .. } => {
            assert_eq!(floor, MIN_SESSION_SCHEMA_VERSION);
        }
        other => panic!("expected SchemaBelowFloor, got {other:?}"),
    }
}

#[test]
fn check_binary_provenance_rejects_too_new_recorded_schema() {
    let recorded = MpBinaryProvenance {
        binary_path: "/future/mp".into(),
        version: "2.0.0".into(),
        schema_version: SESSION_SCHEMA_VERSION + 5,
        build_kind: "release".into(),
        recorded_at: "2026-01-01T00:00:00Z".into(),
    };
    let current = MpBinaryProvenance::current();
    let err = check_binary_provenance(Some(&recorded), &current).unwrap_err();
    assert!(matches!(
        *err,
        BinaryProvenanceMismatch::SchemaTooNew { .. }
    ));
}

#[test]
fn check_binary_provenance_rejects_path_mismatch() {
    let recorded = MpBinaryProvenance {
        binary_path: "/first/install/mp".into(),
        version: "1.0.0-rc2".into(),
        schema_version: SESSION_SCHEMA_VERSION,
        build_kind: "release".into(),
        recorded_at: "2026-01-01T00:00:00Z".into(),
    };
    let current = MpBinaryProvenance::current();
    let err = check_binary_provenance(Some(&recorded), &current).unwrap_err();
    assert!(matches!(
        *err,
        BinaryProvenanceMismatch::BinaryPathMismatch { .. }
    ));
}

#[test]
fn rejection_hints_carry_rebuild_or_install_action() {
    // AC-07: rejection hint must be actionable.
    let recorded = MpBinaryProvenance {
        binary_path: "/future/mp".into(),
        version: "2.0.0".into(),
        schema_version: SESSION_SCHEMA_VERSION + 5,
        build_kind: "release".into(),
        recorded_at: "2026-01-01T00:00:00Z".into(),
    };
    let current = MpBinaryProvenance::current();
    let err = check_binary_provenance(Some(&recorded), &current).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("Rebuild") || msg.contains("cargo build"),
        "rejection hint must mention rebuild: {msg}"
    );
    assert!(
        msg.contains("install") || msg.contains("make install"),
        "rejection hint must mention install: {msg}"
    );

    // Path mismatch hint.
    let recorded = MpBinaryProvenance {
        binary_path: "/first/install/mp".into(),
        version: "1.0.0-rc2".into(),
        schema_version: SESSION_SCHEMA_VERSION,
        build_kind: "release".into(),
        recorded_at: "2026-01-01T00:00:00Z".into(),
    };
    let err = check_binary_provenance(Some(&recorded), &current).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("make install") || msg.contains("install"),
        "path mismatch hint must mention install: {msg}"
    );
}

#[test]
fn spawn_pipeline_refuses_to_persist_session_on_stale_binary() {
    // End-to-end: a pre-existing session.json with a recorded
    // provenance that is newer than the current binary must
    // cause spawn_session to return SpawnError::StaleBinary
    // BEFORE any plan write. The session.json on disk is NOT
    // modified (no schema-too-new mutation that would silently
    // drop fields).
    use mp::autopilot::prompts::spawn::{RoleReexport as Role, TopologyReexport as Topology};
    use mp::autopilot::role::resolve_role_config;
    use mp::autopilot::session::AutopilotSession;
    use mp::autopilot::spawn::{spawn_session, MockHerdrSpawnOps, SpawnError, SpawnInputs};
    use mp::paths::PlanContext;

    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path().to_path_buf();
    let plan_dir = project_root.join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    let ctx = PlanContext {
        project_root: project_root.clone(),
        plan_dir: plan_dir.clone(),
    };

    // Pre-write a session.json with a "too-new" provenance.
    // Use sample_session_for_tests to get a valid topology +
    // roles + queue shape, then override the binary_provenance.
    let mut session = AutopilotSession::sample("sess-alpha");
    session.binary_provenance = Some(MpBinaryProvenance {
        binary_path: "/future/mp".into(),
        version: "2.0.0".into(),
        schema_version: SESSION_SCHEMA_VERSION + 5,
        build_kind: "release".into(),
        recorded_at: "2026-01-01T00:00:00Z".into(),
    });
    mp::autopilot::session::save_session(&ctx, "sess-alpha", &session).unwrap();

    // Capture the recorded file content before spawn.
    let session_path = mp::autopilot::session::SessionPath::new(&ctx, "sess-alpha")
        .unwrap()
        .file;
    let pre_content = std::fs::read_to_string(&session_path).unwrap();

    // Run the pipeline — must fail with StaleBinary.
    let ro = resolve_role_config(
        None,
        None,
        &mp::autopilot::role::builtin_role_default(Role::Orchestrator),
    );
    let rr = resolve_role_config(
        None,
        None,
        &mp::autopilot::role::builtin_role_default(Role::Runner),
    );
    let rv = resolve_role_config(
        None,
        None,
        &mp::autopilot::role::builtin_role_default(Role::Reviewer),
    );
    let si = SpawnInputs {
        ctx: &ctx,
        session_id: "sess-alpha",
        topology: Topology::ThreeAgent,
        project_root: project_root.as_path(),
        role_o: ro,
        role_r: rr,
        role_v: rv,
        project_name: "master-plan",
        milestone_id: "M210",
        queue_position: 0,
    };
    let ops = MockHerdrSpawnOps::new();
    let err = spawn_session(&ops, &si).expect_err("pipeline must reject stale binary");
    match err {
        SpawnError::StaleBinary(mismatch) => {
            assert!(matches!(
                *mismatch,
                BinaryProvenanceMismatch::SchemaTooNew { .. }
            ));
        }
        other => panic!("expected StaleBinary, got {other:?}"),
    }
    // session.json content is unchanged (no plan mutation).
    let post_content = std::fs::read_to_string(&session_path).unwrap();
    assert_eq!(pre_content, post_content);
}
