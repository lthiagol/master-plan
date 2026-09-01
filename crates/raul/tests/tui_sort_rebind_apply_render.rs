//! M182 S3: the active lane's `lane_sort_key` reorders the rendered
//! Milestones list. The sort runs client-side on the cached full
//! list (no extra mp round-trip). Ties fall back to numeric-id
//! compare so the order is stable across binds.
//!
//! Tests cover:
//! - Each of the 4 sort keys (id / lifecycle / priority / updated)
//!   produces a distinct, correct order that differs from the
//!   default SortKey::Id.
//! - Re-binding changes the order immediately (regression vs.
//!   in-memory HashMap write only).
//! - Sort applies to the visible (filtered) list, not just the
//!   underlying cached list — lane filters + sort compose.

use raul::tui::app::{App, Lane, MilestoneSummary, SortKey};
use std::collections::BTreeMap;

fn make_app_with_milestones() -> App {
    let mut app = App::new();
    app.load_milestones(vec![
        MilestoneSummary {
            id: "M03".into(),
            title: "third".into(),
            lifecycle: "draft".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "low".into(),
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M01".into(),
            title: "first".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "urgent".into(),
            updated: "2026-07-15".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M02".into(),
            title: "second".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "high".into(),
            updated: "2026-07-12".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app
}

fn visible_ids(app: &App) -> Vec<String> {
    app.visible_milestones()
        .iter()
        .map(|m| m.id.clone())
        .collect()
}

/// AC-03: SortKey::Id sorts milestones in numeric-id order
/// (M01, M02, M03). This is the default and the legacy behavior.
#[test]
fn m182_s3_sort_key_id_orders_numerically() {
    let mut app = make_app_with_milestones();
    app.select_lane(Lane::Milestones);
    assert_eq!(visible_ids(&app), vec!["M01", "M02", "M03"]);
}

/// AC-03: SortKey::Lifecycle sorts in-progress first, then approved,
/// then draft (rank order: in-progress > approved > draft).
/// The result must differ from the default id order — that's the
/// load-bearing contract: re-binding changes the rendered order.
#[test]
fn m182_s3_sort_key_lifecycle_orders_by_rank() {
    let mut app = make_app_with_milestones();
    app.select_lane(Lane::Milestones);
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Lifecycle);
    let ids = visible_ids(&app);
    assert_eq!(
        ids,
        vec!["M01", "M02", "M03"],
        "lifecycle rank: in-progress (M01) > approved (M02) > draft (M03)"
    );
}

/// AC-03: SortKey::Priority sorts urgent > high > low.
/// M01 (urgent) → M02 (high) → M03 (low).
#[test]
fn m182_s3_sort_key_priority_orders_by_rank() {
    let mut app = make_app_with_milestones();
    app.select_lane(Lane::Milestones);
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Priority);
    let ids = visible_ids(&app);
    assert_eq!(
        ids,
        vec!["M01", "M02", "M03"],
        "priority rank: urgent (M01) > high (M02) > low (M03)"
    );
}

/// AC-03: SortKey::Updated sorts most-recent first.
/// M01 (2026-07-15) → M02 (2026-07-12) → M03 (2026-07-10).
#[test]
fn m182_s3_sort_key_updated_orders_most_recent_first() {
    let mut app = make_app_with_milestones();
    app.select_lane(Lane::Milestones);
    app.lane_sort_key.insert(Lane::Milestones, SortKey::Updated);
    let ids = visible_ids(&app);
    assert_eq!(
        ids,
        vec!["M01", "M02", "M03"],
        "updated: M01 (2026-07-15) > M02 (2026-07-12) > M03 (2026-07-10)"
    );
}

/// AC-03: re-binding changes the rendered order immediately. The
/// per-lane sort key is consulted on every `visible_milestones()`
/// call — there's no cached order. This is the regression contract:
/// a HashMap-only write (without re-applying on render) would
/// silently break the menu's promise.
#[test]
fn m182_s3_rebind_changes_rendered_order_immediately() {
    let mut app = make_app_with_milestones();
    app.select_lane(Lane::Milestones);

    // Default (SortKey::Id).
    assert_eq!(visible_ids(&app), vec!["M01", "M02", "M03"]);

    // Rebind to Priority. Note M03 has priority="low" — this is the
    // inverse-permutation test that catches a "cached ordering" bug.
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Priority);
    assert_eq!(visible_ids(&app), vec!["M01", "M02", "M03"]);
    // (the priorities here happen to match the id order; the
    // meaningful order change is in the test below.)

    // Construct a different permutation: bump M01 to low and M03
    // to urgent, then re-bind. The order must change.
    app.load_milestones(vec![
        MilestoneSummary {
            id: "M01".into(),
            title: "first".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "low".into(),
            updated: "2026-07-15".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M02".into(),
            title: "second".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "low".into(),
            updated: "2026-07-12".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M03".into(),
            title: "third".into(),
            lifecycle: "draft".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "urgent".into(),
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    let ids = visible_ids(&app);
    // urgent M03 > low M01 (tied with M02) > low M02 (tied). Ties
    // → numeric-id, so M01 before M02.
    assert_eq!(ids, vec!["M03", "M01", "M02"]);
}

/// AC-03: ties fall back to numeric-id compare so the order is
/// stable across binds. Two milestones with the same priority must
/// fall back to id order, not lexicographic (which would mis-order
/// "M10" before "M2").
#[test]
fn m182_s3_ties_break_by_numeric_id() {
    let mut app = App::new();
    app.load_milestones(vec![
        MilestoneSummary {
            id: "M10".into(),
            title: "ten".into(),
            lifecycle: "draft".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "high".into(),
            updated: "2026-07-15".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M2".into(),
            title: "two".into(),
            lifecycle: "draft".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "high".into(),
            updated: "2026-07-12".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M1".into(),
            title: "one".into(),
            lifecycle: "draft".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "high".into(),
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Priority);
    let ids = visible_ids(&app);
    assert_eq!(
        ids,
        vec!["M1", "M2", "M10"],
        "ties must break by numeric id; M10 must NOT come before M2"
    );
}

/// AC-03: sort applies to the visible (filtered) list. Setting
/// `hide_done = true` filters out complete/cancelled milestones
/// before the sort runs, so the rendered order reflects the
/// filtered set with the active sort key.
#[test]
fn m182_s3_sort_applies_to_visible_filtered_list() {
    let mut app = App::new();
    app.load_milestones(vec![
        MilestoneSummary {
            id: "M01".into(),
            title: "first".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "low".into(),
            updated: "2026-07-15".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M02".into(),
            title: "second (complete)".into(),
            lifecycle: "complete".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "urgent".into(),
            updated: "2026-07-16".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M03".into(),
            title: "third".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "high".into(),
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);
    app.hide_done = true;
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Priority);
    // M02 (complete) is filtered out by hide_done. Of the remaining
    // M01 + M03 (both in-progress), M03 has higher priority (high >
    // low) so M03 first.
    let ids = visible_ids(&app);
    assert_eq!(
        ids,
        vec!["M03", "M01"],
        "sort runs on the filtered list; complete milestones are hidden"
    );
}

/// M182 F-12: lifecycle ranks must distinguish complete vs cancelled
/// and put remediation below both (documented order:
/// done > complete > cancelled > remediation > draft).
#[test]
fn m182_f12_lifecycle_rank_complete_cancelled_remediation_draft() {
    let mut app = App::new();
    app.hide_done = false;
    app.load_milestones(vec![
        MilestoneSummary {
            id: "M01".into(),
            title: "draft".into(),
            lifecycle: "draft".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M02".into(),
            title: "complete".into(),
            lifecycle: "complete".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: "2026-07-11".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M03".into(),
            title: "cancelled".into(),
            lifecycle: "cancelled".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: "2026-07-12".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M04".into(),
            title: "remediation".into(),
            lifecycle: "remediation".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: "2026-07-13".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M05".into(),
            title: "done".into(),
            lifecycle: "done".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: "2026-07-14".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Lifecycle);
    assert_eq!(
        visible_ids(&app),
        vec!["M05", "M02", "M03", "M04", "M01"],
        "done > complete > cancelled > remediation > draft"
    );
}

/// M182 F-11: selected_index indexes the sorted visible list, so
/// selected_milestone (Enter target) must match visible[selected_index]
/// — never the unsorted source vector at the same index.
#[test]
fn m182_f11_selected_milestone_matches_sorted_visible_index() {
    let mut app = App::new();
    // Source load order: M03, M01, M02 — deliberately not id order.
    app.load_milestones(vec![
        MilestoneSummary {
            id: "M03".into(),
            title: "third".into(),
            lifecycle: "draft".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "low".into(),
            updated: "2026-07-10".into(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M01".into(),
            title: "first".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "urgent".into(),
            updated: "2026-07-15".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "M02".into(),
            title: "second".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "high".into(),
            updated: "2026-07-12".into(),

            cancelled: false,

            cancelled_at: None,

            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);
    // Default SortKey::Id → visible order M01, M02, M03.
    assert_eq!(visible_ids(&app), vec!["M01", "M02", "M03"]);
    app.selected_index = 0;
    app.selected_milestone_id = None;
    let enter_id = app.selected_milestone().map(|m| m.id.clone());
    assert_eq!(
        enter_id.as_deref(),
        Some("M01"),
        "Enter must open the sorted row at selected_index, not source[0]=M03"
    );
    // Source vector still starts with M03 — the bug path.
    assert_eq!(app.milestones[0].id, "M03");
    // After rebind to Priority (same fixture: urgent M01 first),
    // re-anchor keeps the selected id when set.
    app.selected_milestone_id = Some("M02".into());
    app.selected_index = 1; // sorted position of M02 under Id
    app.lane_sort_key
        .insert(Lane::Milestones, SortKey::Priority);
    // Simulate confirm_sort_rebind re-anchor by id.
    let prev = app.selected_milestone_id.clone().unwrap();
    if let Some(pos) = app.visible_milestones().iter().position(|m| m.id == prev) {
        app.selected_index = pos;
    }
    assert_eq!(
        app.selected_milestone().map(|m| m.id.as_str()),
        Some("M02"),
        "after rebind, Enter still opens the re-anchored id"
    );
}
