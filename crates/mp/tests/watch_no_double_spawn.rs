//! M152 S3 / AC-03: `mp watch` default refuses to double-spawn when
//! role panes already exist; `--force` opts in to the override.
//!
//! The test installs a fake `herdr` script that fakes a populated
//! `agent list` (showing both `role-runner-1` and
//! `role-coordinator-1`), then runs `mp watch <id>` against a real
//! milestone and asserts:
//! - **default**: exits non-zero with the structured error message
//!   naming `--resume` and `--force` (the operative fix paths).
//! - **`--force`**: passes the gate (the precondition check still
//!   runs against the fake herdr that lists the panes; with
//!   `--force` the gate is bypassed).
//!
//! The companion `mp watch --resume` reattach behavior is covered
//! in `crates/mp/tests/watch_resume.rs` via the pure reconciler
//! layer; this file is specifically the gate / refusal contract.

mod common;

use crate::common::TestEnv;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn install_fake_herdr_with_existing_panes(dir: &Path) -> PathBuf {
    // The fake herdr reports BOTH role-* panes as live so the
    // reconciliation gate sees them and refuses to spawn a second
    // copy. start/send/etc. exist as no-ops so any *additional*
    // invocation after --force or --resume doesn't error out.
    //
    // M197 followup: also answer `agent start --help` and
    // `pane split --help` with the 0.7.x flag list so the
    // `herdr_cli_shape` precondition passes; otherwise the
    // precondition gate fires *before* the double-spawn gate and
    // the test never exercises the refusal path it was written to
    // pin.
    let bin = dir.join("herdr");
    let body = r#"#!/bin/sh
case "$1:$2:$3" in
  agent:start:--help)
    cat <<'HELP'
Usage: herdr agent start <NAME> --kind <KIND> --pane <ID>

Options:
  --kind <KIND>  Harness kind
  --pane <ID>    Existing pane id
HELP
    ;;
  pane:split:--help)
    echo "Usage: herdr pane split [OPTIONS]"
    ;;
esac
case "$2" in
  list)
    cat <<'JSON'
{"agents":[
  {"name":"role-runner-1","pane_id":"%5"},
  {"name":"role-coordinator-1","pane_id":"%7"}
]}
JSON
    ;;
  start)
    echo '{"pane_id":"%NEW","status":"started"}'
    ;;
  send)
    exit 0
    ;;
  send-keys)
    exit 0
    ;;
  wait)
    echo '{"status":"idle"}'
    ;;
  read)
    echo ""
    ;;
  status)
    echo '{"status":"idle"}'
    ;;
  *)
    echo ok
    ;;
esac
"#;
    fs::write(&bin, body).unwrap();
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
    bin
}

fn path_with(dir: &Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<PathBuf> = std::env::split_paths(&existing).collect();
    parts.insert(0, dir.to_path_buf());
    std::env::join_paths(parts)
        .expect("joined PATH")
        .to_string_lossy()
        .into_owned()
}

fn seed_approved_milestone(env: &TestEnv, title: &str) -> String {
    let json = format!(
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
    let created = env.run_json(&["milestone", "create", "--json", &json, "--format", "json"]);
    let id = created["milestone"]["id"].as_str().unwrap().to_string();
    env.run(&["milestone", "set-spec-status", &id, "ready"]);
    env.run(&["milestone", "approve", &id, "--format", "json"]);
    // Pre-M152 milestone tests had no harness requirement;
    // M152's watch --resume / --force only fire AFTER the
    // precondition gate, so the harness must be wired first.
    env.run(&["config", "set", "agent.runner.harness", "opencode"]);
    env.run(&["config", "set", "agent.coordinator.harness", "opencode"]);
    id
}

#[test]
fn default_watch_refuses_to_double_spawn_existing_role_panes() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    install_fake_herdr_with_existing_panes(&bin_dir);
    let id = seed_approved_milestone(&env, "double-spawn-guard");

    let out = env.run_with_env(
        &[("PATH", &path_with(&bin_dir))],
        &["watch", &id, "--format", "json"],
    );
    assert!(
        !out.status.success(),
        "default mp watch must exit non-zero when role panes \
         already exist: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );

    // The structured report on stdout carries the gate verdict;
    // the human-readable message on stderr carries the fix hint.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}\n{stderr}");

    // Each fix path is named explicitly so an agent (or user) can
    // pick the right one without re-reading docs.
    assert!(
        combined.contains("--resume") && combined.contains("--force"),
        "refusal message must point at both --resume and --force: {combined}"
    );
    // The offender is named (so the user knows what was already
    // there) — at least one role pane id surfaces in the report.
    assert!(
        combined.contains("%5") || combined.contains("%7"),
        "refusal must name the live pane ids: {combined}"
    );
}

#[test]
fn force_flag_bypasses_double_spawn_gate() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    install_fake_herdr_with_existing_panes(&bin_dir);
    let id = seed_approved_milestone(&env, "force-bypass");

    let out = env.run_with_env(
        &[("PATH", &path_with(&bin_dir))],
        &[
            "watch",
            &id,
            "--force",
            "--stall-timeout-ms",
            "10",
            "--format",
            "json",
        ],
    );
    // The fake herdr doesn't actually advance the milestone —
    // the run will likely stall and time out, but it must NOT
    // refuse at the gate. exit-code-wise we accept anything
    // except the "double_spawn_refused" shape.
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("use --resume to re-attach, or --force to ignore"),
        "--force must skip the double-spawn gate; combined={combined}"
    );
}

#[test]
fn resume_flag_passes_gate_and_progresses_past_refusal_check() {
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    install_fake_herdr_with_existing_panes(&bin_dir);
    let id = seed_approved_milestone(&env, "resume-attach");

    let out = env.run_with_env(
        &[("PATH", &path_with(&bin_dir))],
        &[
            "watch",
            &id,
            "--resume",
            "--stall-timeout-ms",
            "10",
            "--format",
            "json",
        ],
    );
    // --resume clears the gate. The fake herdr lists two live
    // panes; the gate check is satisfied (gate exists ONLY when
    // neither --resume nor --force is set). The run will likely
    // stall waiting for lifecycle advances the fake cannot
    // produce, but the run itself must not refuse at startup.
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("existing role pane for the active milestones"),
        "--resume must skip the gate; combined={combined}"
    );
}

#[test]
fn no_existing_panes_no_gate_default_runs_clean() {
    // Baseline regression: with no live panes (default case), the
    // default `mp watch` does not engage the gate at all. Pre-M152
    // behavior is preserved.
    let env = TestEnv::new();
    let bin_dir = env.tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    // Fake herdr that lists NO role panes so the gate never sees
    // any Live panes to refuse.
    let bin = bin_dir.join("herdr");
    let body = r#"#!/bin/sh
case "$2" in
  list) echo '{"agents":[]}' ;;
  start) echo '{"pane_id":"%N","status":"started"}' ;;
  send) exit 0 ;;
  send-keys) exit 0 ;;
  wait) echo '{"status":"idle"}' ;;
  read) echo "" ;;
  status) echo '{"status":"idle"}' ;;
  *) echo ok ;;
esac
"#;
    fs::write(&bin, body).unwrap();
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();

    let id = seed_approved_milestone(&env, "baseline-clean");
    let out = env.run_with_env(
        &[("PATH", &path_with(&bin_dir))],
        &["watch", &id, "--stall-timeout-ms", "10", "--format", "json"],
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("double_spawn_refused")
            && !combined.contains("use --resume to re-attach, or --force to ignore"),
        "no panes case must NOT trip the gate; combined={combined}"
    );
}
