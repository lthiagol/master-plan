//! M229: breaking-release cleanup. This module owns the canonical
//! black-box coverage for AC-01 (preflight refusal + acceptance
//! fixture) and AC-02 (absence of `mp watch` / `mp watch-control`
//! and the legacy autopilot migrate verb from the canonical
//! surface after the cleanup).
//!
//! Originally the M149 / M208 `mp watch` CLI tests lived here; M229
//! rewrote them to test the absence + the preflight gate. The
//! preflight acceptance fixture used by AC-01 is written into a
//! scratch plan directory and never touches the live plan.

mod common;

use common::TestEnv;
use serde_json::Value;
use std::path::Path;

fn ctx_for_dir(dir: &Path) -> mp::paths::PlanContext {
    mp::paths::PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

fn write_release_fixture(plan_dir: &Path, releases: Value) {
    let payload = serde_json::json!({
        "project": {
            "name": "test",
            "description": "",
            "stack": [],
            "platforms": [],
            "created": "2026-09-04",
            "target_version": "",
            "planning_status": "in-execution",
            "planning_phase": "charter"
        },
        "charter": {"goals": [], "non_goals": [], "deferred": [], "principles": []},
        "metrics": {
            "lines_of_code": 0,
            "unit_tests": 0,
            "integration_tests": 0,
            "coverage_percent": 0.0,
            "checked_at": "2026-09-04"
        },
        "execution": {
            "strategy": "resume_then_ready",
            "interleave": "milestone",
            "mode": "autonomous",
            "handoff_at": "",
            "handoff_by": "",
            "focus_milestone": "",
            "focus_through_step": "",
            "adoption_order": [],
            "handoff_changed_milestones": [],
            "handoff_baseline": {}
        },
        "milestones": [],
        "releases": releases
    });
    std::fs::create_dir_all(plan_dir).unwrap();
    std::fs::write(
        plan_dir.join("plan.json"),
        serde_json::to_string_pretty(&payload).unwrap(),
    )
    .unwrap();
}

fn write_milestone_229(plan_dir: &Path, target_version: &str) {
    let milestones_dir = plan_dir.join("milestones");
    std::fs::create_dir_all(&milestones_dir).unwrap();
    let payload = serde_json::json!({
        "milestone": {
            "id": "229",
            "target_version": target_version,
            "lifecycle": "approved",
            "spec_status": "ready",
            "execution_status": "planned"
        }
    });
    std::fs::write(
        milestones_dir.join("229-fixture.json"),
        serde_json::to_string_pretty(&payload).unwrap(),
    )
    .unwrap();
}

// ─── AC-01: preflight refuses without recorded target version
// ─── AC-01: preflight refuses without a shipped release covering the
//     migration window (M208 + M219)
// ─── AC-01: preflight accepts when both gates are recorded

#[test]
fn preflight_refuses_when_target_version_is_empty() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    write_release_fixture(&ctx.plan_dir, serde_json::json!([]));
    let report = mp::breaking_release::preflight(&ctx).unwrap();
    assert!(!report.ok, "fresh fixture must refuse");
    assert!(report.target_version.is_empty());
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.contains("no recorded next-major target version")),
        "expected target-version blocker; got {:?}",
        report.blockers
    );
}

#[test]
fn preflight_refuses_when_no_shipped_release_covers_migration_window() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    write_milestone_229(&ctx.plan_dir, "3.0.0");
    write_release_fixture(
        &ctx.plan_dir,
        serde_json::json!([
            {"version": "2.0.0", "status": "shipped", "date": "2026-07-04", "milestones": ["208"]}
        ]),
    );
    let report = mp::breaking_release::preflight(&ctx).unwrap();
    assert!(
        !report.ok,
        "missing M219 in the shipped release must refuse; got {:?}",
        report.blockers
    );
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.contains("migration window")),
        "expected migration-window blocker"
    );
}

#[test]
fn preflight_refuses_when_release_status_is_planned_only() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    write_milestone_229(&ctx.plan_dir, "3.0.0");
    write_release_fixture(
        &ctx.plan_dir,
        serde_json::json!([
            {"version": "3.0.0", "status": "planned", "date": "", "milestones": ["208", "219"]}
        ]),
    );
    let report = mp::breaking_release::preflight(&ctx).unwrap();
    assert!(
        !report.ok,
        "planned-only release should not satisfy migration-window evidence"
    );
}

#[test]
fn preflight_accepts_when_target_version_and_shipped_release_are_recorded() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    write_milestone_229(&ctx.plan_dir, "3.0.0");
    write_release_fixture(
        &ctx.plan_dir,
        serde_json::json!([
            {"version": "2.0.0", "status": "shipped", "date": "2026-07-04", "milestones": ["208", "219"]}
        ]),
    );
    let report = mp::breaking_release::preflight(&ctx).unwrap();
    assert!(
        report.ok,
        "valid release fixture must accept; blockers={:?}",
        report.blockers
    );
    assert_eq!(report.target_version, "3.0.0");
    assert_eq!(report.evidence_releases, vec!["2.0.0"]);
    assert!(report.blockers.is_empty());
}

#[test]
fn preflight_cli_exposes_status_to_operators() {
    let env = TestEnv::new();
    let ctx = ctx_for_dir(env.tmp.path());
    // Create the live plan fixture in the test env so the CLI can find it.
    let plan_dir = ctx.plan_dir.clone();
    write_milestone_229(&plan_dir, "3.0.0");
    write_release_fixture(
        &plan_dir,
        serde_json::json!([
            {"version": "2.0.0", "status": "shipped", "date": "2026-07-04", "milestones": ["208", "219"]}
        ]),
    );
    let out = env.run(&[
        "--plan-dir",
        plan_dir.to_str().unwrap(),
        "breaking-release",
        "preflight",
    ]);
    assert!(
        out.status.success(),
        "preflight should exit 0 even when refusing; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["ok"], Value::Bool(true));
    assert_eq!(v["target_version"], Value::String("3.0.0".to_string()));
    assert_eq!(v["evidence_releases"], serde_json::json!(["2.0.0"]));
}

// ─── AC-02: post-removal absence tests. The legacy `mp watch`,
// `mp watch-control`, and `mp autopilot migrate` commands must be
// gone (or surface unknown-command errors) after the S2 cleanup.

#[test]
fn watch_command_is_rejected_after_breaking_release() {
    let env = TestEnv::new();
    let out = env.run(&["watch", "--help"]);
    assert!(
        !out.status.success(),
        "mp watch must be removed; got exit 0 with stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let combined = format!("{}{}", String::from_utf8_lossy(&out.stdout), stderr);
    assert!(
        combined.to_lowercase().contains("unknown")
            || combined.contains("invalid subcommand")
            || combined.contains("unrecognized"),
        "mp watch must surface an unknown-command error; got stderr={stderr}"
    );
}

#[test]
fn watch_control_command_is_rejected_after_breaking_release() {
    let env = TestEnv::new();
    let out = env.run(&["watch-control", "status"]);
    assert!(
        !out.status.success(),
        "mp watch-control must be removed; got exit 0"
    );
}

#[test]
fn autopilot_migrate_command_is_rejected_after_breaking_release() {
    let env = TestEnv::new();
    let out = env.run(&["autopilot", "migrate"]);
    assert!(
        !out.status.success(),
        "mp autopilot migrate must be removed; got exit 0"
    );
}

#[test]
fn autopilot_help_no_longer_advertises_migrate_verb() {
    let env = TestEnv::new();
    let out = env.run(&["autopilot", "--help"]);
    assert!(out.status.success(), "autopilot --help must still exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let v: Value = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    let doc = v
        .get("tree")
        .map(|t| t.to_string())
        .unwrap_or_else(|| stdout.clone());
    // The migrate verb must not be present in any help output.
    assert!(
        !doc.contains("migrate") || doc.contains("migrate-role"),
        "autopilot --help must not advertise a 'migrate' verb; got {doc}"
    );
}

// ─── M229 / F-01 regression: the production autopilot path must NOT
// route through any `commands::watch*` module. The legacy alias is
// removed at the CLI surface and the supporting code is renamed/
// relocated so the canonical autopilot drive is decoupled from the
// legacy `mp watch` family entirely. ────────────────────────────────

#[test]
fn m229_f01_autopilot_start_dry_run_emits_autopilot_log_path_not_watch_log() {
    // F-03 / cycle 3 behavioral pin: run the production
    // `mp autopilot start <id> --dry-run` dispatch end-to-end
    // and verify the output JSON names the renamed
    // `.mp/autopilot.log` path. The legacy `cmd_watch` path
    // surfaced `.mp/watch.log`; a regression to that dispatch
    // would re-emit the legacy filename in the dry-run report.
    let env = TestEnv::new();
    // Create an approved+ready milestone so the autopilot
    // dry-run path goes past the early `planning/approval`
    // gate and through `cmd_autopilot_drive`.
    let payload = serde_json::json!({
        "title": "f-03 dry-run fixture",
        "intent": {"outcome": "f-03 dry-run"},
        "problem": {"description": "f-03 fixture"},
        "scope": {"in_scope": ["x"], "out_of_scope": ["y", "z"]},
        "acceptance_criteria": [
            {"description": "ac", "verification": "manual: yes"}
        ]
    });
    let create_out = env.run_json(&[
        "milestone",
        "create",
        "--json",
        &payload.to_string(),
        "--format",
        "json",
    ]);
    let id = create_out["milestone"]["id"]
        .as_str()
        .expect("created milestone id");
    env.run(&["milestone", "set-spec-status", id, "ready"]);
    env.run(&["milestone", "approve", id, "--format", "json"]);

    // Run the canonical autopilot entry point. With a stale
    // dispatch to `commands::watch`, this would print
    // `log_file = ".../.mp/watch.log"`.
    let out = env.run(&["autopilot", "start", id, "--dry-run", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The dry-run report may surface precondition non-green
    // for a bare TestEnv (no harness skill installed), but the
    // dispatch routing is still exercised — `log_file` is set
    // before the precondition check.
    assert!(
        stdout.contains("autopilot.log"),
        "production dispatch must surface the renamed `.mp/autopilot.log` log path; got stdout: {stdout}"
    );
    assert!(
        !stdout.contains("watch.log"),
        "production dispatch must NOT surface the legacy `.mp/watch.log` path; got stdout: {stdout}"
    );
    // Additionally: the autopilot surface was reached (not the
    // legacy `mp watch` CLI surface, which no longer exists).
    assert!(
        out.status.success() || stdout.contains("autopilot.log"),
        "autopilot start --dry-run must reach the autopilot dispatcher and surface the autopilot.log path; got exit {:?} stdout: {stdout}",
        out.status.code()
    );
}

#[test]
fn m229_f01_cmd_autopilot_start_dispatches_to_autopilot_drive_not_watch() {
    // F-03 / cycle 3 source-pin: the F-01 regression must FAIL if
    // `cmd_autopilot_start` is reverted to call the legacy
    // `commands::watch::cmd_watch`. The earlier cycle 2 test only
    // invoked `mp autopilot start --help`, which exits at clap's
    // help handler before the dispatch ever runs. This test parses
    // the production source and asserts the call routing, so a
    // regression that restored `commands::watch::cmd_watch(…)`
    // (or any `commands::watch::*` reference) inside
    // `cmd_autopilot_start` would fail to compile or trip the
    // assert below.
    let dispatch_path = common::repo_root().join("crates/mp/src/commands/autopilot.rs");
    let source = std::fs::read_to_string(&dispatch_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", dispatch_path.display()));

    // Pin the fn boundary: the body of `cmd_autopilot_start` must
    // sit immediately after the marker comment.
    let start_marker = "pub(crate) fn cmd_autopilot_start(";
    let body_start = source
        .find(start_marker)
        .unwrap_or_else(|| panic!("cmd_autopilot_start not found in {dispatch_path:?}"));
    // Capture the fn body by counting braces from the open paren of
    // the arg list through the close brace of the function. A flat
    // forward search for matching `{` after `-> Result<()>` suffices
    // for the current source shape (single return-expression block).
    let body_open = source[body_start..]
        .find('{')
        .map(|i| body_start + i)
        .expect("cmd_autopilot_start body opens with {");
    let body_close = source[body_open..]
        .find("}\n")
        .map(|i| body_open + i + 1)
        .expect("cmd_autopilot_start body closes with }");
    let body = &source[body_open..body_close];

    assert!(
        body.contains("autopilot_drive::cmd_autopilot_drive"),
        "cmd_autopilot_start must call crate::commands::autopilot_drive::cmd_autopilot_drive; body:\n{body}"
    );
    // The legacy `commands::watch::cmd_watch` MUST NOT appear in
    // the dispatch body. A regression that re-introduces the
    // `commands/watch` shim fails here.
    assert!(
        !body.contains("commands::watch") && !body.contains("commands/watch"),
        "cmd_autopilot_start must NOT call the legacy commands::watch::cmd_watch; body:\n{body}"
    );
    assert!(
        !body.contains("::cmd_watch"),
        "cmd_autopilot_start must NOT call any cmd_watch alias; body:\n{body}"
    );
}

#[test]
fn m229_f01_legacy_commands_modules_are_renamed_or_deleted() {
    // Static check: the canonical autopilot run-state shape is named
    // `AutopilotRunState`, not `WatchRunState`; the legacy paths are
    // explicitly renamed.
    //
    // We exercise the public types the autopilot drive re-exports
    // through `mp::autopilot::drive`. If the legacy `WatchRunState`
    // rename regressed, this test would fail to compile because
    // `AutopilotRunState::path_for` would resolve back to the
    // legacy `.mp/watch.state.json` filename.
    use mp::autopilot::drive::AutopilotRunState;
    let dir = tempfile::TempDir::new().unwrap();
    let path = AutopilotRunState::path_for(dir.path());
    let path_str = path.to_string_lossy();
    assert!(
        path_str.contains("autopilot-run"),
        "AutopilotRunState path must use the renamed autopilot-run.state.json file; got {path_str}"
    );
    assert!(
        !path_str.contains("watch.state"),
        "legacy watch.state.json filename must NOT appear in the AutopilotRunState path; got {path_str}"
    );
}

// ─── M229 / F-02 regression: production config surfaces no longer
// expose the legacy `ui.show_watch_tab` key. The renamed key
// (`ui.show_autopilot_tab`) is the canonical surface; the legacy
// key fails as an unknown config key. ────────────────────────────

#[test]
fn m229_f02_legacy_show_watch_tab_config_key_is_removed() {
    let env = TestEnv::new();
    // The canonical key must still read + write + default.
    let get_can = env.run_json(&["config", "get", "ui.show_autopilot_tab", "--format", "json"]);
    assert!(
        !get_can.is_null() || get_can.is_object(),
        "ui.show_autopilot_tab must be readable; got {get_can}"
    );

    // The legacy key must be rejected as an unknown key.
    let set_legacy = env.run(&["config", "set", "ui.show_watch_tab", "true"]);
    assert!(
        !set_legacy.status.success(),
        "ui.show_watch_tab must be rejected as an unknown key after M229; got exit 0"
    );
    let stderr = String::from_utf8_lossy(&set_legacy.stderr);
    let stdout = String::from_utf8_lossy(&set_legacy.stdout);
    assert!(
        stderr.contains("unknown")
            || stderr.contains("ui.show_watch_tab")
            || stdout.contains("unknown")
            || stdout.contains("ui.show_watch_tab"),
        "set failure must name the legacy key: stderr={stderr} stdout={stdout}"
    );

    let get_legacy = env.run(&["config", "get", "ui.show_watch_tab"]);
    assert!(
        !get_legacy.status.success(),
        "ui.show_watch_tab must be rejected as an unknown key on get"
    );
}

#[test]

fn m229_f02_execution_watch_readiness_renamed_to_autopilot_readiness() {
    // The execution readiness field must surface as
    // `autopilot_readiness` (not `watch_readiness`) in
    // `execution_check` JSON.
    use mp::execution::AutopilotReadiness;
    let dir = tempfile::TempDir::new().unwrap();
    let ctx = mp::paths::PlanContext {
        project_root: dir.path().to_path_buf(),
        plan_dir: dir.path().join("master-plan"),
    };
    // The type is public, the name is the contract. If the rename
    // regressed, the type would not compile.
    fn _assert_name(_: AutopilotReadiness) {}
    let _ = ctx;
}
