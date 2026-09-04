//! M208 / S01 / AC-01: `mp autopilot --help` lists the full autopilot
//! verb tree (start, status, stop, output, result, config, session)
//! with descriptions matching today's `mp watch` and `mp watch-control`
//! sub-commands.
//!
//! Black-box coverage of the CLI surface:
//! - each verb appears in the help output (sanity: the verb tree is
//!   advertised, not hidden behind a flag)
//! - each verb's short description echoes the legacy surface so a
//!   `mp watch` user can find the new command
//! - `--help` exits 0 with a non-empty stdout

mod common;

use common::TestEnv;

fn help_stdout(env: &TestEnv, args: &[&str]) -> String {
    let out = env.run(args);
    assert!(
        out.status.success(),
        "mp {} --help should exit 0; stderr={}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn autopilot_help_lists_every_required_verb() {
    let env = TestEnv::new();
    let help = help_stdout(&env, &["autopilot", "--help"]);
    for verb in [
        "start", "status", "stop", "output", "result", "config", "session",
    ] {
        assert!(
            help.contains(verb),
            "`mp autopilot --help` should list the `{verb}` verb; got:\n{help}"
        );
    }
}

#[test]
fn autopilot_help_describes_start_as_replacement_for_watch() {
    let env = TestEnv::new();
    let help = help_stdout(&env, &["autopilot", "--help"]);
    // The `start` verb must be advertised as the replacement for
    // `mp watch <ids...>` so users running muscle-memory commands
    // can find their way to the new tree.
    let start_idx = help.find("start").expect("start verb listed");
    let snippet = &help[start_idx..];
    assert!(
        snippet.contains("watch") || snippet.contains("lifecycle"),
        "start verb description should mention watch / lifecycle for migration discoverability; got snippet: {snippet}"
    );
}

#[test]
fn autopilot_help_status_describes_control_plane() {
    // M229: the legacy `mp watch-control status` was removed by the
    // breaking-release cleanup. The canonical `mp autopilot status`
    // verb must describe its control-plane read contract without
    // referencing the removed alias.
    let env = TestEnv::new();
    let help = help_stdout(&env, &["autopilot", "--help"]);
    let status_idx = help.find("status").expect("status verb listed");
    let snippet = &help[status_idx..];
    assert!(
        snippet.contains("control")
            || snippet.contains("queue")
            || snippet.contains("lifecycle")
            || snippet.contains("state"),
        "status verb description should advertise its control-plane read contract: {snippet}"
    );
}

#[test]
fn autopilot_help_stop_describes_graceful_signal() {
    // M229: the legacy `mp watch-control stop` was removed by the
    // breaking-release cleanup. The canonical `mp autopilot stop`
    // verb must still describe its graceful-signal contract — a
    // SIGINT to the recorded PID. The pre-M229 wording referenced
    // the removed alias; the canonical surface is described here
    // without it.
    let env = TestEnv::new();
    let help = help_stdout(&env, &["autopilot", "--help"]);
    let stop_idx = help.find("stop").expect("stop verb listed");
    let snippet = &help[stop_idx..];
    assert!(
        snippet.contains("graceful")
            || snippet.contains("signal")
            || snippet.contains("PID")
            || snippet.contains("pid"),
        "stop verb description should advertise the graceful-signal contract: {snippet}"
    );
}

#[test]
fn autopilot_help_output_and_result_describe_pane_and_outcome_verbs() {
    let env = TestEnv::new();
    let help = help_stdout(&env, &["autopilot", "--help"]);
    let output_idx = help.find("output").expect("output verb listed");
    let output_snippet = &help[output_idx..];
    assert!(
        output_snippet.contains("pane") || output_snippet.contains("read"),
        "output verb description should mention bounded pane output; got: {output_snippet}"
    );
    let result_idx = help.find("result").expect("result verb listed");
    let result_snippet = &help[result_idx..];
    assert!(
        result_snippet.contains("outcome") || result_snippet.contains("terminal"),
        "result verb description should mention the terminal outcome; got: {result_snippet}"
    );
}

#[test]
fn autopilot_start_help_matches_watch_help_for_shared_args() {
    // S02 contract: `mp autopilot start` accepts the same args as the
    // legacy `mp watch <ids...>`. The help text should surface every
    // shared flag (--dry-run, --log-file, --stall-timeout-ms,
    // --poll-interval-ms, --resume, --force, --detach) so a user
    // migrating from `mp watch` can find the matching flag on
    // `mp autopilot start`.
    let env = TestEnv::new();
    let help = help_stdout(&env, &["autopilot", "start", "--help"]);
    for flag in [
        "--dry-run",
        "--log-file",
        "--stall-timeout-ms",
        "--poll-interval-ms",
        "--resume",
        "--force",
        "--detach",
    ] {
        assert!(
            help.contains(flag),
            "`mp autopilot start --help` should mention {flag}; got:\n{help}"
        );
    }
}
