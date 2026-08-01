use crate::common::TestEnv;

#[test]
fn interview_checklist_draft_returns_questions_without_id() {
    let env = TestEnv::new();

    let out = env.run(&[
        "interview",
        "checklist",
        "--checklist-type",
        "milestone",
        "--draft",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !json["suggested_questions"].as_array().unwrap().is_empty(),
        "draft should return suggested questions"
    );
    assert!(!json["type"].as_str().unwrap().is_empty());
}

#[test]
fn interview_checklist_draft_track_item() {
    let env = TestEnv::new();

    let out = env.run(&[
        "interview",
        "checklist",
        "--checklist-type",
        "track-item",
        "--draft",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!json["suggested_questions"].as_array().unwrap().is_empty());
}
