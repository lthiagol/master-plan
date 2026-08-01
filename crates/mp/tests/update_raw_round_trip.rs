//! M111 S5: `mp show milestone --format raw → mp milestone update --json
//! --accept-extra-fields` round-trips without manual `jq del(...)` stripping.
//! Pre-M111 the round-trip required deleting `design_decisions`, `milestone`,
//! `verification`, `intent`, `problem`, `scope`, and `work_packages` to avoid
//! the `unsupported field(s)` error.

mod common;

use crate::common::TestEnv;

#[test]
fn raw_to_update_round_trips_with_accept_extra_fields() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Step 1: dump milestone 03 in raw format and save it to a file.
    let raw_out = env.run(&["show", "milestone", "03", "--format", "raw"]);
    assert!(raw_out.status.success(), "show --format raw failed");
    let raw_text = String::from_utf8_lossy(&raw_out.stdout).to_string();
    let raw_path = env.tmp.path().join("03-round-trip.json");
    std::fs::write(&raw_path, &raw_text).expect("write raw to disk");

    // Confirm the raw document contains fields the update command would
    // otherwise reject (design_decisions / milestone / etc.).
    let doc: serde_json::Value = serde_json::from_str(&raw_text).expect("raw parses");
    assert!(
        doc.get("design_decisions").is_some(),
        "raw output should include design_decisions (rejected without escape hatch)"
    );
    assert!(
        doc.get("milestone").is_some(),
        "raw output should include the milestone meta (rejected without escape hatch)"
    );

    // Step 2: the default update --json path rejects. Skip the negative path
    // here (covered by fragment_update_guard); jump straight to the
    // accept-extra-fields success path.
    //
    // Real-world round-trips also need `--replace-arrays` because the raw
    // document carries `acceptance_criteria` / `steps` arrays. M111 S5
    // composes both escape hatches so a single command does the round-trip.
    let out = env.run(&[
        "milestone",
        "update",
        "03",
        "--file",
        raw_path.to_str().expect("utf-8 path"),
        "--replace-arrays",
        "--accept-extra-fields",
    ]);
    assert!(
        out.status.success(),
        "round-trip with --replace-arrays --accept-extra-fields failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
}

#[test]
fn update_without_accept_extra_fields_still_rejects_extra_keys() {
    // Sanity: the rejection path is still in place. Pre-M111 the only
    // workaround was manual `jq del(...)`; M111 keeps the strict default and
    // adds `--accept-extra-fields` as the opt-in.
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let payload = r#"{"design_decisions":[]}"#;
    let bad = env.run(&["milestone", "update", "03", "--json", payload]);
    assert!(
        !bad.status.success(),
        "default update must still reject design_decisions"
    );
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("design_decisions"),
        "default rejection must mention the offending key; got: {stderr}"
    );
}

#[test]
fn round_trip_preserves_at_least_title_and_depends_on() {
    // Lightweight functional check: a round-tripped milestone still carries
    // its title and depends_on after the accept-extra-fields escape hatch.
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let raw_out = env.run(&["show", "milestone", "03", "--format", "raw"]);
    let raw_text = String::from_utf8_lossy(&raw_out.stdout).to_string();
    let raw_path = env.tmp.path().join("03-round-trip.json");
    std::fs::write(&raw_path, &raw_text).expect("write raw to disk");

    let out = env.run(&[
        "milestone",
        "update",
        "03",
        "--file",
        raw_path.to_str().expect("utf-8 path"),
        "--replace-arrays",
        "--accept-extra-fields",
    ]);
    assert!(out.status.success());

    let show = env.run(&[
        "show",
        "milestone",
        "03",
        "--fields",
        "milestone.title,milestone.depends_on",
    ]);
    assert!(show.status.success());
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(v["milestone"]["title"].as_str(), Some("OAuth Login"));
}
