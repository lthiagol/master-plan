use crate::common::TestEnv;

#[test]
fn hybrid_next_returns_session_step_when_tracks_done() {
    let env = TestEnv::from_fixture("hybrid-work");
    let next = env.run(&["--plan-dir", ".mp", "next", "--format", "json"]);
    assert!(
        next.status.success(),
        "{}",
        String::from_utf8_lossy(&next.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&next.stdout).unwrap();
    assert!(
        json.get("step").is_some() || json.get("item").is_some(),
        "next should return a step or track item, got: {json}"
    );
}
