//! M210 / AC-06: `mp autopilot start` creates or reuses the
//! named herdr workspace, creates the topology pane slots,
//! starts the configured harness in each pane with the
//! rendered prompt, persists pane IDs only after successful
//! starts, and rolls back partial creation on failure.
//!
//! Coverage:
//! - One-pane, two-pane, and three-pane topologies all
//!   succeed end-to-end via the pipeline.
//! - Workspace ensure / pane create / agent start failures
//!   trigger rollback (delete_pane called on every pane
//!   created so far).
//! - Prompt-delivery failure also triggers rollback.
//! - Pane IDs are persisted ONLY after successful starts —
//!   a failure leaves session.json without the half-spawned
//!   pane's pane_id.
//! - The harness_extra_flags output is forwarded verbatim on
//!   the agent start argv.

use mp::autopilot::prompts::spawn::{
    RoleReexport as Role, SpawnPromptInputs, TopologyReexport as Topology,
};
use mp::autopilot::role::{resolve_role_config, ResolvedRoleConfig};
use mp::autopilot::spawn::{spawn_session, MockHerdrSpawnOps, SpawnError, SpawnInputs};
use mp::paths::PlanContext;
use std::path::PathBuf;
use tempfile::TempDir;

fn rc(role: Role) -> ResolvedRoleConfig {
    let builtin = mp::autopilot::role::builtin_role_default(role);
    resolve_role_config(None, None, &builtin)
}

#[allow(dead_code)]
fn inputs(_role: Role, r: ResolvedRoleConfig) -> SpawnPromptInputs {
    SpawnPromptInputs::new("master-plan", "sess-alpha", "M210", 0, r).unwrap()
}

fn ctx_in(dir: &std::path::Path) -> PlanContext {
    PlanContext {
        project_root: dir.to_path_buf(),
        plan_dir: dir.join("master-plan"),
    }
}

fn session_file_path(ctx: &PlanContext, session_id: &str) -> PathBuf {
    mp::autopilot::session::SessionPath::new(ctx, session_id)
        .unwrap()
        .file
}

fn make_inputs<'a>(
    ctx: &'a PlanContext,
    topology: Topology,
) -> (SpawnInputs<'a>, MockHerdrSpawnOps) {
    let ro = rc(Role::Orchestrator);
    let rr = rc(Role::Runner);
    let rv = rc(Role::Reviewer);
    let si = SpawnInputs {
        ctx,
        session_id: "sess-alpha",
        topology,
        project_root: ctx.project_root.as_path(),
        role_o: ro.clone(),
        role_r: rr.clone(),
        role_v: rv.clone(),
        project_name: "master-plan",
        milestone_id: "M210",
        queue_position: 0,
    };
    (si, MockHerdrSpawnOps::new())
}

fn make_inputs_with_failure<'a, F: FnOnce(&MockHerdrSpawnOps)>(
    ctx: &'a PlanContext,
    topology: Topology,
    script: F,
) -> (SpawnInputs<'a>, MockHerdrSpawnOps) {
    let (si, ops) = make_inputs(ctx, topology);
    script(&ops);
    (si, ops)
}

#[test]
fn three_pane_pipeline_succeeds_end_to_end() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs(&ctx, Topology::ThreeAgent);
    let outcome = spawn_session(&ops, &si).expect("3-pane pipeline should succeed");
    assert_eq!(outcome.panes.len(), 3);
    let snap = ops.snapshot();
    // Workspace was ensured.
    assert_eq!(snap.ensure_calls.len(), 1);
    assert_eq!(snap.ensure_calls[0], "sess-alpha-autopilot");
    // Three panes were created (one per role).
    assert_eq!(snap.create_calls, vec![0, 1, 2]);
    // Three agent starts + three prompt deliveries.
    assert_eq!(snap.start_calls.len(), 3);
    assert_eq!(snap.send_calls.len(), 3);
    // No rollback on success.
    assert_eq!(snap.delete_calls.len(), 0);
    // session.json was persisted.
    let path = session_file_path(&ctx, "sess-alpha");
    assert!(path.exists());
    // Per-pane bundles persisted.
    assert_eq!(outcome.bundles.len(), 3);
    let loaded = mp::autopilot::session::load_session(&ctx, "sess-alpha").unwrap();
    assert!(loaded
        .roles
        .orchestrator
        .as_ref()
        .unwrap()
        .pane_id
        .is_some());
    assert!(loaded.roles.runner.as_ref().unwrap().pane_id.is_some());
    assert!(loaded.roles.reviewer.as_ref().unwrap().pane_id.is_some());
}

#[test]
fn two_pane_pipeline_succeeds_with_supervisor_bundle() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs(&ctx, Topology::TwoAgent);
    let outcome = spawn_session(&ops, &si).expect("2-pane pipeline should succeed");
    assert_eq!(outcome.panes.len(), 2);
    let snap = ops.snapshot();
    assert_eq!(snap.ensure_calls.len(), 1);
    // Supervisor pane (carrying O+V) + runner pane.
    assert_eq!(snap.create_calls, vec![0, 1]);
    assert_eq!(snap.start_calls.len(), 2);
    assert_eq!(snap.send_calls.len(), 2);
    let loaded = mp::autopilot::session::load_session(&ctx, "sess-alpha").unwrap();
    // Orchestrator + reviewer pane ids collapse into the
    // supervisor pane id.
    let supervisor_pane_id = loaded
        .topology
        .orchestrator
        .as_ref()
        .unwrap()
        .pane_id
        .clone();
    assert_eq!(
        loaded.topology.reviewer.as_ref().unwrap().pane_id,
        supervisor_pane_id
    );
    assert_ne!(
        loaded.topology.runner.as_ref().unwrap().pane_id,
        supervisor_pane_id
    );
}

#[test]
fn one_pane_pipeline_succeeds_with_collapsed_bundle() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs(&ctx, Topology::OneAgent);
    let outcome = spawn_session(&ops, &si).expect("1-pane pipeline should succeed");
    assert_eq!(outcome.panes.len(), 1);
    let snap = ops.snapshot();
    assert_eq!(snap.create_calls, vec![0]);
    assert_eq!(snap.start_calls.len(), 1);
    assert_eq!(snap.send_calls.len(), 1);
    let loaded = mp::autopilot::session::load_session(&ctx, "sess-alpha").unwrap();
    // All three roles collapse into the supervisor pane id.
    let supervisor_pane_id = loaded
        .topology
        .orchestrator
        .as_ref()
        .unwrap()
        .pane_id
        .clone();
    assert_eq!(
        loaded.topology.runner.as_ref().unwrap().pane_id,
        supervisor_pane_id
    );
    assert_eq!(
        loaded.topology.reviewer.as_ref().unwrap().pane_id,
        supervisor_pane_id
    );
}

#[test]
fn pane_create_failure_triggers_rollback() {
    // Script: orchestrator pane succeeds, runner pane fails.
    // The pipeline must roll back by deleting the orchestrator
    // pane, then surface the typed error.
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs_with_failure(&ctx, Topology::ThreeAgent, |o| {
        o.push_create_outcome(Ok("pane-1".into()));
        o.push_create_outcome(Err("simulated pane split failure".into()));
    });
    let err = spawn_session(&ops, &si).expect_err("pipeline should fail");
    match err {
        SpawnError::PaneCreateFailed { ordinal, stderr } => {
            assert_eq!(ordinal, 1, "second pane create (runner) failed");
            assert!(stderr.contains("simulated"));
        }
        other => panic!("expected PaneCreateFailed, got {other:?}"),
    }
    let snap = ops.snapshot();
    // Rollback deleted the orchestrator pane (the only one
    // created before the failure).
    assert_eq!(snap.delete_calls.len(), 1);
    assert_eq!(snap.delete_calls[0], "pane-1");
    // Orchestrator pane was successfully started before the
    // runner pane failed; one start recorded.
    assert_eq!(snap.start_calls.len(), 1);
}

#[test]
fn agent_start_failure_triggers_rollback() {
    // Script: orchestrator starts OK, runner start fails.
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs_with_failure(&ctx, Topology::ThreeAgent, |o| {
        o.push_start_outcome(Ok("role-orchestrator-1".into()));
        o.push_start_outcome(Err("simulated agent start failure".into()));
    });
    let err = spawn_session(&ops, &si).expect_err("pipeline should fail");
    match err {
        SpawnError::AgentStartFailed { label, stderr } => {
            assert_eq!(label, "role-runner-1");
            assert!(stderr.contains("simulated"));
        }
        other => panic!("expected AgentStartFailed, got {other:?}"),
    }
    let snap = ops.snapshot();
    // Both panes created (orchestrator + runner) are rolled back.
    assert_eq!(snap.delete_calls.len(), 2);
    assert_eq!(snap.delete_calls[0], "pane-1");
    assert_eq!(snap.delete_calls[1], "pane-2");
}

#[test]
fn prompt_send_failure_triggers_rollback() {
    // Script: orchestrator + runner prompts succeed, reviewer
    // prompt send fails.
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs_with_failure(&ctx, Topology::ThreeAgent, |o| {
        o.push_send_outcome(Ok(()));
        o.push_send_outcome(Ok(()));
        o.push_send_outcome(Err("simulated prompt send failure".into()));
    });
    let err = spawn_session(&ops, &si).expect_err("pipeline should fail");
    match err {
        SpawnError::PromptSendFailed { label, stderr } => {
            assert_eq!(label, "pane-3");
            assert!(stderr.contains("simulated"));
        }
        other => panic!("expected PromptSendFailed, got {other:?}"),
    }
    let snap = ops.snapshot();
    // All three panes are rolled back.
    assert_eq!(snap.delete_calls.len(), 3);
}

#[test]
fn workspace_ensure_failure_short_circuits_before_pane_creation() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs_with_failure(&ctx, Topology::ThreeAgent, |o| {
        o.push_ensure_outcome(Err("simulated workspace failure".into()))
    });
    let err = spawn_session(&ops, &si).expect_err("pipeline should fail");
    match err {
        SpawnError::WorkspaceEnsureFailed { name, stderr } => {
            assert_eq!(name, "sess-alpha-autopilot");
            assert!(stderr.contains("simulated"));
        }
        other => panic!("expected WorkspaceEnsureFailed, got {other:?}"),
    }
    let snap = ops.snapshot();
    // No pane created, no rollback needed.
    assert_eq!(snap.create_calls.len(), 0);
    assert_eq!(snap.delete_calls.len(), 0);
}

#[test]
fn pane_ids_only_persisted_after_successful_starts() {
    // For a 3-pane pipeline, the runner pane fails to start.
    // session.json (if persisted at all) must NOT contain the
    // runner pane_id — only the orchestrator pane that
    // succeeded. The pipeline does not write session.json on
    // failure, so the load_session call returns an error.
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs_with_failure(&ctx, Topology::ThreeAgent, |o| {
        o.push_start_outcome(Err("runner start failed".into()))
    });
    let _err = spawn_session(&ops, &si).expect_err("pipeline should fail");
    // No session.json written.
    let session_path = session_file_path(&ctx, "sess-alpha");
    assert!(
        !session_path.exists(),
        "session.json must NOT exist on partial-failure (no half-spawned sessions)"
    );
}

#[test]
fn harness_extra_flags_forwarded_on_agent_start_argv() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs(&ctx, Topology::ThreeAgent);
    spawn_session(&ops, &si).expect("pipeline should succeed");
    let snap = ops.snapshot();
    // Each role's start argv includes its role's harness kind
    // + skill flag.
    for (label, kind, _pane_id, extras) in &snap.start_calls {
        match label.as_str() {
            "role-orchestrator-1" => {
                assert_eq!(kind, "opencode");
                assert!(extras.contains(&"--skill".to_string()));
                assert!(extras.contains(&"mp-coordinator".to_string()));
            }
            "role-runner-1" => {
                assert_eq!(kind, "opencode");
                assert!(extras.contains(&"--skill".to_string()));
                assert!(extras.contains(&"mp-runner".to_string()));
            }
            "role-reviewer-1" => {
                assert_eq!(kind, "opencode");
                assert!(extras.contains(&"--skill".to_string()));
                assert!(extras.contains(&"mp-runner".to_string()));
            }
            other => panic!("unexpected label {other}"),
        }
    }
}

// F-01 regression test: Reviewer spawn_prompt_rendered must
// contain the Reviewer template, NOT the Runner template.
// The bug: rendered_for used to infer role from skill name
// ('mp-coordinator' -> Orchestrator, _ -> Runner). The
// Reviewer's built-in skill is 'mp-runner', so Reviewer
// inputs fell through to the Runner arm and the persisted
// roles.reviewer.spawn_prompt_rendered contained Runner
// content (Runner boundaries block, Runner typed commands
// including 'mp milestone complete' and 'mp milestone step
// done'). Skill name is a harness-dispatch concern; role
// classification is a template-selection concern.
#[test]
fn f01_regression_reviewer_prompt_uses_reviewer_template_not_runner() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs(&ctx, Topology::ThreeAgent);
    spawn_session(&ops, &si).expect("pipeline should succeed");
    let loaded = mp::autopilot::session::load_session(&ctx, "sess-alpha").unwrap();
    let reviewer_prompt = loaded
        .roles
        .reviewer
        .as_ref()
        .expect("reviewer role populated")
        .spawn_prompt_rendered
        .as_deref()
        .expect("reviewer spawn_prompt_rendered populated");
    // Reviewer template markers — must be present.
    assert!(
        reviewer_prompt.contains("# Spawn prompt — role: reviewer"),
        "reviewer prompt must use Reviewer template header, got: {}",
        reviewer_prompt
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        reviewer_prompt.contains("You are the REVIEWER"),
        "reviewer prompt must contain Reviewer boundaries block"
    );
    // Reviewer typed-command surface — must include mp reviews
    // pass and must NOT include Runner's milestone commands
    // (which used to leak through the buggy skill-name
    // fallback).
    assert!(
        reviewer_prompt.contains("mp reviews pass <id>"),
        "reviewer prompt must include `mp reviews pass <id>` command form"
    );
    // Runner template markers — must be ABSENT.
    assert!(
        !reviewer_prompt.contains("# Spawn prompt — role: runner"),
        "reviewer prompt must NOT use Runner template header (F-01)"
    );
    assert!(
        !reviewer_prompt.contains("You are the RUNNER"),
        "reviewer prompt must NOT contain Runner boundaries block (F-01)"
    );
    // The Runner typed-transition list contains
    // `mp milestone complete <id>` as a command form. With
    // the F-01 bug, this leaked into the Reviewer prompt.
    assert!(
        !reviewer_prompt.contains("mp milestone complete <id>"),
        "reviewer prompt must NOT contain Runner's `mp milestone complete <id>` command form (F-01)"
    );
    assert!(
        !reviewer_prompt.contains("mp milestone step done <id>"),
        "reviewer prompt must NOT contain Runner's `mp milestone step done <id>` command form (F-01)"
    );
    assert!(
        !reviewer_prompt.contains("Per-AC evidence MUST be a real"),
        "reviewer prompt must NOT contain Runner's per-AC evidence reminder (F-01)"
    );
}

// F-01 regression: the same invariant must hold for the
// Orchestrator and Runner slots too — no role's persisted
// prompt should contain another role's template markers.
#[test]
fn f01_regression_orchestrator_and_runner_prompts_use_their_own_templates() {
    let tmp = TempDir::new().unwrap();
    let ctx = ctx_in(tmp.path());
    let (si, ops) = make_inputs(&ctx, Topology::ThreeAgent);
    spawn_session(&ops, &si).expect("pipeline should succeed");
    let loaded = mp::autopilot::session::load_session(&ctx, "sess-alpha").unwrap();

    let orch_prompt = loaded
        .roles
        .orchestrator
        .as_ref()
        .unwrap()
        .spawn_prompt_rendered
        .as_deref()
        .unwrap();
    assert!(orch_prompt.contains("# Spawn prompt — role: orchestrator"));
    assert!(orch_prompt.contains("You are the ORCHESTRATOR"));
    assert!(!orch_prompt.contains("# Spawn prompt — role: runner"));
    assert!(!orch_prompt.contains("# Spawn prompt — role: reviewer"));

    let runner_prompt = loaded
        .roles
        .runner
        .as_ref()
        .unwrap()
        .spawn_prompt_rendered
        .as_deref()
        .unwrap();
    assert!(runner_prompt.contains("# Spawn prompt — role: runner"));
    assert!(runner_prompt.contains("You are the RUNNER"));
    assert!(runner_prompt.contains("mp milestone complete <id>"));
    assert!(!runner_prompt.contains("# Spawn prompt — role: orchestrator"));
    assert!(!runner_prompt.contains("# Spawn prompt — role: reviewer"));
}

// Tiny helper kept as a module-private function above.
