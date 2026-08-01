use crate::common::lib_api;
use crate::common::TestEnv;

#[test]
fn plan_principles_add_appends() {
    let env = TestEnv::new();

    let out = lib_api::run(
        &env,
        &[
            "plan",
            "principles",
            "add",
            "Always spec before code",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let principles = json["principles"].as_array().unwrap();
    assert!(principles
        .iter()
        .any(|p| p.as_str().unwrap().contains("Always spec before code")));
}
