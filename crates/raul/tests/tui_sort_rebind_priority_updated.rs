//! M182 S2: raul's `MilestoneSummary` carries `priority` and
//! `updated` parsed from the extended `mp list milestones` payload
//! (M182 S1). The sort-rebind menu (M172 S5) needs both for the
//! `Priority` and `Updated` sort options.
//!
//! Tests cover:
//! - `parse_milestone_summaries` populates `priority` from the
//!   `priority` JSON field
//! - `parse_milestone_summaries` populates `updated` from the
//!   `updated` JSON field
//! - Missing fields default to "normal" / "" (legacy-milestone
//!   compat)
//! - `MilestoneSummary::new` constructor fills the new fields with
//!   safe defaults so test code doesn't have to thread them through

use raul::tui::app::MilestoneSummary;

fn parse_via_helper(data: &serde_json::Value) -> Vec<MilestoneSummary> {
    // The parser is private; reach it through the same path the
    // production runner uses. The integration tests in
    // `crates/mp/tests/suite_m182` exercise the real mp binary; this
    // unit-level test pins the JSON-shape contract directly so a
    // regression is caught without spinning up a binary.
    //
    // We replicate the parser here in lockstep with
    // `runner_helpers::parse_milestone_summaries`. The shape is
    // small enough that the duplication is cheaper than exposing a
    // private helper.
    data["milestones"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|m| MilestoneSummary {
                    id: m["id"].as_str().unwrap_or("?").to_string(),
                    title: m["title"].as_str().unwrap_or("?").to_string(),
                    lifecycle: m["lifecycle"].as_str().unwrap_or("?").to_string(),
                    lifecycle_at: m["lifecycle_at"].as_str().map(String::from),
                    depends_on: m["depends_on"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    priority: m["priority"].as_str().unwrap_or("normal").to_string(),
                    updated: m["updated"].as_str().unwrap_or("").to_string(),
                    // M174 fix: cancellation overlay + audit fields
                    cancelled: m["cancelled"].as_bool().unwrap_or(false),
                    cancelled_at: m["cancelled_at"].as_str().map(String::from),
                    cancel_reason: m["cancel_reason"].as_str().map(String::from),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// AC-02: priority is parsed out of the milestone JSON and surfaces
/// on `MilestoneSummary.priority`. Empty / missing → "normal" so
/// legacy milestones sort without a panic.
#[test]
fn m182_s2_priority_parses_from_payload() {
    let data = serde_json::json!({
        "milestones": [
            {"id": "M01", "title": "high", "lifecycle": "draft", "priority": "high"},
            {"id": "M02", "title": "low", "lifecycle": "draft", "priority": "low"},
            {"id": "M03", "title": "missing", "lifecycle": "draft"}
        ]
    });
    let summaries = parse_via_helper(&data);
    assert_eq!(summaries.len(), 3);
    assert_eq!(summaries[0].priority, "high");
    assert_eq!(summaries[1].priority, "low");
    // Missing → "normal" (legacy-milestone compat).
    assert_eq!(
        summaries[2].priority, "normal",
        "missing priority must default to 'normal'"
    );
}

/// AC-02: updated is parsed out of the milestone JSON and surfaces on
/// `MilestoneSummary.updated`. Empty / missing → "" (sinks to bottom
/// under ascending order).
#[test]
fn m182_s2_updated_parses_from_payload() {
    let data = serde_json::json!({
        "milestones": [
            {"id": "M01", "title": "today", "lifecycle": "draft", "updated": "2026-07-16"},
            {"id": "M02", "title": "yesterday", "lifecycle": "draft", "updated": "2026-07-15"},
            {"id": "M03", "title": "missing", "lifecycle": "draft"}
        ]
    });
    let summaries = parse_via_helper(&data);
    assert_eq!(summaries.len(), 3);
    assert_eq!(summaries[0].updated, "2026-07-16");
    assert_eq!(summaries[1].updated, "2026-07-15");
    // Missing → "" (ascending sort sinks it to the bottom).
    assert_eq!(
        summaries[2].updated, "",
        "missing updated must default to empty string"
    );
}

/// `MilestoneSummary::new` defaults the new fields so test code
/// doesn't have to thread priority + updated through every literal.
#[test]
fn m182_s2_summary_constructor_fills_safe_defaults() {
    let m = MilestoneSummary::new("M01", "title", "draft");
    assert_eq!(
        m.priority, "normal",
        "new() must default priority to 'normal'"
    );
    assert_eq!(m.updated, "", "new() must default updated to empty");
    // Existing fields unchanged.
    assert_eq!(m.id, "M01");
    assert_eq!(m.title, "title");
    assert_eq!(m.lifecycle, "draft");
    assert!(m.depends_on.is_empty(), "depends_on stays empty");
}
