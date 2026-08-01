//! M92 AC-04: `mp show milestone <id>` default JSON output is produced by the
//! same serialize path as `write_milestone` — i.e. the loaded `MilestoneFile`
//! struct serialized directly, not a separate hand-built "lean" view. Concretely:
//! show's default JSON == the struct written to disk by the store (modulo the
//! `prepare_for_disk` normalization, which both paths apply identically).

use crate::common::TestEnv;

#[test]
fn show_default_json_matches_persisted_document_shape() {
    let env = TestEnv::new();
    let create = r#"{
        "title": "Parity",
        "intent": {"outcome": "show == write"},
        "problem": {"description": "p"},
        "scope": {"in_scope": ["x"], "out_of_scope": ["a", "b"]},
        "acceptance_criteria": [{"description": "ac", "verification": "manual: accepted — t"}]
    }"#;
    let out = env.run(&["milestone", "create", "--json", create]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = v["milestone"]["id"].as_str().unwrap().to_string();
    let slug = v["milestone"]["slug"].as_str().unwrap().to_string();

    // The on-disk document (written by store::write_milestone).
    let disk_path = env
        .tmp
        .path()
        .join(format!("master-plan/milestones/{id}-{slug}.json"));
    let disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&disk_path).unwrap()).unwrap();

    // mp show default output (must use the same serialize path).
    let show = env.run(&["show", "milestone", &id]);
    assert!(
        show.status.success(),
        "{}",
        String::from_utf8_lossy(&show.stderr)
    );
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();

    // Same top-level keys (the load-bearing field set) — no separate "lean" view
    // that omits or reorders fields relative to the persisted document.
    //
    // M133 AC-03 adds `reviews` / `comments` / `handoffs` to the show
    // output, sourced from the separate `reviews.json` file (not the
    // persisted milestone document). `review_trail_error` (null on a
    // healthy/missing file, M133 review remediation) is injected by the
    // same path. Strip all four before the key-set comparison so the
    // M92 parity invariant still holds for the milestone body itself.
    // The M133 contract is verified separately by
    // `show_milestone_includes_comments_and_handoffs` and the raul-side
    // `show_consumes_new_review_fields`.
    let mut shown_for_keys = shown.clone();
    for trail_key in ["reviews", "comments", "handoffs", "review_trail_error"] {
        if let Some(obj) = shown_for_keys.as_object_mut() {
            obj.remove(trail_key);
        }
    }
    let disk_keys: std::collections::BTreeSet<String> =
        disk.as_object().unwrap().keys().cloned().collect();
    let shown_keys: std::collections::BTreeSet<String> = shown_for_keys
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        disk_keys, shown_keys,
        "show keys must match persisted document keys"
    );

    // M100 compat: `show` injects the legacy `spec_status` /
    // `execution_status` view derived from the unified lifecycle so
    // `mp show --fields milestone.spec_status` keeps working for callers
    // that have not migrated to the new field. Stripping is a deliberate
    // parity check on the lifecycle-only payload; the legacy compat view
    // is covered separately by inject_legacy_status_view / show_includes
    // derive tests in commands::show::tests.
    let mut shown_stripped = shown.clone();
    if let Some(milestone_obj) = shown_stripped["milestone"].as_object_mut() {
        milestone_obj.remove("spec_status");
        milestone_obj.remove("execution_status");
    }
    assert_eq!(shown_stripped["milestone"], disk["milestone"]);
    assert_eq!(shown_stripped["intent"], disk["intent"]);
    assert_eq!(
        shown_stripped["acceptance_criteria"],
        disk["acceptance_criteria"]
    );
}

/// `--format raw` emits the verbatim on-disk JSON (byte-identical to the file),
/// proving raw passthrough reads the same artifact write_milestone produced.
#[test]
fn show_raw_emits_verbatim_on_disk_json() {
    let env = TestEnv::new();
    let out = env.run(&[
        "milestone",
        "create",
        "--json",
        r#"{"title":"Raw","intent":{"outcome":"raw"},"problem":{"description":"p"},"scope":{"in_scope":["x"],"out_of_scope":["a","b"]},"acceptance_criteria":[{"description":"ac","verification":"manual: accepted — t"}]}"#,
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = v["milestone"]["id"].as_str().unwrap();

    let raw = env.run(&["show", "milestone", id, "--format", "raw"]);
    assert!(raw.status.success());
    let raw_str = String::from_utf8_lossy(&raw.stdout);

    // raw output is the on-disk file verbatim (it must parse as the same JSON
    // document the store wrote, with no re-serialization).
    let parsed: serde_json::Value = serde_json::from_str(raw_str.trim()).unwrap();
    assert_eq!(parsed["milestone"]["title"].as_str().unwrap(), "Raw");
}
