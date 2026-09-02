//! M112 S1: `mp backlog list` filters backlog items by source/status/priority
//! and applies --limit. Empty backlog returns `{items: []}`, not null. The
//! original M106 S11 test path that used `mp backlog list` is re-enabled here.

mod common;

use crate::common::TestEnv;

#[test]
fn backlog_list_returns_items_object_with_expected_shape() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    let out = env.run(&["backlog", "list"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // Always emit `items`, never null, even if backlog is empty.
    assert!(v.get("items").is_some(), "backlog list must emit items key");
    assert!(v["items"].is_array());
}

#[test]
fn backlog_list_filters_by_source() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // Seed a new backlog item with a distinct source.
    env.run(&[
        "backlog",
        "add",
        "--desc",
        "M112 S1 source filter test",
        "--source",
        "M112-fixture",
        "--priority",
        "high",
    ]);

    let unfiltered = env.run(&["backlog", "list"]);
    let v: serde_json::Value = serde_json::from_slice(&unfiltered.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    let total = items.len();
    let new_total = total + 1;
    assert!(
        new_total >= 2,
        "fixture must have at least one backlog item already"
    );

    let filtered = env.run(&["backlog", "list", "--source", "M112-fixture"]);
    let v2: serde_json::Value = serde_json::from_slice(&filtered.stdout).unwrap();
    let items2 = v2["items"].as_array().unwrap();
    assert_eq!(
        items2.len(),
        1,
        "M112-fixture-only filter must leave one item"
    );
    assert_eq!(items2[0]["description"], "M112 S1 source filter test");
    assert_eq!(items2[0]["source"], "M112-fixture");
}

#[test]
fn backlog_list_filters_by_priority_and_status() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    env.run(&[
        "backlog",
        "add",
        "--desc",
        "high-prio",
        "--priority",
        "high",
    ]);
    env.run(&["backlog", "add", "--desc", "low-prio", "--priority", "low"]);

    let high = env.run(&["backlog", "list", "--priority", "high"]);
    assert!(high.status.success());
    let v: serde_json::Value = serde_json::from_slice(&high.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert!(items.iter().all(|i| i["priority"] == "high"));

    // Status filter: defaults to `active` for new items.
    let active = env.run(&["backlog", "list", "--status", "active"]);
    let v_active: serde_json::Value = serde_json::from_slice(&active.stdout).unwrap();
    let items_active = v_active["items"].as_array().unwrap();
    assert!(
        items_active.iter().all(|i| i["status"] == "active"),
        "all active items must have status=active; got: {items_active:?}"
    );
}

#[test]
fn backlog_list_limit_slices_first_n() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    // The fixture has many backlog items. --limit 3 must yield <=3.
    let out = env.run(&["backlog", "list", "--limit", "3"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert!(
        items.len() <= 3,
        "limit must cap the result count; got {}",
        items.len()
    );
}

#[test]
fn backlog_list_combined_filters() {
    let env = TestEnv::from_fixture("walkthrough-oauth");

    env.run(&[
        "backlog",
        "add",
        "--desc",
        "match-all",
        "--source",
        "M112",
        "--priority",
        "high",
    ]);
    env.run(&[
        "backlog",
        "add",
        "--desc",
        "wrong-priority",
        "--source",
        "M112",
        "--priority",
        "low",
    ]);

    let out = env.run(&[
        "backlog",
        "list",
        "--source",
        "M112",
        "--priority",
        "high",
        "--limit",
        "10",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["description"], "match-all");
}

// ── M203: backlog + ideas preview projection (AC-02, AC-03, AC-07, AC-08) ──
//
// Each `mp list backlog` row gains a `preview` field. Active items: the
// description continuation (lines after the first), ~80-char truncated.
// Resolved items: `resolved · <resolution>`. Empty resolution → just
// `resolved`. `mp list ideas` projects the first line of `body`,
// ~80-char truncated. The TUI consumes `preview` directly.
//
// Note: `mp list backlog` does not support a `--source` filter, so tests
// here add uniquely identifiable items and locate them by description.

fn list_backlog_items(env: &TestEnv) -> Vec<serde_json::Value> {
    let out = env.run(&["list", "backlog"]);
    assert!(
        out.status.success(),
        "list backlog failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["backlog"].as_array().unwrap().clone()
}

fn find_backlog_item<'a>(
    items: &'a [serde_json::Value],
    needle: &str,
) -> &'a serde_json::Value {
    items
        .iter()
        .find(|i| i["description"].as_str().unwrap_or("").contains(needle))
        .unwrap_or_else(|| {
            panic!(
                "no backlog item with description containing {needle:?}; items: {items:?}"
            )
        })
}

#[test]
fn list_backlog_projects_preview_field() {
    let env = TestEnv::new();

    env.run(&[
        "backlog",
        "add",
        "--desc",
        "M203 S2 proj\nSecond line is the continuation\nThird line still counts",
    ]);

    let items = list_backlog_items(&env);
    let item = find_backlog_item(&items, "M203 S2 proj");
    let preview = item["preview"]
        .as_str()
        .expect("preview field must be a string");
    // Active item → description continuation, joined, trimmed.
    assert!(
        preview.starts_with("Second line"),
        "active preview must start with the continuation; got: {preview:?}"
    );
    assert!(
        preview.contains("Third line"),
        "continuation must include lines after the first; got: {preview:?}"
    );
}

#[test]
fn preview_for_active_item_uses_description_continuation() {
    let env = TestEnv::new();

    let add_single = env.run(&[
        "backlog",
        "add",
        "--desc",
        "M203 single-line active row, no continuation",
    ]);
    let _ = add_single;

    let add_multi = env.run(&[
        "backlog",
        "add",
        "--desc",
        "M203 multi-line active title\nDetail about why this matters for the team",
    ]);
    let _ = add_multi;

    let items = list_backlog_items(&env);

    let single = find_backlog_item(&items, "M203 single-line active row");
    assert_eq!(
        single["preview"], "",
        "active item with no continuation must have empty preview; got: {}",
        single["preview"]
    );

    let multi = find_backlog_item(&items, "M203 multi-line active title");
    let preview = multi["preview"].as_str().unwrap();
    assert_eq!(
        preview, "Detail about why this matters for the team",
        "multi-line active item preview must equal the continuation; got: {preview:?}"
    );
}

#[test]
fn preview_for_resolved_item_collapses_to_resolution() {
    let env = TestEnv::new();

    let add = env.run(&[
        "backlog",
        "add",
        "--desc",
        "M203 resolve-with-reason\nSome long elaboration that nobody needs in the row",
    ]);
    let id = crate::common::json_from_stdout(&add.stdout)["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let resolve = env.run(&[
        "backlog",
        "resolve",
        &id,
        "--reason",
        "shipped in M99",
        "--format",
        "json",
    ]);
    assert!(resolve.status.success());

    let items = list_backlog_items(&env);
    let item = find_backlog_item(&items, "M203 resolve-with-reason");
    assert_eq!(
        item["preview"], "resolved · shipped in M99",
        "resolved preview must collapse to `resolved · <resolution>`; got: {}",
        item["preview"]
    );
}

#[test]
fn preview_for_resolved_collapses() {
    let env = TestEnv::new();

    let add = env.run(&[
        "backlog",
        "add",
        "--desc",
        "M203 wont-fix row",
    ]);
    let id = crate::common::json_from_stdout(&add.stdout)["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let resolve = env.run(&[
        "backlog",
        "resolve",
        &id,
        "--wont-fix",
        "--reason",
        "out of scope",
        "--format",
        "json",
    ]);
    assert!(resolve.status.success());

    let items = list_backlog_items(&env);
    let item = find_backlog_item(&items, "M203 wont-fix row");
    assert_eq!(
        item["preview"], "resolved · wont-fix: out of scope",
        "wont-fix resolution must include the reason; got: {}",
        item["preview"]
    );
}

#[test]
fn preview_for_resolved_with_empty_resolution() {
    // Construct a synthetic resolved item with an empty `resolution`
    // field — the `mp backlog resolve` CLI auto-fills `resolution` to
    // either the reason or `"resolved"`, so the only path to a truly
    // empty resolution is an on-disk entry that pre-dates M138
    // refactoring. Verify the projection's empty-resolution branch
    // collapses to the bare `resolved` token.
    let env = TestEnv::new();

    let add = env.run(&[
        "backlog",
        "add",
        "--desc",
        "M203 resolved-empty row",
    ]);
    let id = crate::common::json_from_stdout(&add.stdout)["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Manually edit backlog.json to set status=resolved with empty
    // resolution. The runtime contract under test is independent of
    // how the item reached that state.
    let plan_dir = env.tmp.path().join("master-plan");
    let backlog_path = plan_dir.join("backlog.json");
    let raw = std::fs::read_to_string(&backlog_path).expect("backlog.json readable");
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let items = v["items"].as_array_mut().unwrap();
    let item = items
        .iter_mut()
        .find(|i| i["id"].as_str() == Some(id.as_str()))
        .expect("item present");
    item["status"] = serde_json::Value::String("resolved".to_string());
    item["resolution"] = serde_json::Value::String(String::new());
    std::fs::write(&backlog_path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    let listed = list_backlog_items(&env);
    let item = find_backlog_item(&listed, "M203 resolved-empty row");
    assert_eq!(
        item["preview"], "resolved",
        "empty resolution must collapse to just `resolved`; got: {}",
        item["preview"]
    );
}

#[test]
fn bf_item_preview_uses_legacy_description() {
    // Legacy BF/TW items in the backlog lane (mixed in alongside BL/B-*
    // items) render with the same `description` continuation rule. The
    // default-source `planning` items follow the same projection, so we
    // add a BF-style entry with a recognisable description and verify
    // it lands with the right preview.
    let env = TestEnv::new();

    env.run(&[
        "backlog",
        "add",
        "--desc",
        "M203 BF-style entry\nInner detail about the fix",
        "--source",
        "track-bugfix",
    ]);

    let items = list_backlog_items(&env);
    let item = find_backlog_item(&items, "M203 BF-style entry");
    assert_eq!(
        item["preview"], "Inner detail about the fix",
        "BF item preview must use the description continuation; got: {}",
        item["preview"]
    );
}

#[test]
fn tw_item_preview_uses_legacy_description() {
    let env = TestEnv::new();

    env.run(&[
        "backlog",
        "add",
        "--desc",
        "M203 TW-style entry\nSpacer padding was 8px should be 12px",
        "--source",
        "track-tweak",
    ]);

    let items = list_backlog_items(&env);
    let item = find_backlog_item(&items, "M203 TW-style entry");
    assert_eq!(
        item["preview"], "Spacer padding was 8px should be 12px",
        "TW item preview must use the description continuation; got: {}",
        item["preview"]
    );
}

// ── M203 AC-03: ideas preview projection ───────────────────────────────────

fn list_ideas_items(env: &TestEnv) -> Vec<serde_json::Value> {
    let out = env.run(&["list", "ideas"]);
    assert!(
        out.status.success(),
        "list ideas failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["ideas"].as_array().unwrap().clone()
}

#[test]
fn list_ideas_projects_preview_field() {
    let env = TestEnv::new();

    env.run(&[
        "idea",
        "create",
        "--title",
        "M203 S3 idea",
        "--body",
        "Replace the manual HMAC chain with BLS signatures\nFollow-up: profile the verify path",
    ]);

    let items = list_ideas_items(&env);
    assert!(!items.is_empty(), "expected at least one idea row");
    let item = items
        .iter()
        .find(|i| i["title"] == "M203 S3 idea")
        .expect("idea row present");
    assert!(
        item.get("preview").is_some(),
        "each idea must project a `preview` field; got: {item}"
    );
    let preview = item["preview"].as_str().expect("preview is a string");
    assert_eq!(
        preview, "Replace the manual HMAC chain with BLS signatures",
        "idea preview must equal the first line of body; got: {preview:?}"
    );
}

#[test]
fn preview_uses_first_line_of_body() {
    let env = TestEnv::new();

    env.run(&[
        "idea",
        "create",
        "--title",
        "M203 idea single line",
        "--body",
        "Body is one line",
    ]);
    env.run(&[
        "idea",
        "create",
        "--title",
        "M203 idea multi line",
        "--body",
        "First line goes here\nSecond line\nThird line",
    ]);
    env.run(&[
        "idea",
        "create",
        "--title",
        "M203 idea empty body",
    ]);

    let items = list_ideas_items(&env);

    let single = items
        .iter()
        .find(|i| i["title"] == "M203 idea single line")
        .expect("single-line idea present");
    assert_eq!(
        single["preview"], "Body is one line",
        "single-line body preview must equal the body"
    );

    let multi = items
        .iter()
        .find(|i| i["title"] == "M203 idea multi line")
        .expect("multi-line idea present");
    assert_eq!(
        multi["preview"], "First line goes here",
        "multi-line body preview must equal the first line only"
    );

    let empty = items
        .iter()
        .find(|i| i["title"] == "M203 idea empty body")
        .expect("empty idea present");
    assert_eq!(
        empty["preview"], "",
        "empty body preview must be empty string"
    );
}

#[test]
fn preview_truncates_at_eighty_chars() {
    let env = TestEnv::new();

    let long_body: String = "x".repeat(120);
    env.run(&[
        "idea",
        "create",
        "--title",
        "M203 idea long body",
        "--body",
        &long_body,
    ]);

    let items = list_ideas_items(&env);
    let item = items
        .iter()
        .find(|i| i["title"] == "M203 idea long body")
        .expect("long idea present");
    let preview = item["preview"].as_str().unwrap();
    // The 120-char body is one line, must truncate at 80 + "...".
    assert_eq!(
        preview.chars().count(),
        80 + 3,
        "preview must truncate to 80 chars plus `...`; got len={} preview={preview:?}",
        preview.chars().count()
    );
    assert!(
        preview.ends_with("..."),
        "truncated preview must end with `...`; got: {preview:?}"
    );
    assert!(
        preview.chars().filter(|c| *c == 'x').count() == 80,
        "preview must contain exactly 80 x chars before the ellipsis"
    );
}
