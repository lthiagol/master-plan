use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn session_focus_sets_active_session() {
    let env = TestEnv::new();

    lib_api::run(
        &env,
        &[
            "session",
            "start",
            "--branch",
            "feature/auth",
            "--title",
            "Auth",
            "--format",
            "json",
        ],
    );

    let out = lib_api::run_json(&env, &["session", "focus", "auth", "--format", "json"]);
    assert_eq!(out["focused"], "auth");

    let list = lib_api::run_json(&env, &["session", "list", "--format", "json"]);
    let sessions = list["sessions"].as_array().unwrap();
    let auth: Vec<_> = sessions.iter().filter(|s| s["id"] == "auth").collect();
    assert_eq!(auth.len(), 1);
    assert_eq!(auth[0]["focused"], true);
}

#[test]
fn session_unfocus_clears_focus() {
    let env = TestEnv::new();

    lib_api::run(
        &env,
        &[
            "session",
            "start",
            "--branch",
            "feature/auth",
            "--title",
            "Auth",
            "--format",
            "json",
        ],
    );
    lib_api::run(&env, &["session", "focus", "auth", "--format", "json"]);
    lib_api::run(&env, &["session", "unfocus", "--format", "json"]);

    let list = lib_api::run_json(&env, &["session", "list", "--format", "json"]);
    let sessions = list["sessions"].as_array().unwrap();
    let auth: Vec<_> = sessions.iter().filter(|s| s["id"] == "auth").collect();
    assert_eq!(auth[0]["focused"], false);
}

#[test]
fn session_show_no_id_resolves_focused() {
    let env = TestEnv::new();

    lib_api::run(
        &env,
        &[
            "session",
            "start",
            "--branch",
            "feature/auth",
            "--title",
            "Auth",
            "--format",
            "json",
        ],
    );
    lib_api::run(
        &env,
        &[
            "session",
            "start",
            "--branch",
            "feature/billing",
            "--title",
            "Billing",
            "--format",
            "json",
        ],
    );
    lib_api::run(&env, &["session", "focus", "auth", "--format", "json"]);

    let out = lib_api::run_json(&env, &["session", "show", "--format", "json"]);
    assert_eq!(out["session"]["id"], "auth");
    assert_eq!(out["session"]["focused"], true);
}

#[test]
fn session_focus_rejects_unknown_session() {
    let env = TestEnv::new();

    let out = lib_api::run(&env, &["session", "focus", "nonesuch", "--format", "json"]);
    assert!(!out.status.success());
}

#[test]
fn hybrid_idea_create_and_session_start() {
    let env = TestEnv::blank();
    let plan_dir = ".mp";

    assert!(env
        .run(&[
            "init",
            "--profile",
            "hybrid",
            "--plan-dir",
            plan_dir,
            "--format",
            "json",
        ])
        .status
        .success());

    let idea = lib_api::run_json(
        &env,
        &[
            "--plan-dir",
            plan_dir,
            "idea",
            "create",
            "--title",
            "Dark mode",
            "--format",
            "json",
        ],
    );
    assert_eq!(idea["idea"]["title"], "Dark mode");

    let session = lib_api::run(
        &env,
        &[
            "--plan-dir",
            plan_dir,
            "session",
            "start",
            "--branch",
            "feature/test",
            "--title",
            "Test session",
            "--format",
            "json",
        ],
    );
    assert!(
        session.status.success(),
        "{}",
        String::from_utf8_lossy(&session.stderr)
    );
    let session_json: serde_json::Value = serde_json::from_slice(&session.stdout).unwrap();
    assert_eq!(session_json["session_id"], "test");
}
