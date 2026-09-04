//! M151 S4 / AC-04 — unknown-harness error path.
//!
//! `HarnessRegistry::get` returns a structured `HarnessError`
//! that names the offender, lists the supported harnesses, and
//! points at the on-ramp `herdr integration install <name>`
//! command. The same message must flow through:
//! - `mp agent harness start-command <unknown>` (CLI surface),
//! - `mp watch` precondition gate (M149 S0 / S4 wiring).
//!
//! The black-box check here pins `mp watch`: hand-edit
//! `config.json` to inject an unsupported harness, run
//! `mp watch <id>`, and assert exit non-zero + the install hint.

mod common;

use common::TestEnv;
use serde_json::json;

/// Hand-edit `agent.<role>.harness` in the on-disk config to
/// bypass `mp config set` validation. Returns the new harness value
/// so the test can run assertions against it.
fn inject_unknown_harness(env: &TestEnv, role: &str, name: &str) {
    let config_path = env.tmp.path().join("master-plan").join("config.json");
    let raw = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", config_path.display()));
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    // On-disk schema is flat: `{workflow, output, ..., agent}` —
    // the `config` envelope is only on the `config show` CLI
    // output. Future-proof the path so a migration to a nested
    // schema does not silently shadow the test.
    let agent = v
        .get_mut("agent")
        .and_then(|a| a.get_mut(role))
        .unwrap_or_else(|| panic!("missing agent.{role} in {raw}"));
    agent["harness"] = json!(name);
    std::fs::write(&config_path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

#[test]
fn autopilot_start_with_unknown_runner_harness_exits_non_zero_with_install_hint() {
    let env = TestEnv::new();
    // Seed a valid coordinator so the runner check is the only
    // failure mode; a future regression that accidentally
    // reports the wrong check would trip the assertions below.
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.coordinator.harness", "opencode"]);

    inject_unknown_harness(&env, "runner", "claude");

    let out = env.run(&["autopilot", "start", "151", "--format", "json"]);
    assert!(
        !out.status.success(),
        "watch with unknown harness must exit non-zero: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // Five contracts must hold simultaneously:
    // 1. Offender named.
    assert!(
        combined.contains("claude"),
        "watch error must name the offending harness: {combined}"
    );
    // 2. Supported list present.
    assert!(
        combined.contains("opencode") && combined.contains("cursor"),
        "watch error must list supported harnesses: {combined}"
    );
    // 3. Install on-ramp present.
    assert!(
        combined.contains("herdr integration install claude"),
        "watch error must suggest the install on-ramp: {combined}"
    );
    // 4. Precondition gate flags the failure.
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ok = report["preconditions"]["ok"].as_bool().unwrap_or(true);
    assert!(!ok, "preconditions.ok must be false: {report}");
    // 5. The failing check is the runner config (not
    //    accidentally the coordinator or log-path check).
    let checks = report["preconditions"]["checks"].as_array().unwrap();
    let runner = checks
        .iter()
        .find(|c| c["name"] == "runner_config_present")
        .expect("runner_config_present check present");
    assert_eq!(
        runner["ok"],
        json!(false),
        "runner check must fail when harness is unknown"
    );
}

#[test]
fn autopilot_start_with_unknown_coordinator_harness_surfaces_install_hint() {
    let env = TestEnv::new();
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.coordinator.harness", "opencode"]);
    inject_unknown_harness(&env, "coordinator", "aider");

    let out = env.run(&["autopilot", "start", "151", "--format", "json"]);
    assert!(
        !out.status.success(),
        "watch with bad coordinator harness must exit non-zero"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("aider") && combined.contains("herdr integration install aider"),
        "error must mention aider + the on-ramp: {combined}"
    );
}

#[test]
fn autopilot_start_with_unknown_harness_never_panics_or_hangs() {
    // A truly pathological harness value — empty string — must
    // still produce a structured precondition failure, not a
    // panic or stack overflow. This guards the registry's
    // panic-free contract ([AC-04]).
    let env = TestEnv::new();
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.coordinator.harness", "opencode"]);
    inject_unknown_harness(&env, "runner", "");

    let out = env.run(&["autopilot", "start", "151", "--format", "json"]);
    assert!(
        !out.status.success(),
        "watch with empty harness string must fail gracefully"
    );
    // Empty string should still land in the registry's
    // Unsupported path (no entry matches the empty id), so the
    // error names every supported harness.
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("opencode") && combined.contains("pi") && combined.contains("cursor"),
        "every supported harness must appear: {combined}"
    );
}

#[test]
fn registry_unknown_error_message_contains_all_three_v1_harnesses() {
    // Library-level contract: the message string assembled by
    // `HarnessError::Unsupported` lists every v1 harness in a
    // stable order and the install hint. This guards the
    // formatting against typos in the Display impl.
    let reg = mp::harness::HarnessRegistry::v1();
    for bad in ["claude-code", "windsurf", "kiro"] {
        let err = reg.get(bad).unwrap_err();
        let msg = format!("{err}");
        // Ordering check — supported list precedes the install
        // hint in the formatted message.
        let supported_pos = msg.find("supported:").expect("supported: present");
        let install_pos = msg
            .find("herdr integration install")
            .expect("install hint present");
        assert!(
            supported_pos < install_pos,
            "supported list must appear before the install hint \
             in the error message ({bad}): {msg}"
        );
        // The install hint must include the bad name verbatim
        // (claude-code → `herdr integration install claude-code`).
        let needle = format!("herdr integration install {bad}");
        assert!(
            msg.contains(&needle),
            "install hint must mention the bad name {bad}: {msg}"
        );
    }
}
