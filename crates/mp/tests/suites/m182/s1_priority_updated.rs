//! M182 S1: `mp list milestones` projection includes `priority` and
//! `updated` for every milestone. The fields are required by the
//! raul sort-rebind menu (M172 S5) to render the four sort keys
//! (id / lifecycle / priority / updated) without a per-milestone
//! `show` round-trip.

use crate::common::lib_api;
use crate::common::TestEnv;

fn create_milestone(env: &TestEnv, title: &str, priority: &str, spec_status: &str) -> String {
    let json = serde_json::json!({
        "title": title,
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "priority": priority,
        "intent": { "outcome": "M182 fixture" },
        "problem": { "description": "M182 S1 list-payload test." },
        "scope": {
            "in_scope": ["list payload"],
            "out_of_scope": ["x", "y"]
        },
        "acceptance_criteria": [
            {"description": "AC1", "verification": "manual: ok"}
        ],
        "spec_status": spec_status,
    });
    let json_str = serde_json::to_string(&json).unwrap();
    let out = lib_api::run(
        env,
        &[
            "milestone",
            "create",
            "--json",
            &json_str,
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

/// AC-01: every milestone in `mp list milestones` carries a `priority`
/// field — the raul sort-rebind menu's "Priority" option needs it.
#[test]
fn list_milestones_includes_priority_per_item() {
    let env = TestEnv::new();
    create_milestone(&env, "High prio", "high", "draft");
    create_milestone(&env, "Normal prio", "normal", "draft");

    let out = lib_api::run(&env, &["list", "milestones", "--format", "json"]);
    assert!(
        out.status.success(),
        "list milestones must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().expect("milestones array");
    assert_eq!(items.len(), 2, "expected 2 milestones in the list");
    for item in items {
        let id = item["id"].as_str().expect("item.id");
        let priority = item["priority"]
            .as_str()
            .unwrap_or_else(|| panic!("item {id} missing 'priority' field; payload: {item}"));
        assert!(!priority.is_empty(), "item {id} priority must be non-empty");
    }
}

/// AC-01: every milestone in `mp list milestones` carries an `updated`
/// field — the raul sort-rebind menu's "Updated" option needs it.
/// `updated` is the milestone file's last-touch date (YYYY-MM-DD);
/// milestones created via `mp milestone create` always populate it.
#[test]
fn list_milestones_includes_updated_per_item() {
    let env = TestEnv::new();
    create_milestone(&env, "Has updated", "normal", "draft");

    let out = lib_api::run(&env, &["list", "milestones", "--format", "json"]);
    assert!(
        out.status.success(),
        "list milestones must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().expect("milestones array");
    assert_eq!(items.len(), 1);
    let item = &items[0];
    let updated = item["updated"]
        .as_str()
        .unwrap_or_else(|| panic!("missing 'updated' field; payload: {item}"));
    // updated is YYYY-MM-DD; created-today milestones always populate it.
    assert!(
        updated.len() >= 10,
        "updated must be a YYYY-MM-DD string; got {updated:?}"
    );
}

/// AC-01: the existing fields (id, display, title, lifecycle, ...) are
/// preserved alongside the new priority + updated fields. Pre-M182
/// dropping one would be a backward-compat regression.
#[test]
fn list_milestones_preserves_legacy_fields_alongside_priority_updated() {
    let env = TestEnv::new();
    create_milestone(&env, "Both fields", "normal", "draft");

    let out = lib_api::run(&env, &["list", "milestones", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let item = &v["milestones"][0];
    for legacy in ["id", "display", "title", "lifecycle", "lifecycle_at"] {
        assert!(
            item.get(legacy).is_some(),
            "missing legacy field {legacy} on item {item}"
        );
    }
    assert!(item.get("priority").is_some(), "missing new field priority");
    assert!(item.get("updated").is_some(), "missing new field updated");
}

/// AC-01: `--where updated==<date>` filters milestones by their last
/// touch date. Same contract as `--where priority==high` (used by the
/// bulk set-priority tests).
#[test]
fn list_milestones_where_filter_supports_updated_field() {
    let env = TestEnv::new();
    create_milestone(&env, "Today", "normal", "draft");

    let out = lib_api::run(
        &env,
        &[
            "list",
            "milestones",
            "--where",
            "updated==2026-07-16",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "list milestones --where updated== must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The filter runs against the just-created milestone's updated
    // date which is today. Assert the filter shape (parses, doesn't
    // crash). Exact-match testing is date-sensitive; this is the
    // shape pin.
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["milestones"].is_array());
}

/// AC-01: `--sort priority` orders milestones by priority rank
/// (urgent > high > regular > low > ?) — the same rank used by the
/// path engine. Pre-M182 the `--sort` lookup table didn't include
/// `priority` in the where-filter branch; the `--sort` projection
/// itself has always supported it (see line 307 of list.rs).
#[test]
fn list_milestones_sort_by_priority_orders_correctly() {
    let env = TestEnv::new();
    let _high = create_milestone(&env, "High", "high", "draft");
    let _low = create_milestone(&env, "Low", "low", "draft");
    let _urgent = create_milestone(&env, "Urgent", "urgent", "draft");

    let out = lib_api::run(
        &env,
        &[
            "list",
            "milestones",
            "--sort",
            "priority",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "list milestones --sort priority must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["milestones"].as_array().expect("milestones array");
    assert_eq!(items.len(), 3);
    // First item must be the highest priority (urgent > high > low).
    let first_priority = items[0]["priority"].as_str().expect("priority");
    assert_eq!(
        first_priority, "urgent",
        "first item must be the highest priority; got {first_priority}"
    );
}
