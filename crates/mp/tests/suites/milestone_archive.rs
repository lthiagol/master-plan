use std::fs;

use crate::common::lib_api;
use crate::common::TestEnv;

fn create_minimal_milestone(env: &TestEnv, title: &str) -> String {
    let json = format!(
        r#"{{
            "title": "{title}",
            "intent": {{ "outcome": "Test outcome." }},
            "scope": {{
                "in_scope": ["Item A"],
                "out_of_scope": ["Item B", "Item C"]
            }}
        }}"#
    );
    lib_api::run_json(
        env,
        &["milestone", "create", "--json", &json, "--format", "json"],
    )["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn milestone_archive_restore_round_trip() {
    let env = TestEnv::new();
    let id = create_minimal_milestone(&env, "Archive Test");

    let show = lib_api::run_json(&env, &["show", "milestone", &id, "--format", "json"]);
    assert_eq!(show["milestone"]["id"], id);

    let archive = lib_api::run_json(&env, &["milestone", "archive", &id, "--format", "json"]);
    assert_eq!(archive["ok"], true);
    assert_eq!(archive["archived"], id);

    let list = lib_api::run_json(&env, &["list", "milestones", "--format", "json"]);
    let ids: Vec<&str> = list["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert!(
        !ids.contains(&id.as_str()),
        "archived milestone should not appear in list"
    );

    let list_archived = lib_api::run_json(
        &env,
        &[
            "list",
            "milestones",
            "--include-archived",
            "--format",
            "json",
        ],
    );
    let archived_ids: Vec<&str> = list_archived["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert!(
        archived_ids.contains(&id.as_str()),
        "archived milestone should appear with --include-archived"
    );

    let restore = lib_api::run_json(&env, &["milestone", "restore", &id, "--format", "json"]);
    assert_eq!(restore["ok"], true);
    assert_eq!(restore["restored"], id);

    let list_after = lib_api::run_json(&env, &["list", "milestones", "--format", "json"]);
    let after_ids: Vec<&str> = list_after["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert!(
        after_ids.contains(&id.as_str()),
        "restored milestone should appear in list again"
    );
}

#[test]
fn show_archived_milestone_from_archive_dir() {
    let env = TestEnv::new();
    let id = create_minimal_milestone(&env, "Show Archived");

    lib_api::run_json(&env, &["milestone", "archive", &id, "--format", "json"]);

    let show = lib_api::run_json(&env, &["show", "milestone", &id, "--format", "json"]);
    assert_eq!(show["milestone"]["id"], id);
    assert_eq!(show["milestone"]["title"], "Show Archived");
}

#[test]
fn archive_and_restore_preserves_no_index_entry() {
    let env = TestEnv::new();
    let id = create_minimal_milestone(&env, "Index Check");

    let milestone_dir = env.tmp.path().join("master-plan/milestones");
    let active_before: Vec<_> = fs::read_dir(&milestone_dir)
        .expect("read milestones dir")
        .collect();
    assert_eq!(
        active_before.len(),
        1,
        "one active milestone file before archive"
    );

    lib_api::run_json(&env, &["milestone", "archive", &id, "--format", "json"]);

    let active_after: Vec<_> = fs::read_dir(&milestone_dir)
        .expect("read milestones dir")
        .collect();
    assert_eq!(
        active_after.len(),
        0,
        "no active milestone files after archive"
    );

    let archive_dir = env.tmp.path().join("master-plan/archive/milestones");
    let archived_files: Vec<_> = fs::read_dir(&archive_dir)
        .expect("read archive dir")
        .collect();
    assert_eq!(archived_files.len(), 1, "one archived milestone file");

    lib_api::run_json(&env, &["milestone", "restore", &id, "--format", "json"]);

    let active_restored: Vec<_> = fs::read_dir(&milestone_dir)
        .expect("read milestones dir")
        .collect();
    assert_eq!(active_restored.len(), 1, "milestone restored to active dir");
}

#[test]
fn milestone_archive_and_purge() {
    let env = TestEnv::new();
    let id = create_minimal_milestone(&env, "Purge Test");

    lib_api::run_json(&env, &["milestone", "archive", &id, "--format", "json"]);

    let archive_dir = env.tmp.path().join("master-plan/archive/milestones");
    let archived_before: Vec<_> = fs::read_dir(&archive_dir)
        .expect("read archive dir")
        .collect();
    assert_eq!(
        archived_before.len(),
        1,
        "archived file exists before purge"
    );

    lib_api::run_json(&env, &["milestone", "purge", &id, "--format", "json"]);

    let archived_after: Vec<_> = fs::read_dir(&archive_dir)
        .expect("read archive dir")
        .collect();
    assert_eq!(archived_after.len(), 0, "no archived files after purge");
}

#[test]
fn archive_on_milestone_delete_wired() {
    let env = TestEnv::new();
    let id = create_minimal_milestone(&env, "Delete Archive Check");

    let del = lib_api::run(&env, &["milestone", "delete", &id, "--format", "json"]);
    assert!(
        del.status.success(),
        "delete without --force should succeed"
    );
    let result: serde_json::Value = serde_json::from_slice(&del.stdout).unwrap();
    assert_eq!(
        result["archived"], id,
        "delete should archive when config.archive_on_milestone_delete is true"
    );
    assert!(
        result["note"]
            .as_str()
            .unwrap_or("")
            .contains("archive_on_milestone_delete"),
        "note mentions config"
    );

    let archive_dir = env.tmp.path().join("master-plan/archive/milestones");
    let archived: Vec<_> = fs::read_dir(&archive_dir)
        .expect("read archive dir")
        .collect();
    assert_eq!(
        archived.len(),
        1,
        "milestone was archived, not hard-deleted"
    );
}

#[test]
fn archive_delete_force_skips_archive() {
    let env = TestEnv::new();
    let id = create_minimal_milestone(&env, "Force Delete");

    let del = lib_api::run(
        &env,
        &["milestone", "delete", &id, "--force", "--format", "json"],
    );
    assert!(del.status.success());
    let result: serde_json::Value = serde_json::from_slice(&del.stdout).unwrap();
    assert_eq!(result["deleted"], id, "delete --force should hard-delete");

    let archive_dir = env.tmp.path().join("master-plan/archive/milestones");
    let archived: Vec<_> = fs::read_dir(&archive_dir)
        .expect("read archive dir")
        .collect();
    assert_eq!(archived.len(), 0, "no archived files after --force delete");
}

#[test]
fn archived_milestone_excluded_from_validate() {
    let env = TestEnv::new();
    let id = create_minimal_milestone(&env, "Validate Exclusion");

    let valid_before = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        valid_before.status.success(),
        "validate passes before archive"
    );

    lib_api::run_json(&env, &["milestone", "archive", &id, "--format", "json"]);

    let valid_after = lib_api::run(&env, &["validate", "--format", "json"]);
    assert!(
        valid_after.status.success(),
        "validate passes with archived milestone (no stale index refs)"
    );
}

#[test]
fn list_archived_via_archive_target() {
    let env = TestEnv::new();
    let id = create_minimal_milestone(&env, "List Archived");

    lib_api::run_json(&env, &["milestone", "archive", &id, "--format", "json"]);

    let archived_list = lib_api::run_json(
        &env,
        &[
            "list",
            "archived",
            "--entity-type",
            "milestone",
            "--format",
            "json",
        ],
    );
    let entries: Vec<&str> = archived_list["archived"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["entity_id"].as_str())
        .collect();
    assert!(
        entries.contains(&id.as_str()),
        "archived milestone appears in list archived"
    );
}
