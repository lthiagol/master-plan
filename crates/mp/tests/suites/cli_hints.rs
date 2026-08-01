use crate::common::TestEnv;

/// BF-03: `mp milestone …` misroutes must produce one-line hints pointing at the
/// right command instead of dying on a bare `unrecognized subcommand '…'`.
#[test]
fn hint_for_milestone_list_misroute() {
    let env = TestEnv::new();
    let out = env.run(&["milestone", "list"]);
    assert!(
        !out.status.success(),
        "milestone list should fail (subcommand unknown)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("hint:") && stderr.contains("mp list milestones"),
        "expected `hint:` pointing at `mp list milestones`; got stderr:\n{stderr}"
    );
}

#[test]
fn hint_for_milestone_show_misroute() {
    let env = TestEnv::new();
    let out = env.run(&["milestone", "show"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("hint:") && stderr.contains("mp show milestone"),
        "expected `hint:` pointing at `mp show milestone <ID>`; got stderr:\n{stderr}"
    );
}

#[test]
fn hint_for_milestone_id_before_verb_misroute() {
    let env = TestEnv::new();
    let out = env.run(&["milestone", "M91", "show"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("hint:"),
        "expected `hint:` for id-before-verb misroute; got stderr:\n{stderr}"
    );
    // The hint must clearly point at `mp show milestone <ID>` (the read path).
    assert!(
        stderr.contains("mp show milestone"),
        "hint should reference `mp show milestone`; got stderr:\n{stderr}"
    );
}

#[test]
fn hint_suppressed_for_unrelated_misroutes() {
    let env = TestEnv::new();
    // Random non-id, non-list/show verb -> clap error must still surface,
    // but the milestone-specific hint must NOT fire (no false positives).
    let out = env.run(&["milestone", "totally-unknown-verb"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("hint:"),
        "milestone-specific hint should NOT fire for non-id-shaped unknown verbs; got stderr:\n{stderr}"
    );
}

#[test]
fn hint_suppressed_for_other_resources() {
    let env = TestEnv::new();
    // Same pattern but the resource isn't `milestone` -> no milestone hint.
    // `track list` itself is a valid command, so use an invalid track subcommand
    // to trigger clap rejection without the milestone hint.
    let out = env.run(&["track", "totally-unknown"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("mp list milestones") && !stderr.contains("mp show milestone"),
        "milestone-specific hint must NOT fire for unrelated track errors; got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("hint:"),
        "milestone-specific hint must NOT fire for unrelated resources; got stderr:\n{stderr}"
    );
}

#[test]
fn valid_milestone_command_still_works() {
    let env = TestEnv::new();
    // Sanity: a real milestone command (--help) must still succeed without our hint.
    let out = env.run(&["milestone", "--help"]);
    assert!(out.status.success(), "mp milestone --help should succeed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("hint:"),
        "successful runs must not leak a hint line; got stderr:\n{stderr}"
    );
}

#[test]
fn version_flag_still_exits_zero() {
    let env = TestEnv::new();
    // BF-03 regression check: switching Cli::parse() -> Cli::try_parse_from() required
    // routing DisplayHelp/DisplayVersion to exit 0. --version must keep doing that.
    let out = env.run(&["--version"]);
    assert!(out.status.success(), "mp --version should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains('.'), // version like "2.0.0-rc.6"
        "--version stdout should print the version string; got: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("hint:"),
        "--version must not emit a milestone hint; got stderr:\n{stderr}"
    );
}
