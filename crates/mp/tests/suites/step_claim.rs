//! Step claim / lease and mp next skip (M70).

use crate::common::lib_api;
use crate::common::TestEnv;

fn setup_claimable_milestone(env: &TestEnv) -> String {
    let create = lib_api::run(
        env,
        &[
            "milestone",
            "create",
            "--title",
            "Claim fixture",
            "--json",
            r#"{
            "title":"Claim fixture",
            "intent":{"outcome":"x"},
            "problem":{"description":"y"},
            "scope":{"in_scope":["a"],"out_of_scope":["b","c"]},
            "acceptance_criteria":[{"description":"ac","verification":"manual: ok"}]
        }"#,
        ],
    );
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    let id = serde_json::from_slice::<serde_json::Value>(&create.stdout).unwrap()["milestone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    lib_api::run(env, &["milestone", "approve", &id]);
    lib_api::run(
        env,
        &["milestone", "decompose", &id, "--work-packages", "1"],
    );
    lib_api::run(
        env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "First",
            "--tests",
            "manual: ok",
            "--done-when",
            "done",
        ],
    );
    lib_api::run(
        env,
        &[
            "milestone",
            "step",
            "add",
            &id,
            "--wp",
            "WP1",
            "--action",
            "Second",
            "--tests",
            "manual: ok",
            "--done-when",
            "done",
        ],
    );
    lib_api::run(env, &["milestone", "set-status", &id, "in-progress"]);
    id
}

#[test]
fn claim_and_release_persist_on_step() {
    let env = TestEnv::new();
    let id = setup_claimable_milestone(&env);

    let claim = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "claim",
            &id,
            "S1",
            "--by",
            "agent-a",
            "--lease",
            "1h",
        ],
    );
    assert!(
        claim.status.success(),
        "{}",
        String::from_utf8_lossy(&claim.stderr)
    );
    let step = &serde_json::from_slice::<serde_json::Value>(&claim.stdout).unwrap()["step"];
    assert_eq!(step["claimed_by"], "agent-a");
    assert!(!step["lease_expires_at"].as_str().unwrap_or("").is_empty());

    let release = lib_api::run(&env, &["milestone", "step", "release", &id, "S1"]);
    assert!(release.status.success());
    let step = &serde_json::from_slice::<serde_json::Value>(&release.stdout).unwrap()["step"];
    assert_eq!(step["claimed_by"], "");
}

#[test]
fn mp_next_skips_actively_claimed_step() {
    let env = TestEnv::new();
    let id = setup_claimable_milestone(&env);

    lib_api::run(
        &env,
        &["milestone", "step", "claim", &id, "S1", "--by", "agent-a"],
    );

    let next = lib_api::run(&env, &["next", "--format", "json"]);
    assert!(
        next.status.success(),
        "{}",
        String::from_utf8_lossy(&next.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&next.stdout).unwrap();
    let step_id = json["step"]["id"].as_str().unwrap_or("");
    assert_ne!(step_id, "S1", "mp next should skip claimed S1, got {json}");

    lib_api::run(&env, &["milestone", "step", "release", &id, "S1"]);
    let next2 = lib_api::run(&env, &["next", "--format", "json"]);
    assert!(next2.status.success());
    let json2: serde_json::Value = serde_json::from_slice(&next2.stdout).unwrap();
    assert_eq!(json2["step"]["id"], "S1");
}

#[test]
fn expired_lease_is_ignored_by_mp_next() {
    let env = TestEnv::new();
    let id = setup_claimable_milestone(&env);

    let claim = lib_api::run(
        &env,
        &[
            "milestone",
            "step",
            "claim",
            &id,
            "S1",
            "--by",
            "agent-a",
            "--lease",
            "1m",
        ],
    );
    assert!(claim.status.success());
    let expires = serde_json::from_slice::<serde_json::Value>(&claim.stdout).unwrap()["step"]
        ["lease_expires_at"]
        .as_str()
        .unwrap()
        .to_string();
    let milestones_dir = env.tmp.path().join("master-plan/milestones");
    let milestone_path = std::fs::read_dir(&milestones_dir)
        .expect("read milestones")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{id}-")))
        })
        .expect("milestone file");
    let content = std::fs::read_to_string(&milestone_path).expect("read milestone");
    std::fs::write(
        &milestone_path,
        content.replace(&expires, "2000-01-01T00:00:00+00:00"),
    )
    .expect("write milestone");

    let next = lib_api::run(&env, &["next", "--format", "json"]);
    assert!(next.status.success());
    let json: serde_json::Value = serde_json::from_slice(&next.stdout).unwrap();
    assert_eq!(json["step"]["id"], "S1");
}

#[test]
fn mp_path_lists_claimed_step_in_blocked() {
    let env = TestEnv::new();
    let id = setup_claimable_milestone(&env);

    lib_api::run(
        &env,
        &["milestone", "step", "claim", &id, "S1", "--by", "agent-b"],
    );

    let path = lib_api::run(&env, &["path", "--format", "json"]);
    assert!(
        path.status.success(),
        "{}",
        String::from_utf8_lossy(&path.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&path.stdout).unwrap();
    let blocked = json["blocked"].as_array().expect("blocked");
    assert!(
        blocked.iter().any(|b| {
            b["step"] == "S1" && b["reason"] == "step claimed" && b["claimed_by"] == "agent-b"
        }),
        "path should block claimed S1: {json}"
    );
    let first_action = json["actions"]
        .as_array()
        .and_then(|a| a.first())
        .expect("path action");
    assert_ne!(first_action["step"]["id"], "S1");
}
