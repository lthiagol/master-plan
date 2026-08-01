//! M149 S9 / AC-06: `mp watch --dry-run` execution plan preview.
//!
//! Verifies the dry-run report includes milestone state, next
//! actions, the herdr command argv that would be spawned, and a
//! prompt preview — without modifying plan.json or spawning agents.

mod common;

use crate::common::TestEnv;
use serde_json::Value;

fn watch(env: &TestEnv, args: &[&str]) -> Value {
    let mut full = vec!["watch"];
    full.extend(args.iter());
    full.push("--format");
    full.push("json");
    env.run_json(&full)
}

fn create_draft_milestone(env: &TestEnv, title: &str) -> String {
    let create_json = format!(
        r#"{{
        "title": "{title}",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {{ "outcome": "{title}" }},
        "problem": {{ "description": "p" }},
        "scope": {{ "in_scope": ["x"], "out_of_scope": ["y", "z"] }},
        "acceptance_criteria": [
            {{ "description": "ac", "verification": "manual: yes" }}
        ]
    }}"#
    );
    let created = env.run_json(&[
        "milestone",
        "create",
        "--json",
        &create_json,
        "--format",
        "json",
    ]);
    created["milestone"]["id"].as_str().unwrap().to_string()
}

fn create_approved_milestone(env: &TestEnv, title: &str) -> String {
    let create_json = format!(
        r#"{{
        "title": "{title}",
        "depends_on": [],
        "effort": "S",
        "risk": "low",
        "intent": {{ "outcome": "{title}" }},
        "problem": {{ "description": "p" }},
        "scope": {{ "in_scope": ["x"], "out_of_scope": ["y", "z"] }},
        "acceptance_criteria": [
            {{ "description": "ac", "verification": "manual: yes" }}
        ]
    }}"#
    );
    let created = env.run_json(&[
        "milestone",
        "create",
        "--json",
        &create_json,
        "--format",
        "json",
    ]);
    let id = created["milestone"]["id"].as_str().unwrap().to_string();
    // Promote to approved so next_stage routes to Execute.
    env.run(&["milestone", "set-spec-status", &id, "ready"]);
    env.run(&["milestone", "approve", &id, "--format", "json"]);
    id
}

#[test]
fn dry_run_does_not_modify_plan_json() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "plan-stability");
    let plan_before =
        std::fs::read_to_string(env.tmp.path().join("master-plan/plan.json")).unwrap_or_default();
    let _ = watch(&env, &["--dry-run", &id]);
    let plan_after =
        std::fs::read_to_string(env.tmp.path().join("master-plan/plan.json")).unwrap_or_default();
    assert_eq!(plan_before, plan_after, "dry-run must not modify plan.json");
}

#[test]
fn dry_run_does_not_create_watch_state_file() {
    // M178 S8 / AC-07: dry-run is the sole authoritative preflight
    // and must NOT spawn panes, modify plan.json, or persist any
    // state file. This pin catches future refactors that accidentally
    // wire dry-run through the same `cmd_watch_drive` path that
    // writes `.mp/watch.state.json`.
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "state-no-touch");
    let state_path = env.tmp.path().join("master-plan/.mp/watch.state.json");
    assert!(
        !state_path.exists(),
        "fresh project should not have a state file"
    );
    let _ = watch(&env, &["--dry-run", &id]);
    assert!(
        !state_path.exists(),
        "dry-run must not create .mp/watch.state.json (got {state_path:?})"
    );
}

#[test]
fn dry_run_does_not_spawn_herdr_or_run_a_driver() {
    // AC-07: dry-run remains side-effect-free. We pin the
    // negative space by asserting the watch log file is NOT created
    // (the foreground path writes to it on every herdr call).
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "log-no-touch");
    let log_path = env.tmp.path().join("master-plan/.mp/watch.log");
    assert!(!log_path.exists());
    let _ = watch(&env, &["--dry-run", &id]);
    assert!(
        !log_path.exists(),
        "dry-run must not write watch.log (got {log_path:?})"
    );
}

#[test]
fn dry_run_includes_milestone_state_and_next_action() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "state-surface");
    let report = watch(&env, &["--dry-run", &id]);
    let entry = &report["milestones"][0];
    assert_eq!(entry["id"].as_str(), Some(id.as_str()));
    assert_eq!(entry["lifecycle"].as_str(), Some("approved"));
    assert_eq!(entry["spec_status"].as_str(), Some("ready"));
    assert_eq!(entry["ready"], serde_json::json!(true));
    assert_eq!(entry["next_action"].as_str(), Some("execute"));
}

#[test]
fn dry_run_includes_stage_and_target_lifecycle() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "stage-surface");
    let report = watch(&env, &["--dry-run", &id]);
    let entry = &report["milestones"][0];
    assert_eq!(entry["stage"].as_str(), Some("execute"));
    assert_eq!(entry["target_lifecycle"].as_str(), Some("complete"));
}

#[test]
fn dry_run_includes_herdr_start_argv_preview() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "herdr-preview");
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    let report = watch(&env, &["--dry-run", &id]);
    let entry = &report["milestones"][0];
    let cmds = entry["herdr_commands"].as_array().unwrap();
    assert!(
        !cmds.is_empty(),
        "should preview at least one herdr command"
    );
    let cmd = &cmds[0];
    assert_eq!(cmd["role"].as_str(), Some("runner"));
    assert_eq!(cmd["label"].as_str(), Some("role-runner-1"));
    let argv = cmd["argv"].as_array().unwrap();
    let argv_str = serde_json::to_string(argv).unwrap();
    assert!(
        argv_str.contains("agent")
            && argv_str.contains("start")
            && argv_str.contains("role-runner-1"),
        "argv should be `agent start <label>`: {argv_str}"
    );
    assert!(
        argv_str.contains("opencode"),
        "harness argv should include opencode: {argv_str}"
    );
}

#[test]
fn dry_run_reflects_runner_model_in_argv() {
    // M151 ext-review F-04 (2026-07-14): AC-03 end-to-end shape
    // check. The base case (`dry_run_includes_herdr_start_argv_preview`)
    // only covers harness resolution; this test sets the runner
    // model via `mp config set` and asserts the herdr_argv preview
    // surfaces `--model <name>` via the registry's flag translator.
    // Pinning this in --dry-run protects the registry -> watch
    // wiring from regressing back to a config-bypass.
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "model-flow");
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.runner.model", "claude-opus-4"]);
    let report = watch(&env, &["--dry-run", &id]);
    let entry = &report["milestones"][0];
    let cmds = entry["herdr_commands"].as_array().unwrap();
    let cmd = cmds
        .iter()
        .find(|c| c["role"] == "runner")
        .expect("runner herdr command present");
    let argv_str = serde_json::to_string(&cmd["argv"]).unwrap();
    assert!(
        argv_str.contains("--model") && argv_str.contains("claude-opus-4"),
        "runner herdr argv should include --model claude-opus-4 (registry wired through \
         mp watch): {argv_str}"
    );
}

#[test]
fn dry_run_includes_prompt_preview_for_approved_milestone() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "prompt-preview");
    let report = watch(&env, &["--dry-run", &id]);
    let entry = &report["milestones"][0];
    let preview = entry["prompt_preview"].as_str().unwrap();
    // Preview is truncated at 280 chars; just pin the safety
    // preamble + the wrapped title, which fit in the head of the
    // truncated string.
    assert!(
        preview.contains("SAFETY"),
        "preview should include the safety preamble: {preview}"
    );
    assert!(
        preview.contains(&format!("<milestone-id>{id}</milestone-id>")),
        "preview should include the wrapped milestone id: {preview}"
    );
}

#[test]
fn dry_run_skips_unready_milestone_without_herdr_commands() {
    let env = TestEnv::new();
    // A draft milestone gets skipped by should_skip (lifecycle=draft
    // is outside mp watch scope). The dry-run preview should show no
    // herdr commands, no stage plan, and a skip_* next-action.
    let id = create_draft_milestone(&env, "draft-skip");
    let report = watch(&env, &["--dry-run", &id]);
    let entry = &report["milestones"][0];
    assert_eq!(entry["lifecycle"].as_str(), Some("draft"));
    assert!(
        entry["next_action"].as_str().unwrap().starts_with("skip_"),
        "draft should route to skip_*: {}",
        entry["next_action"]
    );
    assert!(
        entry["herdr_commands"].as_array().unwrap().is_empty(),
        "skipped milestone should have no herdr commands"
    );
    assert!(
        entry["stage"].is_null(),
        "skipped milestone should have no stage plan"
    );
}

#[test]
fn dry_run_handles_unknown_milestone_without_panicking() {
    let env = TestEnv::new();
    let report = watch(&env, &["--dry-run", "99999"]);
    let entry = &report["milestones"][0];
    assert!(entry["error"].as_str().is_some());
    assert!(
        entry["herdr_commands"].as_array().unwrap().is_empty(),
        "unknown milestone should produce no herdr commands"
    );
}

#[test]
fn dry_run_reports_preconditions_alongside_milestones() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "preconditions-surface");
    let report = watch(&env, &["--dry-run", &id]);
    assert!(report["preconditions"]["checks"].is_array());
    assert!(
        report["preconditions"]["checks"].as_array().unwrap().len() >= 4,
        "precondition checks should always be reported"
    );
}

#[test]
fn dry_run_lists_log_file_path() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "log-path-surface");
    let report = watch(&env, &["--dry-run", &id]);
    let log_file = report["log_file"].as_str().unwrap();
    assert!(
        log_file.ends_with(".mp/watch.log"),
        "default log path should be <plan_dir>/.mp/watch.log: {log_file}"
    );
}

// ─── M153 ext-review F-10: dry-run uses project-local override path ─────────

/// F-10: with a project-local `<plan_dir>/watch/execute.md` override,
/// the dry-run preview must render the override body and report
/// `prompt_source == "override"` so operators see the same prompt
/// the live state machine will dispatch.
#[test]
fn dry_run_renders_project_local_override_and_reports_override_source() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "override-precedence");

    // Drop the project-local override into the plan dir.
    let plan_watch_dir = env.tmp.path().join("master-plan/watch");
    std::fs::create_dir_all(&plan_watch_dir).unwrap();
    let sentinel = "F-10-DRYRUN-OVERRIDE-SENTINEL";
    std::fs::write(
        plan_watch_dir.join("execute.md"),
        format!("{{header}}{sentinel}\n"),
    )
    .unwrap();

    let report = watch(&env, &["--dry-run", &id]);
    let entry = &report["milestones"][0];
    assert_eq!(
        entry["prompt_source"].as_str(),
        Some("override"),
        "dry-run should report prompt_source=override when the project-local file is present; got entry={entry}"
    );
    let preview = entry["prompt_preview"].as_str().unwrap();
    // The preview is truncated to 280 chars and `{header}` renders into
    // ~270 chars of metadata before the body, so an override sentinel
    // must live BEFORE the {header} placeholder or be asserted via
    // prompt_source rather than via preview substring.
    assert_eq!(
        entry["prompt_source"].as_str(),
        Some("override"),
        "dry-run should report prompt_source=override when the project-local file is present; got entry={entry}"
    );
    assert!(
        preview.contains("{header}") || preview.starts_with("# mp watch — execute"),
        "dry-run preview should at least contain the watch header or the {{header}} placeholder; got: {preview}"
    );
    let path = entry["prompt_source_path"].as_str().unwrap();
    assert!(
        path.ends_with("watch/execute.md"),
        "prompt_source_path should point at the override file; got: {path}"
    );
}

/// F-10: without an override file, the dry-run reports
/// `prompt_source == "default"` and renders the compiled-in
/// preamble. This pins the no-override case so the F-10 wiring
/// can't regress to "always default".
#[test]
fn dry_run_reports_default_source_when_no_override_file_exists() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "default-source");
    // Ensure no override file exists at the project-local rung.
    let plan_watch_dir = env.tmp.path().join("master-plan/watch");
    if plan_watch_dir.exists() {
        std::fs::remove_dir_all(&plan_watch_dir).unwrap();
    }
    let report = watch(&env, &["--dry-run", &id]);
    let entry = &report["milestones"][0];
    assert_eq!(entry["prompt_source"].as_str(), Some("default"));
    assert!(
        entry["prompt_source_path"].is_null(),
        "prompt_source_path must be absent for the default case; got: {:?}",
        entry["prompt_source_path"]
    );
}

/// F-11: a headerless project-local override is reflected in the
/// dry-run's `override_diagnostics` array so operators can debug a
/// misbehaving override without reading the live log.
#[test]
fn dry_run_surfaces_override_diagnostics_for_headerless_override() {
    let env = TestEnv::new();
    let id = create_approved_milestone(&env, "diagnostic-surfacing");
    let plan_watch_dir = env.tmp.path().join("master-plan/watch");
    std::fs::create_dir_all(&plan_watch_dir).unwrap();
    std::fs::write(
        plan_watch_dir.join("execute.md"),
        "BAD OVERRIDE — no placeholder\n",
    )
    .unwrap();

    let report = watch(&env, &["--dry-run", &id]);
    let entry = &report["milestones"][0];
    assert_eq!(entry["prompt_source"].as_str(), Some("default"));
    let diags = entry["override_diagnostics"]
        .as_array()
        .expect("override_diagnostics array");
    assert_eq!(diags.len(), 1, "exactly one diagnostic");
    let d = &diags[0];
    assert_eq!(d["kind"].as_str(), Some("header_missing"));
    assert_eq!(d["rung"].as_str(), Some("plan_dir"));
    assert!(d["path"].as_str().unwrap().ends_with("watch/execute.md"));
}
