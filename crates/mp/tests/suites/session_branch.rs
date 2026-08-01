use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn session_start_resumes_same_branch() {
    let env = TestEnv::blank();
    assert!(env
        .run(&[
            "init",
            "--profile",
            "hybrid",
            "--plan-dir",
            "master-plan",
            "--format",
            "json",
        ])
        .status
        .success());

    let start1 = lib_api::run(
        &env,
        &[
            "--plan-dir",
            "master-plan",
            "session",
            "start",
            "--branch",
            "feature/oauth",
            "--format",
            "json",
        ],
    );
    assert!(start1.status.success());
    let j1: serde_json::Value = serde_json::from_slice(&start1.stdout).unwrap();
    assert_eq!(j1["resumed"], false);

    let start2 = lib_api::run(
        &env,
        &[
            "--plan-dir",
            "master-plan",
            "session",
            "start",
            "--branch",
            "feature/oauth",
            "--format",
            "json",
        ],
    );
    assert!(start2.status.success());
    let j2: serde_json::Value = serde_json::from_slice(&start2.stdout).unwrap();
    assert_eq!(j2["resumed"], true);
    assert_eq!(j1["session_id"], j2["session_id"]);
}
