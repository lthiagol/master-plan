//! M101 R1: gating tests for the done -> complete transition. With
//! self-phase findings open, `mp milestone complete` MUST bail. With
//! all findings resolved (or only external-phase remaining), the
//! transition succeeds. Mirrors AC-01 (self-phase gate) and the
//! auto-exit-remediation invariant from AC-05.

use crate::common::lib_api;
use crate::common::TestEnv;
use mp_model::MilestoneFile;

fn create_milestone(env: &TestEnv, title: &str) -> String {
    let payload = serde_json::json!({
        "title": title,
        "intent": { "outcome": "M101 gating regression" },
        "problem": { "description": "M101 gating regression" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{ "description": "AC-01", "verification": "echo ok" }],
        "spec_status": "ready",
    })
    .to_string();
    let out = lib_api::run(env, &["milestone", "create", "--json", &payload]);
    assert!(
        out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["milestone"]["id"].as_str().unwrap().to_string()
}

/// Force the milestone to `lifecycle=done` by writing the file directly.
/// mp's set-status done auto-promotes to complete (M100 design), so there
/// is no CLI path to the `done` checkpoint state — but the model supports
/// it and the M101 auto-enter logic depends on it. M100 design note: this
/// is one of the gaps a future milestone should address (separation of
/// `done` as a stable checkpoint from `complete` as the terminal state).
fn force_lifecycle_done(env: &TestEnv, id: &str) {
    let dir = env.tmp.path().join("master-plan/milestones");
    let path: std::path::PathBuf = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with(id))
        .map(|e| e.path())
        .unwrap_or_else(|| panic!("milestone file not found for {id}"));
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: MilestoneFile = serde_json::from_str(&raw).unwrap();
    m.milestone.lifecycle = "executed".to_string();
    m.milestone.execution_status = "done".to_string();
    m.milestone.spec_status = "verified".to_string();
    m.milestone.priority = "normal".to_string();
    m.milestone.blocked = false;
    m.milestone.deferred = false;
    m.milestone.cancelled = false;
    let serialized = serde_json::to_string_pretty(&m).unwrap();
    std::fs::write(&path, format!("{serialized}\n")).unwrap();
}

#[test]
fn transition_gating_blocks_done_until_self_findings_closed() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "gating-self");
    force_lifecycle_done(&env, &id);

    // The CLI doesn't expose --phase yet (M101 R2 WP5), so the finding
    // lands with phase='' which the helper treats as self-phase (M125
    // convention).
    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "high",
            "--category",
            "correctness",
            "--desc",
            "gating regression seed",
            "--author",
            "test",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "finding add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Now complete_milestone must bail because of the open self finding.
    let out = lib_api::run(
        &env,
        &[
            "milestone",
            "complete",
            &id,
            "--evidence",
            "should be blocked",
        ],
    );
    assert!(
        !out.status.success(),
        "complete must fail with open self-phase finding; got success"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("open self-phase finding"),
        "stderr should mention self-phase gate; got: {stderr}"
    );
}

#[test]
fn auto_enter_remediation_on_open_finding_at_done_checkpoint() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "auto-enter-remediation");
    force_lifecycle_done(&env, &id);

    // File an open self-phase finding.
    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "high",
            "--category",
            "correctness",
            "--desc",
            "remediation seed",
            "--author",
            "test",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "finding add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Read the milestone back; expect lifecycle=remediation.
    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            &id,
            "--fields",
            "milestone.lifecycle",
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let lc = v["milestone"]["lifecycle"].as_str().unwrap();
    assert_eq!(
        lc, "remediation",
        "filing an open self-phase finding must auto-enter remediation; lifecycle was {lc}"
    );
}

#[test]
fn auto_exit_remediation_on_last_finding_resolved() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "auto-exit-remediation");
    force_lifecycle_done(&env, &id);

    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "high",
            "--category",
            "correctness",
            "--desc",
            "exit seed",
            "--author",
            "test",
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());

    // Confirm we're in remediation.
    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            &id,
            "--fields",
            "milestone.lifecycle",
            "--format",
            "json",
        ],
    );
    let lc: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]
        ["lifecycle"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        lc, "remediation",
        "setup precondition: lifecycle should be remediation; got {lc}"
    );

    // Find the open finding id (F-01).
    let out = lib_api::run(
        &env,
        &["reviews", "finding", "list", &id, "--format", "json"],
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let finding_id = v["findings"][0]["id"].as_str().unwrap().to_string();

    // Resolve it.
    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "resolve",
            &id,
            &finding_id,
            "--commit",
            "auto-exit-remediation-test",
        ],
    );
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Lifecycle should now be done (pre-state for self-phase findings).
    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            &id,
            "--fields",
            "milestone.lifecycle",
            "--format",
            "json",
        ],
    );
    let lc: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]
        ["lifecycle"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        lc, "executed",
        "resolving the last open finding on a self-phase remediation milestone must exit to done; got {lc}"
    );
}

#[test]
fn remediation_priority_no_auto_revert() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "priority-no-revert");
    force_lifecycle_done(&env, &id);

    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "high",
            "--category",
            "correctness",
            "--desc",
            "priority invariant",
            "--author",
            "test",
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());

    // Confirm priority=high after entering remediation.
    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            &id,
            "--fields",
            "milestone.priority",
            "--format",
            "json",
        ],
    );
    let pri: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()
        ["milestone"]["priority"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(pri, "high", "remediation must set priority=high; got {pri}");

    // Resolve the finding → exit remediation → priority STAYS high.
    let out = lib_api::run(
        &env,
        &["reviews", "finding", "list", &id, "--format", "json"],
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let finding_id = v["findings"][0]["id"].as_str().unwrap().to_string();
    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "resolve",
            &id,
            &finding_id,
            "--commit",
            "no-auto-revert-test",
        ],
    );
    if !out.status.success() {
        eprintln!("RESOLVE STDOUT: {}", String::from_utf8_lossy(&out.stdout));
        eprintln!("RESOLVE STDERR: {}", String::from_utf8_lossy(&out.stderr));
    }
    assert!(out.status.success());

    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            &id,
            "--fields",
            "milestone.priority,milestone.lifecycle",
            "--format",
            "json",
        ],
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let pri: String = v["milestone"]["priority"].as_str().unwrap().to_string();
    let lc: String = v["milestone"]["lifecycle"].as_str().unwrap().to_string();
    assert_eq!(lc, "executed", "lifecycle should exit to done; got {lc}");
    assert_eq!(
        pri, "high",
        "priority must NOT auto-revert on remediation exit (M101 AC-13 invariant); got {pri}"
    );
}

#[test]
fn auto_enter_remediation_from_lifecycle_complete() {
    // M101 R1 + subagent review H-1: the auto-enter match arm must
    // include `complete` (the common post-M100 terminal state). Filing
    // a finding on a complete milestone escalates to remediation +
    // priority=high, the same as on `done`. Pre-fix, this case
    // silently fell through and the gate never fired.
    use mp_model::MilestoneFile;

    let env = TestEnv::new();
    let id = create_milestone(&env, "auto-enter-from-complete");

    // Set lifecycle=complete via direct file edit (no CLI path exposes
    // this — it's the M100 terminal state set by set_execution_status
    // "done" or complete_milestone).
    let dir = env.tmp.path().join("master-plan/milestones");
    let path: std::path::PathBuf = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with(&id))
        .map(|e| e.path())
        .unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: MilestoneFile = serde_json::from_str(&raw).unwrap();
    m.milestone.lifecycle = "complete".to_string();
    m.milestone.execution_status = "done".to_string();
    m.milestone.spec_status = "verified".to_string();
    m.milestone.priority = "normal".to_string();
    m.milestone.blocked = false;
    m.milestone.deferred = false;
    m.milestone.cancelled = false;
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&m).unwrap()),
    )
    .unwrap();

    // File a self-phase finding on the complete milestone.
    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "high",
            "--category",
            "correctness",
            "--desc",
            "post-completion finding",
            "--author",
            "test",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "finding add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Expect lifecycle=remediation and priority=high.
    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            &id,
            "--fields",
            "milestone.lifecycle,milestone.priority",
            "--format",
            "json",
        ],
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let lc = v["milestone"]["lifecycle"].as_str().unwrap();
    let pri = v["milestone"]["priority"].as_str().unwrap();
    assert_eq!(
        lc, "remediation",
        "filing on complete must auto-enter remediation; got {lc}"
    );
    assert_eq!(
        pri, "high",
        "filing on complete must escalate priority; got {pri}"
    );
}

#[test]
fn auto_enter_remediation_does_not_downgrade_urgent() {
    // M101 R1 + subagent review M-3: priority=urgent stays urgent on
    // remediation entry. Only lower-priority values escalate to high.
    use mp_model::MilestoneFile;

    let env = TestEnv::new();
    let id = create_milestone(&env, "urgent-stays-urgent");

    let dir = env.tmp.path().join("master-plan/milestones");
    let path: std::path::PathBuf = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with(&id))
        .map(|e| e.path())
        .unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: MilestoneFile = serde_json::from_str(&raw).unwrap();
    m.milestone.lifecycle = "executed".to_string();
    m.milestone.priority = "urgent".to_string();
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&m).unwrap()),
    )
    .unwrap();

    let out = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "high",
            "--category",
            "correctness",
            "--desc",
            "urgent priority test",
            "--author",
            "test",
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());

    let out = lib_api::run(
        &env,
        &[
            "show",
            "milestone",
            &id,
            "--fields",
            "milestone.priority",
            "--format",
            "json",
        ],
    );
    let pri: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()
        ["milestone"]["priority"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        pri, "urgent",
        "urgent priority must NOT downgrade on remediation entry; got {pri}"
    );
}

/// BF-14 (M131): the pre_state derived on remediation exit must be
/// ORDER-INDEPENDENT. On a milestone with mixed self + external
/// findings, resolving them in either order must yield the same exit
/// lifecycle. The CLI doesn't expose --phase external yet, so findings
/// are written directly to the milestone file as JSON (mirroring
/// `force_lifecycle_done`). An external finding means the milestone
/// came from the self-reviewed/reviewed track, so exit must be
/// "self-reviewed" regardless of resolution order.
fn seed_mixed_phase_remediation(env: &TestEnv, id: &str) {
    let dir = env.tmp.path().join("master-plan/milestones");
    let path: std::path::PathBuf = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with(id))
        .map(|e| e.path())
        .unwrap_or_else(|| panic!("milestone file not found for {id}"));
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["lifecycle"] = "remediation".into();
    // M189: active remediation origins are done/complete (self-reviewed is
    // a demoted alias that collapses to Done on restore).
    m["milestone"]["remediation_pre_state"] = "executed".into();
    m["milestone"]["priority"] = "high".into();
    m["findings"] = serde_json::json!([
        {
            "id": "F-01", "severity": "high", "category": "correctness",
            "description": "self-phase", "status": "open", "author": "test",
            "fixed_in": "", "created": "", "resolved": "", "phase": "self"
        },
        {
            "id": "F-02", "severity": "high", "category": "correctness",
            "description": "external-phase", "status": "open", "author": "test",
            "fixed_in": "", "created": "", "resolved": "", "phase": "external"
        }
    ]);
    let serialized = serde_json::to_string_pretty(&m).unwrap();
    std::fs::write(&path, format!("{serialized}\n")).unwrap();
}

fn read_lifecycle(env: &TestEnv, id: &str) -> String {
    let out = lib_api::run(
        env,
        &[
            "show",
            "milestone",
            id,
            "--fields",
            "milestone.lifecycle",
            "--format",
            "json",
        ],
    );
    assert!(out.status.success());
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["milestone"]["lifecycle"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn resolve_finding_pre_state_order_independent_self_resolved_first() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "bf14-self-first");
    seed_mixed_phase_remediation(&env, &id);

    // Resolve self-phase first, then external-phase.
    assert!(env
        .run(&["reviews", "finding", "resolve", &id, "F-01", "--format", "json"])
        .status
        .success());
    // After F-01, F-02 still open → still remediation.
    assert_eq!(read_lifecycle(&env, &id), "remediation");
    assert!(env
        .run(&["reviews", "finding", "resolve", &id, "F-02", "--format", "json"])
        .status
        .success());

    // External finding present → pre_state must restore the captured
    // delivery phase (done after M189 alias demotion).
    assert_eq!(
        read_lifecycle(&env, &id),
        "executed",
        "resolving self-first then external must exit to done (captured pre_state)"
    );
}

#[test]
fn resolve_finding_pre_state_order_independent_external_resolved_first() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "bf14-external-first");
    seed_mixed_phase_remediation(&env, &id);

    // Resolve external-phase first, then self-phase (opposite order).
    assert!(env
        .run(&["reviews", "finding", "resolve", &id, "F-02", "--format", "json"])
        .status
        .success());
    assert_eq!(read_lifecycle(&env, &id), "remediation");
    assert!(env
        .run(&["reviews", "finding", "resolve", &id, "F-01", "--format", "json"])
        .status
        .success());

    // Same exit state as the self-first order — order-independent.
    assert_eq!(
        read_lifecycle(&env, &id),
        "executed",
        "resolving external-first then self must exit to done (order-independent)"
    );
}

/// BF-14 review remediation (M131): the residual case the first BF-14
/// attempt missed. A milestone can carry a *resolved* external finding
/// (status=fixed, phase=external) in its history AND a later open self
/// finding that drove the most recent remediation entry. The entry-side
/// capture writes `remediation_pre_state = "done"`; the exit must replay
/// that value, landing on `done` — NOT `self-reviewed`. The first M131
/// attempt scanned the whole finding set for any external-phase finding,
/// which would have seen the resolved external one and wrongly exited to
/// `self-reviewed`. The on-disk `remediation_pre_state` field makes the
/// exit correct by construction.
#[test]
fn resolve_finding_pre_state_prefers_entry_capture_over_resolved_external() {
    let env = TestEnv::new();
    let id = create_milestone(&env, "bf14-residual");
    force_lifecycle_done(&env, &id);

    // Seed a RESOLVED external finding into history (written directly to
    // disk — the CLI doesn't expose --phase, and we need a finding that's
    // already fixed). The milestone stays at `done`. This is the finding
    // the old whole-set scan would have seen and misclassified on.
    let dir = env.tmp.path().join("master-plan/milestones");
    let path: std::path::PathBuf = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with(&id))
        .map(|e| e.path())
        .unwrap_or_else(|| panic!("milestone file not found for {id}"));
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["findings"] = serde_json::json!([{
        "id": "F-ext", "severity": "high", "category": "correctness",
        "description": "resolved external in history", "status": "fixed",
        "phase": "external", "author": "test",
        "fixed_in": "deadbeef", "resolved": "2026-07-09", "created": "2026-07-09"
    }]);
    let serialized = serde_json::to_string_pretty(&m).unwrap();
    std::fs::write(&path, format!("{serialized}\n")).unwrap();

    // Add an open self finding via the CLI so the real entry path fires:
    // done + has_open_self_findings → remediation, and the entry capture
    // writes remediation_pre_state = "done". Capture the new finding id.
    let add = lib_api::run(
        &env,
        &[
            "reviews",
            "finding",
            "add",
            &id,
            "--severity",
            "high",
            "--category",
            "correctness",
            "--desc",
            "open self drives remediation entry",
            "--format",
            "json",
        ],
    );
    assert!(
        add.status.success(),
        "add finding failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let new_id = added["finding"]["id"]
        .as_str()
        .expect("added finding id")
        .to_string();

    // Entry fired: lifecycle is remediation, pre_state captured as "done".
    assert_eq!(read_lifecycle(&env, &id), "remediation");

    // Resolve the open self finding. The exit must replay the entry-captured
    // pre_state ("done"), NOT the old whole-set scan ("self-reviewed").
    assert!(env
        .run(&["reviews", "finding", "resolve", &id, &new_id, "--format", "json"])
        .status
        .success());

    assert_eq!(
        read_lifecycle(&env, &id),
        "executed",
        "resolved-external-in-history + later self must exit to 'done' via entry-captured pre_state, not 'self-reviewed'"
    );

    // The field is consumed on exit — it must not linger on a
    // non-remediation milestone (keeps healthy-milestone JSON clean).
    let out = lib_api::run(&env, &["show", "milestone", &id, "--format", "raw"]);
    assert!(out.status.success());
    let raw_after = String::from_utf8_lossy(&out.stdout);
    assert!(
        !raw_after.contains("remediation_pre_state"),
        "remediation_pre_state must be cleared after exiting remediation; got: {raw_after}"
    );
}
