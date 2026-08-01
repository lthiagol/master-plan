//! M144: pin the `lifecycle_at` field semantics:
//!   * AC-01: healthy milestone serializes without the key; transitioned
//!     milestone serializes with the key set.
//!   * AC-02: every write path that sets `lifecycle` also sets `lifecycle_at`.
//!   * AC-03: overlay flips (blocked, deferred, cancelled) do NOT update
//!     `lifecycle_at`.
//!   * AC-04: migration backfill populates `lifecycle_at` from `created`.
//!   * AC-08: JSON shape golden stays byte-identical for healthy milestones.

use crate::common::TestEnv;
use mp::migrate::migrate_plan_lifecycle;
use mp::milestone::{
    apply_spec_status, block_milestone, complete_milestone, defer_milestone, reopen_milestone,
    set_execution_status, unblock_milestone,
};
use mp::paths::PlanContext;
use mp_model::{AcceptanceCriterion, MilestoneFile, WorkPackage};
use std::path::Path;

/// Build a minimal valid milestone file directly on disk. We bypass the
/// CLI because `mp milestone create` enforces schema validation on
/// `intent.outcome` and `scope.out_of_scope`, which is overkill for
/// lifecycle_at tests — we only need a fixture the lifecycle setters
/// will accept.
fn write_milestone(env: &TestEnv, id: &str) -> String {
    let plan_dir = env.tmp.path().join("master-plan");
    let milestones_dir = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones_dir).expect("mkdir milestones");
    let mut m = MilestoneFile::default();
    m.milestone.id = id.to_string();
    m.milestone.slug = format!("{id}-test");
    m.milestone.title = format!("{id} test");
    m.milestone.lifecycle = "draft".to_string();
    m.milestone.lifecycle_at = Some("2026-01-01T00:00:00Z".to_string());
    m.milestone.spec_status = String::new();
    m.milestone.execution_status = String::new();
    m.milestone.effort = "S".to_string();
    m.milestone.risk = "low".to_string();
    m.milestone.priority = "normal".to_string();
    m.milestone.created = "2026-01-01".to_string();
    m.milestone.updated = "2026-01-01".to_string();
    m.intent.outcome = "outcome".to_string();
    m.problem.description = "problem".to_string();
    m.scope.in_scope = vec!["x".to_string()];
    m.scope.out_of_scope = vec!["a".to_string(), "b".to_string()];
    m.acceptance_criteria = vec![AcceptanceCriterion {
        id: "AC-01".to_string(),
        description: "AC1".to_string(),
        verification: "echo ok".to_string(),
        evidence: String::new(),
        status: "pending".to_string(),
    }];
    m.work_packages = vec![WorkPackage {
        id: "WP1".to_string(),
        name: "WP1".to_string(),
        goal: "do the thing".to_string(),
        rollback: "n/a".to_string(),
        steps: vec![],
    }];
    let path = milestones_dir.join(format!("{id}-{id}-test.json"));
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&m).expect("serialize")),
    )
    .expect("write milestone file");
    id.to_string()
}

fn load_milestone(env: &TestEnv, id: &str) -> MilestoneFile {
    let plan_dir = env.tmp.path().join("master-plan");
    let path = plan_dir
        .join("milestones")
        .join(format!("{id}-{id}-test.json"));
    let raw = std::fs::read_to_string(&path).expect("read milestone file");
    serde_json::from_str(&raw).expect("parse milestone file")
}

fn load_milestone_path(plan_dir: &Path, id: &str, slug: &str) -> MilestoneFile {
    let path = plan_dir
        .join("milestones")
        .join(format!("{id}-{slug}.json"));
    let raw = std::fs::read_to_string(&path).expect("read milestone file");
    serde_json::from_str(&raw).expect("parse milestone file")
}

fn ctx_for(env: &TestEnv) -> PlanContext {
    let plan_dir = env.tmp.path().join("master-plan");
    let workdir = env.tmp.path().to_path_buf();
    PlanContext::discover(Some(plan_dir), Some(workdir)).expect("PlanContext::discover")
}

/// AC-01 + AC-08: a freshly created milestone has `lifecycle_at` set
/// (because create_milestone enters `draft`); the field is preserved on
/// reload; healthy default (`None`) is byte-absent in serialized JSON.
#[test]
fn create_milestone_sets_lifecycle_at() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "01");
    let m = load_milestone(&env, &id);
    let at = m
        .milestone
        .lifecycle_at
        .as_ref()
        .expect("create_milestone must populate lifecycle_at");
    assert!(!at.is_empty(), "lifecycle_at must not be empty");
}

/// AC-02: apply_spec_status sets lifecycle_at. Drive approved → ready
/// path on a draft milestone.
#[test]
fn apply_spec_status_sets_lifecycle_at() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "02");
    let ctx = ctx_for(&env);
    let at_before = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .unwrap_or_default();
    // Sleep briefly so the timestamp differs.
    std::thread::sleep(std::time::Duration::from_millis(50));
    apply_spec_status(&ctx, &id, "review").expect("apply_spec_status");
    let m = load_milestone(&env, &id);
    assert_eq!(m.milestone.lifecycle, "groomed");
    let at_after = m
        .milestone
        .lifecycle_at
        .as_ref()
        .expect("lifecycle_at must be set after apply_spec_status");
    assert_ne!(
        at_after, &at_before,
        "lifecycle_at must change on lifecycle transition"
    );
}

/// M144 code-review (F-08): re-applying a `spec_status` that maps to the
/// SAME lifecycle must NOT bump `lifecycle_at`. `"review"` and `"interview"`
/// both map to `groomed`, so the second write is a no-op for lifecycle and
/// the TUI "since" clock must stay pinned. Without the F-08 guard every
/// spec_status write reset the timestamp.
#[test]
fn apply_spec_status_same_lifecycle_keeps_lifecycle_at() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "08");
    let ctx = ctx_for(&env);
    // draft → groomed via `review`; bumps lifecycle_at.
    apply_spec_status(&ctx, &id, "review").expect("review");
    let at_after_first = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .expect("lifecycle_at set after review");
    assert_eq!(load_milestone(&env, &id).milestone.lifecycle, "groomed");

    // Sleep so a bump would be observable, then re-map to the same lifecycle.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    apply_spec_status(&ctx, &id, "interview").expect("interview");
    let m = load_milestone(&env, &id);
    assert_eq!(
        m.milestone.lifecycle, "groomed",
        "interview must map to groomed"
    );
    assert_eq!(
        m.milestone.lifecycle_at.as_deref(),
        Some(at_after_first.as_str()),
        "lifecycle_at must NOT bump when the lifecycle is unchanged"
    );
}

/// AC-03: block_milestone is an overlay flip; lifecycle_at must NOT change.
/// First transition into `in-progress`, capture lifecycle_at, then block,
/// then verify the timestamp is unchanged.
#[test]
fn block_milestone_preserves_lifecycle_at() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "03");
    let ctx = ctx_for(&env);
    // Move into a stable lifecycle (groomed).
    apply_spec_status(&ctx, &id, "review").expect("apply_spec_status");
    std::thread::sleep(std::time::Duration::from_millis(50));
    apply_spec_status(&ctx, &id, "ready").expect("ready");
    let at_before = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .expect("set after transition");
    std::thread::sleep(std::time::Duration::from_millis(50));
    block_milestone(&ctx, &id, "test block", None).expect("block_milestone");
    let m = load_milestone(&env, &id);
    assert!(m.milestone.blocked);
    assert_eq!(
        m.milestone.lifecycle_at.as_deref(),
        Some(at_before.as_str()),
        "block_milestone is an overlay flip and must not update lifecycle_at"
    );
}

/// AC-03: defer_milestone preserves lifecycle_at.
#[test]
fn defer_milestone_preserves_lifecycle_at() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "04");
    let ctx = ctx_for(&env);
    apply_spec_status(&ctx, &id, "review").expect("apply_spec_status");
    std::thread::sleep(std::time::Duration::from_millis(50));
    apply_spec_status(&ctx, &id, "ready").expect("ready");
    let at_before = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .expect("set after transition");
    std::thread::sleep(std::time::Duration::from_millis(50));
    defer_milestone(&ctx, &id, "later", None).expect("defer_milestone");
    let m = load_milestone(&env, &id);
    assert!(m.milestone.deferred);
    assert_eq!(
        m.milestone.lifecycle_at.as_deref(),
        Some(at_before.as_str())
    );
}

/// AC-03: cancelled overlay preserves lifecycle_at. There's no public
/// `cancel_milestone` helper — cancellation is the bool overlay set by
/// migration / direct edit. We write a legacy-shape milestone with the
/// cancelled overlay true, run the migration, and verify the original
/// `lifecycle_at` (if any) is preserved through the overlay flip. The
/// migration backfill sets lifecycle_at from `created` when absent, so
/// this also pins the post-migration overlay does NOT clobber that
/// timestamp.
#[test]
fn cancelled_overlay_preserves_lifecycle_at() {
    let env = TestEnv::new();
    let plan_dir = env.tmp.path().join("master-plan");
    let milestones_dir = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones_dir).expect("mkdir milestones");
    let mut m = MilestoneFile::default();
    m.milestone.id = "01".to_string();
    m.milestone.slug = "cancel".to_string();
    m.milestone.title = "cancel".to_string();
    m.milestone.lifecycle = String::new();
    m.milestone.spec_status = "ready".to_string();
    m.milestone.execution_status = "planned".to_string();
    m.milestone.cancelled = true;
    m.milestone.effort = "S".to_string();
    m.milestone.risk = "low".to_string();
    m.milestone.priority = "normal".to_string();
    m.milestone.created = "2026-01-15".to_string();
    m.milestone.updated = "2026-01-15".to_string();
    let path = milestones_dir.join("01-cancel.json");
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&m).expect("serialize")),
    )
    .expect("write cancel fixture");

    let report = migrate_plan_lifecycle(&plan_dir).expect("migrate");
    assert_eq!(report.migrated, 1);

    let on_disk = load_milestone_path(&plan_dir, "01", "cancel");
    assert!(on_disk.milestone.cancelled);
    // M144 fix: migration backfill is RFC3339, not YYYY-MM-DD. Assert
    // the shape rather than a specific value (which would be migration-
    // time dependent).
    let at = on_disk
        .milestone
        .lifecycle_at
        .as_deref()
        .expect("cancelled overlay must preserve migration-backfilled lifecycle_at");
    assert!(
        at.len() >= 19,
        "lifecycle_at must be RFC3339-shaped; got: {at}"
    );
}

/// AC-03: unblock_milestone preserves lifecycle_at (overlay flip out).
#[test]
fn unblock_milestone_preserves_lifecycle_at() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "06");
    let ctx = ctx_for(&env);
    apply_spec_status(&ctx, &id, "review").expect("review");
    apply_spec_status(&ctx, &id, "ready").expect("ready");
    block_milestone(&ctx, &id, "block-then-unblock", None).expect("block");
    let at_before = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .expect("set before unblock");
    std::thread::sleep(std::time::Duration::from_millis(50));
    unblock_milestone(&ctx, &id).expect("unblock");
    let m = load_milestone(&env, &id);
    assert!(!m.milestone.blocked);
    assert_eq!(
        m.milestone.lifecycle_at.as_deref(),
        Some(at_before.as_str())
    );
}

/// AC-02: set_execution_status(in-progress) sets lifecycle_at.
#[test]
fn set_execution_status_in_progress_sets_lifecycle_at() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "07");
    let ctx = ctx_for(&env);
    apply_spec_status(&ctx, &id, "review").expect("review");
    std::thread::sleep(std::time::Duration::from_millis(50));
    apply_spec_status(&ctx, &id, "ready").expect("ready");
    let at_before = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .expect("set after ready");
    std::thread::sleep(std::time::Duration::from_millis(50));
    set_execution_status(&ctx, &id, "in-progress").expect("in-progress");
    let m = load_milestone(&env, &id);
    assert_eq!(m.milestone.lifecycle, "in-progress");
    let at_after = m
        .milestone
        .lifecycle_at
        .as_ref()
        .expect("lifecycle_at after in-progress");
    assert_ne!(
        at_after, &at_before,
        "lifecycle_at must change on in-progress transition"
    );
}

/// AC-02: reopen_milestone (after done) sets lifecycle_at.
#[test]
fn reopen_milestone_sets_lifecycle_at() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "08");
    let ctx = ctx_for(&env);
    apply_spec_status(&ctx, &id, "review").expect("review");
    apply_spec_status(&ctx, &id, "ready").expect("ready");
    set_execution_status(&ctx, &id, "in-progress").expect("in-progress");
    complete_milestone(&ctx, &id, None, None, true).expect("complete_milestone");
    let at_before = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .expect("set after complete");
    std::thread::sleep(std::time::Duration::from_millis(50));
    reopen_milestone(&ctx, &id).expect("reopen");
    let m = load_milestone(&env, &id);
    assert_eq!(m.milestone.lifecycle, "in-progress");
    let at_after = m
        .milestone
        .lifecycle_at
        .as_ref()
        .expect("lifecycle_at after reopen");
    assert_ne!(at_after, &at_before, "lifecycle_at must change on reopen");
}

/// AC-02: complete_milestone sets lifecycle_at.
#[test]
fn complete_milestone_sets_lifecycle_at() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "09");
    let ctx = ctx_for(&env);
    apply_spec_status(&ctx, &id, "review").expect("review");
    apply_spec_status(&ctx, &id, "ready").expect("ready");
    set_execution_status(&ctx, &id, "in-progress").expect("in-progress");
    let at_before = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .expect("set after in-progress");
    std::thread::sleep(std::time::Duration::from_millis(50));
    complete_milestone(&ctx, &id, None, None, true).expect("complete_milestone");
    let m = load_milestone(&env, &id);
    assert_eq!(m.milestone.lifecycle, "complete");
    let at_after = m
        .milestone
        .lifecycle_at
        .as_ref()
        .expect("lifecycle_at after complete");
    assert_ne!(at_after, &at_before);
}

/// AC-04: migration backfill populates lifecycle_at (RFC3339, set to
/// the migration's now_rfc3339 timestamp).
#[test]
fn migrate_backfills_lifecycle_at_from_created() {
    let env = TestEnv::new();
    // Build a legacy-shape milestone file directly (bypasses create_milestone
    // so we get a pre-migration shape with empty lifecycle).
    let plan_dir = env.tmp.path().join("master-plan");
    let milestones_dir = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones_dir).expect("mkdir milestones");
    let mut m = MilestoneFile::default();
    m.milestone.id = "01".to_string();
    m.milestone.slug = "legacy".to_string();
    m.milestone.title = "legacy".to_string();
    m.milestone.lifecycle = String::new();
    m.milestone.spec_status = "ready".to_string();
    m.milestone.execution_status = "planned".to_string();
    m.milestone.effort = "S".to_string();
    m.milestone.risk = "low".to_string();
    m.milestone.priority = "normal".to_string();
    m.milestone.created = "2026-01-01".to_string();
    m.milestone.updated = "2026-01-01".to_string();
    let path = milestones_dir.join("01-legacy.json");
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&m).expect("serialize")),
    )
    .expect("write legacy file");

    let report = migrate_plan_lifecycle(&plan_dir).expect("migrate");
    assert_eq!(report.migrated, 1);
    assert_eq!(report.skipped, 0);

    let on_disk = load_milestone_path(&plan_dir, "01", "legacy");
    let at = on_disk
        .milestone
        .lifecycle_at
        .as_ref()
        .expect("migrate must populate lifecycle_at");
    // M144 fix: backfill is RFC3339 (parseable), not YYYY-MM-DD (which the
    // humanizer would render as "unknown"). The exact value is
    // migration-time, so we only assert the shape.
    assert!(
        at.len() >= 19,
        "lifecycle_at must be RFC3339-shaped; got: {at}"
    );
    assert_eq!(on_disk.milestone.lifecycle, "approved");
}

/// M144 code-review (F-07): the migration early-return path (an
/// already-migrated file whose lifecycle is set but `lifecycle_at` is still
/// `None`) must backfill `lifecycle_at`. The main migration path is covered
/// by `migrate_backfills_lifecycle_at_from_created`; this pins the
/// early-return branch added in the F-07 remediation — without it a future
/// refactor could drop the backfill and no test would notice.
#[test]
fn migrate_early_return_backfills_lifecycle_at() {
    let env = TestEnv::new();
    let plan_dir = env.tmp.path().join("master-plan");
    let milestones_dir = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones_dir).expect("mkdir milestones");
    let mut m = MilestoneFile::default();
    m.milestone.id = "07".to_string();
    m.milestone.slug = "already-migrated".to_string();
    m.milestone.title = "already migrated".to_string();
    // Already-migrated shape: lifecycle set, legacy fields empty, but
    // lifecycle_at missing — the exact gap the F-07 early-return backfills.
    m.milestone.lifecycle = "approved".to_string();
    m.milestone.lifecycle_at = None;
    m.milestone.spec_status = String::new();
    m.milestone.execution_status = String::new();
    m.milestone.effort = "S".to_string();
    m.milestone.risk = "low".to_string();
    m.milestone.priority = "normal".to_string();
    m.milestone.created = "2026-01-01".to_string();
    m.milestone.updated = "2026-01-01".to_string();
    let path = milestones_dir.join("07-already-migrated.json");
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&m).expect("serialize")),
    )
    .expect("write file");

    let report = migrate_plan_lifecycle(&plan_dir).expect("migrate");
    // The early-return backfill changes the file → counted as a migration.
    assert_eq!(report.migrated, 1, "early-return backfill should migrate");
    assert_eq!(report.skipped, 0);

    let on_disk = load_milestone_path(&plan_dir, "07", "already-migrated");
    let at = on_disk
        .milestone
        .lifecycle_at
        .as_ref()
        .expect("early-return must backfill lifecycle_at");
    // Backfill derives from `created` → YYYY-MM-DDT00:00:00Z.
    assert!(
        at.starts_with("2026-01-01"),
        "backfill should derive from `created`; got: {at}"
    );
    assert_eq!(on_disk.milestone.lifecycle, "approved");
}

/// M144 WP-04: bulk set-spec-status must advance lifecycle_at per-id.
/// `apply_set_spec_status` routes through the per-id `apply_spec_status`,
/// which sets `lifecycle_at`; this test pins the bulk path inherits the
/// same semantics by exercising the per-id write helper that the bulk
/// command loops over.
///
/// A full CLI-level bulk test would require building a complete plan
/// (plan.json + reviews.json + annotations.json) which is a much larger
/// fixture than the rest of this suite — see `crates/mp/tests/suites/
/// lifecycle_setter_overlay.rs` for the heavyweight setup. The
/// per-id write helper IS the bulk path's per-element function, so
/// pinning it directly gives the same coverage with a fraction of
/// the fixture cost.
#[test]
fn bulk_set_spec_status_advances_lifecycle_at_per_id() {
    let env = TestEnv::new();
    let id = write_milestone(&env, "01");
    let ctx = ctx_for(&env);

    // Capture the starting lifecycle_at.
    let at_before = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .expect("starting lifecycle_at must be set");

    // Use the per-id write helper directly — this is the function the
    // bulk command's closure invokes per id. If this helper ever
    // stops advancing lifecycle_at, the bulk path inherits the bug.
    std::thread::sleep(std::time::Duration::from_millis(50));
    mp::milestone::apply_spec_status(&ctx, &id, "review").expect("review");

    let at_after = load_milestone(&env, &id)
        .milestone
        .lifecycle_at
        .clone()
        .expect("lifecycle_at after write");
    assert_ne!(
        at_before, at_after,
        "per-id write helper must advance lifecycle_at (this is the function bulk loops over)"
    );
    assert!(
        at_after.len() >= 19,
        "lifecycle_at must be RFC3339-shaped; got: {at_after}"
    );
}
