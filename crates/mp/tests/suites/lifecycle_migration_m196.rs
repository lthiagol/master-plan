//! M196 AC-03: the auto-migration rewrites `lifecycle: "done"` to
//! `"executed"` across plan files, idempotent. The legacy string
//! is preserved as a parse-time alias during the migration window
//! so half-migrated milestones still resolve the executor's
//! end-state correctly.

use mp::migrate::migrate_milestone_to_lifecycle;
use mp_model::MilestoneFile;

/// AC-03: a milestone with `lifecycle: "done"` is rewritten to
/// `"executed"` by `migrate_milestone_to_lifecycle` (idempotent).
#[test]
fn legacy_done_is_rewritten_to_executed() {
    let m = MilestoneFile {
        milestone: mp_model::MilestoneMeta {
            id: "10".into(),
            title: "M196 migration test".into(),
            slug: "m196-migration-test".into(),
            lifecycle: "done".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let migrated = migrate_milestone_to_lifecycle(m);
    assert_eq!(
        migrated.milestone.lifecycle, "executed",
        "expected `done` to be rewritten to `executed`; got `{}`",
        migrated.milestone.lifecycle
    );
}

/// AC-03: the rewrite is idempotent — re-running on a milestone
/// already at `"executed"` is a no-op.
#[test]
fn rewrite_is_idempotent() {
    let m = MilestoneFile {
        milestone: mp_model::MilestoneMeta {
            id: "11".into(),
            title: "M196 migration idempotent".into(),
            slug: "m196-migration-idempotent".into(),
            lifecycle: "executed".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let migrated = migrate_milestone_to_lifecycle(m);
    assert_eq!(migrated.milestone.lifecycle, "executed");
}

/// AC-03: `complete` stays terminal — the migration does NOT
/// rewrite terminal `complete` to `executed`.
#[test]
fn complete_stays_terminal() {
    let m = MilestoneFile {
        milestone: mp_model::MilestoneMeta {
            id: "12".into(),
            title: "M196 complete stays terminal".into(),
            slug: "m196-complete-stays-terminal".into(),
            lifecycle: "complete".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let migrated = migrate_milestone_to_lifecycle(m);
    assert_eq!(
        migrated.milestone.lifecycle, "complete",
        "`complete` is terminal — must NOT be rewritten"
    );
}

/// AC-03: review-flow states (`reviewed`, `remediation`,
/// `self-reviewed`) are preserved.
#[test]
fn review_flow_states_preserved() {
    for (id, lc) in [
        ("13", "reviewed"),
        ("14", "remediation"),
        ("15", "self-reviewed"),
    ] {
        let m = MilestoneFile {
            milestone: mp_model::MilestoneMeta {
                id: id.into(),
                title: format!("M196 review-flow {lc}"),
                slug: format!("m196-review-flow-{id}"),
                lifecycle: lc.into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let migrated = migrate_milestone_to_lifecycle(m);
        assert_eq!(
            migrated.milestone.lifecycle, lc,
            "{id}: lifecycle should remain `{lc}`; got `{}`",
            migrated.milestone.lifecycle
        );
    }
}
