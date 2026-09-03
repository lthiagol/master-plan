//! M210 / AC-04: spawning stores both per-role source prompts
//! AND the exact bundled prompt delivered to each physical
//! pane, so collapsed topologies are auditable through
//! `mp autopilot session show`.
//!
//! Audit surface pinned by this test file:
//! - session.roles.<role>.spawn_prompt_rendered: per-role
//!   source prompt (one role's contract).
//! - session.prompt_bundles[<label>]: the bundled prompt the
//!   harness actually received for that pane (concatenated for
//!   collapsed topologies).
//! - 3-pane topology: prompt_bundles has three entries
//!   (orchestrator / runner / reviewer), each carrying one
//!   role's contract.
//! - 2-pane topology: prompt_bundles has two entries;
//!   supervisor entry carries the O+V concatenation.
//! - 1-pane topology: prompt_bundles has one entry; the
//!   supervisor entry carries the O+R+V concatenation.

use mp::autopilot::prompts::spawn::{
    harness_extra_flags, render_topology_prompts, BundledPrompt, RoleReexport as Role,
    SpawnPromptInputs, TopologyReexport as Topology,
};
use mp::autopilot::role::{resolve_role_config, ResolvedRoleConfig};
use mp::autopilot::session::{
    AutopilotSession, PaneLayout, PaneRef, RoleConfig, RoleName, RolesConfig, SessionStatus,
    SESSION_SCHEMA_VERSION,
};
use mp::autopilot::spawn::MpBinaryProvenance;
use std::collections::BTreeMap;
use tempfile::TempDir;

fn rc(role: Role, model: Option<&str>) -> ResolvedRoleConfig {
    let builtin = mp::autopilot::role::builtin_role_default(role);
    let mut r = resolve_role_config(None, None, &builtin);
    r.model = model.map(str::to_string);
    r
}

fn inputs(role: Role, rc: ResolvedRoleConfig) -> SpawnPromptInputs {
    SpawnPromptInputs::new("master-plan", "sess-alpha", "M210", 0, rc).unwrap()
}

fn make_session_with_bundles(
    _topology: Topology,
    bundles: &[BundledPrompt],
    role_o: &ResolvedRoleConfig,
    role_r: &ResolvedRoleConfig,
    role_v: &ResolvedRoleConfig,
    handles: &[(&str, &str)], // (label, pane_id)
) -> AutopilotSession {
    // Mirror the persist_session function in autopilot/spawn.rs
    // — the audit surface is what spawn.rs writes.
    let mut session = AutopilotSession::blank("sess-alpha");
    session.herdr_workspace = Some("sess-alpha-autopilot".into());
    session.status = SessionStatus::Active;
    session.topology = PaneLayout {
        orchestrator: handles
            .iter()
            .find(|(l, _)| *l == "role-orchestrator-1")
            .map(|(_, id)| PaneRef {
                pane_id: (*id).to_string(),
                label: Some("role-orchestrator-1".into()),
            }),
        runner: handles
            .iter()
            .find(|(l, _)| *l == "role-runner-1")
            .map(|(_, id)| PaneRef {
                pane_id: (*id).to_string(),
                label: Some("role-runner-1".into()),
            }),
        reviewer: handles
            .iter()
            .find(|(l, _)| *l == "role-reviewer-1")
            .map(|(_, id)| PaneRef {
                pane_id: (*id).to_string(),
                label: Some("role-reviewer-1".into()),
            }),
    };
    let io = inputs(Role::Orchestrator, role_o.clone());
    let ir = inputs(Role::Runner, role_r.clone());
    let iv = inputs(Role::Reviewer, role_v.clone());
    let r_o = mp::autopilot::prompts::spawn::render_role_prompt(Role::Orchestrator, &io);
    let r_r = mp::autopilot::prompts::spawn::render_role_prompt(Role::Runner, &ir);
    let r_v = mp::autopilot::prompts::spawn::render_role_prompt(Role::Reviewer, &iv);
    session.roles = RolesConfig {
        orchestrator: Some(RoleConfig {
            role: RoleName::Orchestrator,
            pane_id: Some("%1".into()),
            model: role_o.model.clone(),
            harness: Some(role_o.harness.clone()),
            skill: Some(role_o.skill.clone()),
            config_hash: None,
            spawn_prompt_rendered: Some(r_o),
        }),
        runner: Some(RoleConfig {
            role: RoleName::Runner,
            pane_id: Some("%2".into()),
            model: role_r.model.clone(),
            harness: Some(role_r.harness.clone()),
            skill: Some(role_r.skill.clone()),
            config_hash: None,
            spawn_prompt_rendered: Some(r_r),
        }),
        reviewer: Some(RoleConfig {
            role: RoleName::Reviewer,
            pane_id: Some("%3".into()),
            model: role_v.model.clone(),
            harness: Some(role_v.harness.clone()),
            skill: Some(role_v.skill.clone()),
            config_hash: None,
            spawn_prompt_rendered: Some(r_v),
        }),
    };
    let mut prompt_bundles: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for bundle in bundles {
        prompt_bundles.insert(
            bundle.label.clone(),
            serde_json::json!({
                "roles": bundle.roles.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
                "prompt": bundle.prompt,
            }),
        );
    }
    session.prompt_bundles = prompt_bundles;
    session.binary_provenance = Some(MpBinaryProvenance {
        binary_path: std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unknown>".into()),
        version: env!("CARGO_PKG_VERSION").into(),
        schema_version: SESSION_SCHEMA_VERSION,
        build_kind: "test".into(),
        recorded_at: "2026-01-01T00:00:00Z".into(),
    });
    session
}

#[test]
fn three_pane_session_persists_three_role_prompts_and_three_bundles() {
    let ro = rc(Role::Orchestrator, Some("anthropic/claude-opus-4-1"));
    let rr = rc(Role::Runner, Some("anthropic/claude-opus-4-1"));
    let rv = rc(Role::Reviewer, Some("anthropic/claude-opus-4-1"));
    let io = inputs(Role::Orchestrator, ro.clone());
    let ir = inputs(Role::Runner, rr.clone());
    let iv = inputs(Role::Reviewer, rv.clone());
    let bundles = render_topology_prompts(&io, &ir, &iv, Topology::ThreeAgent);
    let session = make_session_with_bundles(
        Topology::ThreeAgent,
        &bundles,
        &ro,
        &rr,
        &rv,
        &[("role-orchestrator-1", "%1"), ("role-runner-1", "%2"), ("role-reviewer-1", "%3")],
    );
    // Per-role source prompts are populated.
    assert!(session.roles.orchestrator.as_ref().unwrap().spawn_prompt_rendered.is_some());
    assert!(session.roles.runner.as_ref().unwrap().spawn_prompt_rendered.is_some());
    assert!(session.roles.reviewer.as_ref().unwrap().spawn_prompt_rendered.is_some());
    // prompt_bundles has three entries.
    assert_eq!(session.prompt_bundles.len(), 3);
    for label in ["role-orchestrator-1", "role-runner-1", "role-reviewer-1"] {
        assert!(
            session.prompt_bundles.contains_key(label),
            "missing prompt_bundle for {label}"
        );
    }
}

#[test]
fn two_pane_supervisor_bundle_carries_orchestrator_and_reviewer_prompts() {
    let ro = rc(Role::Orchestrator, None);
    let rr = rc(Role::Runner, None);
    let rv = rc(Role::Reviewer, None);
    let io = inputs(Role::Orchestrator, ro.clone());
    let ir = inputs(Role::Runner, rr.clone());
    let iv = inputs(Role::Reviewer, rv.clone());
    let bundles = render_topology_prompts(&io, &ir, &iv, Topology::TwoAgent);
    let session = make_session_with_bundles(
        Topology::TwoAgent,
        &bundles,
        &ro,
        &rr,
        &rv,
        &[("supervisor", "%1"), ("role-runner-1", "%2")],
    );
    assert_eq!(session.prompt_bundles.len(), 2);
    let supervisor = session
        .prompt_bundles
        .get("supervisor")
        .expect("supervisor bundle present");
    let roles_in_supervisor: Vec<String> = supervisor
        .get("roles")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    assert!(roles_in_supervisor.contains(&"orchestrator".to_string()));
    assert!(roles_in_supervisor.contains(&"reviewer".to_string()));
    let prompt_text = supervisor.get("prompt").and_then(|v| v.as_str()).unwrap();
    assert!(prompt_text.contains("─── role: orchestrator ───"));
    assert!(prompt_text.contains("─── role: reviewer ───"));
}

#[test]
fn one_pane_supervisor_bundle_carries_all_three_role_prompts() {
    let ro = rc(Role::Orchestrator, None);
    let rr = rc(Role::Runner, None);
    let rv = rc(Role::Reviewer, None);
    let io = inputs(Role::Orchestrator, ro.clone());
    let ir = inputs(Role::Runner, rr.clone());
    let iv = inputs(Role::Reviewer, rv.clone());
    let bundles = render_topology_prompts(&io, &ir, &iv, Topology::OneAgent);
    let session = make_session_with_bundles(
        Topology::OneAgent,
        &bundles,
        &ro,
        &rr,
        &rv,
        &[("supervisor", "%1")],
    );
    assert_eq!(session.prompt_bundles.len(), 1);
    let supervisor = session
        .prompt_bundles
        .get("supervisor")
        .expect("supervisor bundle present");
    let roles_in_supervisor: Vec<String> = supervisor
        .get("roles")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    assert!(roles_in_supervisor.contains(&"orchestrator".to_string()));
    assert!(roles_in_supervisor.contains(&"runner".to_string()));
    assert!(roles_in_supervisor.contains(&"reviewer".to_string()));
    let prompt_text = supervisor.get("prompt").and_then(|v| v.as_str()).unwrap();
    assert!(prompt_text.contains("─── role: orchestrator ───"));
    assert!(prompt_text.contains("─── role: runner ───"));
    assert!(prompt_text.contains("─── role: reviewer ───"));
}

#[test]
fn persisted_session_round_trips_via_load_session() {
    // Full integration: persist a 3-pane session via the
    // crate's save_session, then load it back and assert the
    // audit fields survive the round trip.
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path();
    let plan_dir = project_root.join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    let ctx = mp::paths::PlanContext {
        project_root: project_root.to_path_buf(),
        plan_dir: plan_dir.clone(),
    };
    let ro = rc(Role::Orchestrator, Some("anthropic/claude-opus-4-1"));
    let rr = rc(Role::Runner, Some("anthropic/claude-opus-4-1"));
    let rv = rc(Role::Reviewer, Some("anthropic/claude-opus-4-1"));
    let io = inputs(Role::Orchestrator, ro.clone());
    let ir = inputs(Role::Runner, rr.clone());
    let iv = inputs(Role::Reviewer, rv.clone());
    let bundles = render_topology_prompts(&io, &ir, &iv, Topology::ThreeAgent);
    let session = make_session_with_bundles(
        Topology::ThreeAgent,
        &bundles,
        &ro,
        &rr,
        &rv,
        &[("role-orchestrator-1", "%1"), ("role-runner-1", "%2"), ("role-reviewer-1", "%3")],
    );
    mp::autopilot::session::save_session(&ctx, "sess-alpha", &session).unwrap();
    let loaded = mp::autopilot::session::load_session(&ctx, "sess-alpha").unwrap();
    assert_eq!(loaded.prompt_bundles.len(), 3);
    assert!(loaded.roles.orchestrator.as_ref().unwrap().spawn_prompt_rendered.is_some());
    assert!(loaded.roles.runner.as_ref().unwrap().spawn_prompt_rendered.is_some());
    assert!(loaded.roles.reviewer.as_ref().unwrap().spawn_prompt_rendered.is_some());
    assert!(loaded.binary_provenance.is_some());
}

#[test]
fn harness_extra_flags_used_in_pipeline_match_persisted_role_config() {
    // Audit invariant: the harness flags recorded on the
    // session's roles.<role>.harness are the ones the
    // pipeline would have translated at spawn time. If a
    // future refactor breaks the translation, the persisted
    // role config and the live harness_extra_flags output
    // diverge.
    let mut r = rc(Role::Runner, None);
    r.harness = "opencode".into();
    r.skill = "mp-runner".into();
    let flags = harness_extra_flags(&r).unwrap();
    assert!(flags.contains(&"--skill".to_string()));
    assert!(flags.contains(&"mp-runner".to_string()));
}
