use crate::common::TestEnv;

#[test]
fn idea_create_warns_on_similar_open_title() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "hybrid", "--format", "json"])
        .status
        .success());

    assert!(env
        .run(&["idea", "create", "--title", "Dark mode", "--format", "json",])
        .status
        .success());

    let dup = env.run(&["idea", "create", "--title", "Dark Mode", "--format", "json"]);
    assert!(dup.status.success());
    let stderr = String::from_utf8_lossy(&dup.stderr);
    assert!(
        stderr.contains("idea dup-check warning"),
        "expected dup-check warning on stderr, got: {stderr}"
    );
}
