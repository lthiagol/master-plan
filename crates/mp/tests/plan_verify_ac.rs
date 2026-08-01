use common::TestEnv;

mod common;

#[test]
fn verify_ac_unresolvable_symbol_surfaces_precise_error() {
    let env = TestEnv::new();

    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"Bogus Test","intent":{"outcome":"test verify-ac"},"scope":{"in_scope":["a"],"out_of_scope":["b","c"]}}"#,
        "--format",
        "json",
    ]);
    assert!(
        create.status.success(),
        "milestone create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&create.stdout).expect("milestone create JSON");
    let milestone_id = v["milestone"]["id"].as_str().unwrap().to_string();

    let ac_add = env.run(&[
        "milestone",
        "ac",
        "add",
        &milestone_id,
        "--description",
        "bogus target",
        "--verification",
        "cargo test -p nonexistent_crate --test nonexistent_test_target",
        "--format",
        "json",
    ]);
    assert!(
        ac_add.status.success(),
        "ac add failed: {}",
        String::from_utf8_lossy(&ac_add.stderr)
    );

    let out = env.run(&["plan", "verify-ac", &milestone_id, "--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "command exited non-zero: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        stdout
    );

    let v2: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));
    assert_eq!(v2["ok"], false, "should report ok: false");
    assert_eq!(v2["unresolvable"], 1, "should have one unresolvable");
    let acs = v2["acs"].as_array().unwrap();
    let ac0 = &acs[0];
    assert_eq!(ac0["status"], "UNRESOLVABLE");
    assert!(
        ac0["detail"]
            .as_str()
            .unwrap()
            .contains("nonexistent_crate"),
        "detail should name the bogus crate, got: {}",
        ac0["detail"]
    );
    assert!(
        ac0["symbol"].as_str().unwrap().contains("crate:"),
        "symbol should be present, got: {:?}",
        ac0["symbol"]
    );
}

#[test]
fn verify_ac_clean_milestone_exits_ok() {
    let env = TestEnv::new();

    let create = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"Clean Test","intent":{"outcome":"test verify-ac clean"},"scope":{"in_scope":["a"],"out_of_scope":["b","c"]}}"#,
        "--format",
        "json",
    ]);
    assert!(
        create.status.success(),
        "milestone create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&create.stdout).expect("milestone create JSON");
    let milestone_id = v["milestone"]["id"].as_str().unwrap().to_string();

    // Use manual verification - always passes
    let ac_add = env.run(&[
        "milestone",
        "ac",
        "add",
        &milestone_id,
        "--description",
        "manual check",
        "--verification",
        "manual: review the output",
        "--format",
        "json",
    ]);
    assert!(
        ac_add.status.success(),
        "ac add failed: {}",
        String::from_utf8_lossy(&ac_add.stderr)
    );

    let out = env.run(&["plan", "verify-ac", &milestone_id, "--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "command exited non-zero: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        stdout
    );

    let v2: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));
    assert_eq!(v2["ok"], true, "should report ok: true, got {:?}", v2);
    assert_eq!(v2["unresolvable"], 0, "should have zero unresolvable");
}
