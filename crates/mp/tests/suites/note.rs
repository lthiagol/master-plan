use crate::common::TestEnv;

#[test]
fn note_add_creates_meeting_idea() {
    let env = TestEnv::blank();
    assert!(env
        .run(&["init", "--profile", "hybrid", "--format", "json"])
        .status
        .success());

    let note = env.run(&[
        "note",
        "add",
        "--title",
        "Sprint review notes",
        "--body",
        "Ship next milestone",
        "--format",
        "json",
    ]);
    assert!(
        note.status.success(),
        "{}",
        String::from_utf8_lossy(&note.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&note.stdout).unwrap();
    assert_eq!(json["source"], "meeting");
}
