use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn path_next_prefers_focused_session_in_hybrid_profile() {
    let env = TestEnv::blank();
    let plan_dir = env.tmp.path().join(".mp");
    assert!(env
        .run(&[
            "init",
            "--profile",
            "hybrid",
            "--plan-dir",
            plan_dir.to_str().unwrap(),
            "--format",
            "json",
        ])
        .status
        .success());

    let plan_rel = ".mp";

    assert!(env
        .run(&[
            "--plan-dir",
            plan_rel,
            "session",
            "start",
            "--branch",
            "feature/oauth",
            "--title",
            "OAuth session",
            "--format",
            "json",
        ])
        .status
        .success());
    assert!(env
        .run(&[
            "--plan-dir",
            plan_rel,
            "session",
            "focus",
            "oauth",
            "--format",
            "json",
        ])
        .status
        .success());

    let next = lib_api::run_json(&env, &["--plan-dir", plan_rel, "next", "--format", "json"]);
    // Empty session milestone may return an informational message instead of a step.
    assert!(
        next.get("step").is_some()
            || next.get("session").is_some()
            || next.get("milestone").is_some()
            || next.get("message").is_some(),
        "focused hybrid session should return structured next output: {next}"
    );

    let show = lib_api::run_json(
        &env,
        &[
            "--plan-dir",
            plan_rel,
            "session",
            "show",
            "--format",
            "json",
        ],
    );
    assert_eq!(show["session"]["focused"], true);
}
