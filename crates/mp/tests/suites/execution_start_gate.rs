//! Regression guards for execution-start gate wiring (M68 S4 follow-up).

use std::fs;

use crate::common::lib_api;
use crate::common::TestEnv;

/// Gate enforcement on `set-status in-progress` must live in the domain layer only.
#[test]
fn command_handler_does_not_call_validate_milestone_start_execution() {
    let handler = include_str!("../../src/commands/milestone.rs");
    assert!(
        !handler.contains("validate_milestone_start_execution"),
        "commands/milestone.rs must not call validate_milestone_start_execution; domain layer owns execution-start gates"
    );
    let domain = include_str!("../../src/milestone/complete.rs");
    assert!(
        domain.contains("validate_milestone_start_execution"),
        "milestone/complete.rs must call validate_milestone_start_execution when entering in-progress"
    );
}

fn create_milestone(env: &TestEnv, title: &str) -> String {
    let json = format!(
        r#"{{
        "title": "{title}",
        "intent": {{ "outcome": "Ship {title}" }},
        "problem": {{ "description": "Need {title}." }},
        "scope": {{
            "in_scope": ["{title}"],
            "out_of_scope": ["Other", "TBD"]
        }},
        "acceptance_criteria": [
            {{ "description": "{title} works", "verification": "cargo test" }}
        ]
    }}"#
    );
    let out = lib_api::run(
        env,
        &["milestone", "create", "--json", &json, "--format", "json"],
    );
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn patch_milestone(env: &TestEnv, id: &str, f: impl FnOnce(&mut String)) {
    let dir = env.tmp.path().join("master-plan/milestones");
    let entry = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().contains(id))
        .expect("find milestone file");
    let mut content = fs::read_to_string(entry.path()).unwrap();
    f(&mut content);
    fs::write(entry.path(), &content).unwrap();
}

#[test]
fn set_status_in_progress_enforces_g8_via_domain() {
    let env = TestEnv::new();
    let dep_id = create_milestone(&env, "exec-g8-dep");
    let child_id = create_milestone(&env, "exec-g8-child");
    assert!(env
        .run(&["milestone", "approve", &dep_id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&["milestone", "approve", &child_id, "--format", "json"])
        .status
        .success());
    patch_milestone(&env, &child_id, |c| {
        *c = c.replace(
            "\"depends_on\": []",
            &format!("\"depends_on\": [\"{dep_id}\"]"),
        );
    });

    let blocked = lib_api::run(
        &env,
        &[
            "milestone",
            "set-status",
            &child_id,
            "in-progress",
            "--format",
            "json",
        ],
    );
    assert!(
        !blocked.status.success(),
        "set-status in-progress should fail when dependency is not done"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&blocked.stdout),
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(
        combined.contains("G8"),
        "expected G8 from domain gate, got: {combined}"
    );

    assert!(env
        .run(&[
            "milestone",
            "set-status",
            &dep_id,
            "in-progress",
            "--format",
            "json"
        ])
        .status
        .success());
    patch_milestone(&env, &dep_id, |c| {
        *c = c.replace(
            "\"execution_status\": \"in-progress\"",
            "\"execution_status\": \"done\"",
        );
        *c = c.replace(
            "\"spec_status\": \"ready\"",
            "\"spec_status\": \"verified\"",
        );
    });
    assert!(env
        .run(&[
            "milestone",
            "set-status",
            &child_id,
            "in-progress",
            "--format",
            "json",
        ])
        .status
        .success());
}

#[test]
fn set_status_in_progress_enforces_g1_via_domain() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "exec-g1");
    let blocked = lib_api::run(
        &env,
        &[
            "milestone",
            "set-status",
            &id,
            "in-progress",
            "--format",
            "json",
        ],
    );
    assert!(!blocked.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&blocked.stdout),
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(combined.contains("G1"), "expected G1, got: {combined}");

    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "set-status",
            &id,
            "in-progress",
            "--format",
            "json",
        ])
        .status
        .success());
}

// --- M124 (M104 ER-3): transitions must route through `effective_*` helpers ---
//
// The bulk lifecycle migration clears the raw `spec_status`/`execution_status`
// fields and sets `lifecycle` as the authoritative state. Three transition/gate
// sites previously read the raw fields directly, making every migrated
// milestone non-reopenable and producing false G7/G8 errors. These tests pin
// the reroute through `effective_spec_status` / `effective_execution_status`.

/// Drop the raw legacy status field lines to emulate what
/// `migrate_plan_lifecycle` writes (lifecycle authoritative, legacy cleared).
fn clear_legacy_field(c: &str, field: &str) -> String {
    c.lines()
        .filter(|line| {
            let t = line.trim_start();
            !(t.starts_with(&format!("\"{field}\":")) && t.contains('"'))
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Set the lifecycle field (insert or replace) and clear raw legacy status
/// fields, emulating a post-migration milestone file.
fn migrate_to_lifecycle(env: &TestEnv, id: &str, lifecycle: &str) {
    patch_milestone(env, id, |c| {
        if c.contains("\"lifecycle\":") {
            // Replace existing value.
            let key = "\"lifecycle\": \"";
            let start = c.find(key).unwrap() + key.len();
            let end_rel = c[start..].find('"').unwrap();
            c.drain(start..start + end_rel);
            c.insert_str(start, lifecycle);
        } else {
            // Inject just after the milestone object opens.
            *c = c.replace(
                "\"milestone\": {",
                &format!("\"milestone\": {{\n    \"lifecycle\": \"{lifecycle}\","),
            );
        }
        *c = clear_legacy_field(c, "spec_status");
        *c = clear_legacy_field(c, "execution_status");
    });
}

/// Site 1: `reopen_milestone` reads `effective_execution_status`.
/// A migrated milestone (`lifecycle: "done"`, raw `execution_status` empty)
/// must be reopenable. Before the fix this bailed with
/// "reopen requires execution_status done".
#[test]
fn reopen_works_on_migrated_milestone() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "migrated-reopen");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "set-status",
            &id,
            "in-progress",
            "--format",
            "json"
        ])
        .status
        .success());
    // Drive to done+verified, then emulate the bulk migration.
    patch_milestone(&env, &id, |c| {
        *c = c.replace(
            "\"execution_status\": \"in-progress\"",
            "\"execution_status\": \"done\"",
        );
        *c = c.replace(
            "\"spec_status\": \"ready\"",
            "\"spec_status\": \"verified\"",
        );
    });
    migrate_to_lifecycle(&env, &id, "done");

    let out = lib_api::run(&env, &["milestone", "reopen", &id, "--format", "json"]);
    assert!(
        out.status.success(),
        "reopen should succeed on a migrated milestone; got: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["milestone"]["spec_status"], "ready");
    assert_eq!(v["milestone"]["execution_status"], "in-progress");
}

/// Site 2: `set_execution_status` done-arm reads `effective_spec_status`.
/// A migrated milestone at `lifecycle: "complete"` (raw `spec_status` empty)
/// derives `effective_spec_status == "verified"`, so `set-status done` must
/// succeed even though the raw field is empty. Before the fix this always
/// bailed with "execution_status done requires spec_status verified".
#[test]
fn set_status_done_uses_effective_spec_status_on_migrated_milestone() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "migrated-set-done");
    assert!(env
        .run(&["milestone", "approve", &id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&[
            "milestone",
            "set-status",
            &id,
            "in-progress",
            "--format",
            "json"
        ])
        .status
        .success());
    // Drive spec to verified, then emulate the bulk migration (raw fields cleared,
    // lifecycle authoritative). NOTE: do NOT call reopen here — reopen writes
    // the raw fields back, defeating the purpose of the migrated-state fixture.
    patch_milestone(&env, &id, |c| {
        *c = c.replace(
            "\"spec_status\": \"ready\"",
            "\"spec_status\": \"verified\"",
        );
        *c = c.replace(
            "\"execution_status\": \"in-progress\"",
            "\"execution_status\": \"done\"",
        );
    });
    migrate_to_lifecycle(&env, &id, "complete");
    // lifecycle=complete -> effective_spec_status == verified; raw field empty.
    // Re-affirm execution_status=done via set-status; the done-arm must read
    // effective_spec_status, not the empty raw field.
    let out = lib_api::run(
        &env,
        &["milestone", "set-status", &id, "done", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "set-status done should succeed when effective_spec_status is verified \
         (migrated milestone lifecycle=complete); got: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Site 3: `validate_milestone_start_execution` builds `done_ids` via
/// `effective_execution_status`. A child whose dependency was migrated
/// (`lifecycle: "complete"`, raw `execution_status` empty) must pass G8.
#[test]
fn set_status_in_progress_sees_migrated_dependency_as_done() {
    let env = TestEnv::new();
    let dep_id = create_milestone(&env, "migrated-dep");
    let child_id = create_milestone(&env, "migrated-dep-child");
    assert!(env
        .run(&["milestone", "approve", &dep_id, "--format", "json"])
        .status
        .success());
    assert!(env
        .run(&["milestone", "approve", &child_id, "--format", "json"])
        .status
        .success());
    patch_milestone(&env, &child_id, |c| {
        *c = c.replace(
            "\"depends_on\": []",
            &format!("\"depends_on\": [\"{dep_id}\"]"),
        );
    });
    // Complete the dep, then migrate it (clears raw execution_status).
    assert!(env
        .run(&[
            "milestone",
            "set-status",
            &dep_id,
            "in-progress",
            "--format",
            "json"
        ])
        .status
        .success());
    patch_milestone(&env, &dep_id, |c| {
        *c = c.replace(
            "\"execution_status\": \"in-progress\"",
            "\"execution_status\": \"done\"",
        );
        *c = c.replace(
            "\"spec_status\": \"ready\"",
            "\"spec_status\": \"verified\"",
        );
    });
    migrate_to_lifecycle(&env, &dep_id, "complete");

    // Before M124: `G8: dependency <dep> is not done` (raw execution_status empty).
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "set-status",
            &child_id,
            "in-progress",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "child set-status in-progress should succeed when dep is migrated-complete \
         (G8 must read effective_execution_status); got: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
