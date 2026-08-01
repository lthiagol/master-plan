//! Shared data-shaping for the path tree (TUI).
//!
//! The TUI emitter ([`crate::tui::path_view`], ratatui `Line`/`Span`)
//! consumes this module so the lane set, branch order, label rule, row
//! detail, and blocked-fork grouping are defined exactly once. AC-05
//! required the TUI Path tab to mirror the now-removed pre-M164 CLI
//! tree; single-sourcing the *shape* is what keeps the two from drifting
//! when the M157 emitters were in flight.
//!
//! M157 review remediation (F-01/F-02/F-03/F-04): this module replaces
//! the byte-identical helpers that were previously copy-pasted between
//! the CLI emitter and `tui/path_view.rs`. After M164 dropped the CLI
//! surface, only the TUI emitter remains, but the shared shape still
//! lives here so future consumers (e.g. a markdown exporter) share it.

use std::collections::HashMap;

/// Branch render order (after the execution trunk). Empty branches are
/// skipped by the emitters. `backlog` is deliberately absent — it has
/// its own surface.
pub const BRANCH_ORDER: &[&str] = &["awaiting-approval", "blocked", "grooming", "review"];

/// Milestone items for one lane, keyed by lane name.
pub struct LaneData {
    pub items: Vec<serde_json::Value>,
}

/// Build a `name → items` map from the path envelope.
///
/// Accepts both the multi-lane `{ "lanes": [...] }` shape
/// (`mp path --all`) and a single-lane object (`mp path --lane <name>`).
/// Backlog items are filtered out so the tree is milestones-only
/// regardless of which lane carried them — the CLI previously relied on
/// never selecting the backlog lane, the TUI filtered defensively; this
/// makes the rule explicit and identical for both.
pub fn lane_map(data: &serde_json::Value) -> HashMap<String, LaneData> {
    let mut map = HashMap::new();
    let lanes = data.get("lanes").and_then(|v| v.as_array());
    if let Some(arr) = lanes {
        for lane in arr {
            let name = lane["name"].as_str().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            map.insert(
                name,
                LaneData {
                    items: milestone_items(lane.get("items")),
                },
            );
        }
    } else if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
        map.insert(
            name.to_string(),
            LaneData {
                items: milestone_items(data.get("items")),
            },
        );
    }
    map
}

fn milestone_items(items: Option<&serde_json::Value>) -> Vec<serde_json::Value> {
    items
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter(|it| it["type"].as_str() != Some("backlog-item"))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Display title for a lane header.
pub fn display_name(lane: &str) -> &'static str {
    match lane {
        "awaiting-approval" => "Awaiting approval",
        "blocked" => "Blocked",
        "grooming" => "Grooming",
        "review" => "Review",
        "execution" => "Execution",
        _ => "Other",
    }
}

/// Single-sourced milestone row label. The path envelope carries the
/// *raw* id (`"110"`, not `"M110"`); this helper applies the M-prefix
/// for milestone items and leaves backlog ids verbatim (M125 ER-3).
pub fn item_label(item: &serde_json::Value) -> String {
    let id = item["milestone"]["id"].as_str().unwrap_or("?");
    let title = item["milestone"]["title"].as_str().unwrap_or("?");
    let item_type = item["type"].as_str().unwrap_or("");
    if item_type == "backlog-item" {
        format!("{id} — {title}")
    } else {
        format!("M{id} — {title}")
    }
}

/// First `depends_on` entry of a milestone, if any — the **primary**
/// blocker used for fork grouping.
///
/// Product choice (M157): multi-dependency milestones group under their
/// first listed dependency only. Full multi-blocker topology (one item
/// under several forks) is out of scope.
///
/// Path wire (`mp path`) projects **unmet-only** deps, so this is the
/// first remaining blocker in practice; met deps are omitted upstream.
pub fn first_dep(item: &serde_json::Value) -> Option<String> {
    item["milestone"]["depends_on"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// `"deps: Mx, My"` or empty.
pub fn deps_str(item: &serde_json::Value) -> String {
    let deps: Vec<&str> = item["milestone"]["depends_on"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if deps.is_empty() {
        String::new()
    } else {
        format!(
            "deps: {}",
            deps.iter()
                .map(|d| format!("M{d}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Trunk-row detail — deliberately minimal: lifecycle + deps. The trunk
/// is the "ready to run" population; detail belongs on side branches.
pub fn trunk_detail(item: &serde_json::Value) -> String {
    let lc = item["milestone"]["lifecycle"].as_str().unwrap_or("");
    join_detail(&[lc.to_string(), deps_str(item)])
}

/// Side-branch row detail: lifecycle · review phase · open-findings ·
/// deps · non-normal priority. Used for both flat and blocked branch
/// items in both renderers so the two cannot diverge. (Pre-remediation
/// the CLI blocked branch showed priority-only and the TUI flat branch
/// omitted priority — both fixed by routing through here.)
pub fn branch_detail(item: &serde_json::Value) -> String {
    let m = &item["milestone"];
    let mut parts: Vec<String> = Vec::new();
    if let Some(lc) = m["lifecycle"].as_str().filter(|s| !s.is_empty()) {
        parts.push(lc.to_string());
    }
    if let Some(phase) = m["review_phase"].as_str().filter(|s| !s.is_empty()) {
        parts.push(phase.to_string());
    }
    let open = m["open_self_findings"].as_u64().unwrap_or(0)
        + m["open_external_findings"].as_u64().unwrap_or(0);
    if open > 0 {
        parts.push(format!("⚠{open} open"));
    }
    let deps = deps_str(item);
    if !deps.is_empty() {
        parts.push(deps);
    }
    if let Some(p) = m["priority"]
        .as_str()
        .filter(|p| !p.is_empty() && *p != "normal")
    {
        parts.push(format!("priority={p}"));
    }
    join_detail(&parts)
}

/// Join non-empty parts with " · ".
pub fn join_detail(parts: &[String]) -> String {
    parts
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Ordered blocked-fork groups: `(blocker_id, items)`.
///
/// `blocker_id == None` is the trailing "no dependency" group. Order is
/// first-seen by blocker so the tree is stable across renders and
/// identical between emitters. Pre-remediation both renderers tracked
/// order in a parallel `Vec` while pointlessly sorting a `BTreeMap`;
/// the grouping now lives once, here.
pub fn blocked_groups<'a>(
    items: &'a [serde_json::Value],
) -> Vec<(Option<String>, Vec<&'a serde_json::Value>)> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
    let mut no_dep: Vec<&serde_json::Value> = Vec::new();
    for item in items {
        match first_dep(item) {
            Some(b) => {
                if !order.contains(&b) {
                    order.push(b.clone());
                }
                groups.entry(b).or_default().push(item);
            }
            None => no_dep.push(item),
        }
    }
    let mut out: Vec<(Option<String>, Vec<&'a serde_json::Value>)> = order
        .into_iter()
        .map(|k| {
            let group = groups.remove(&k).unwrap_or_default();
            (Some(k), group)
        })
        .collect();
    if !no_dep.is_empty() {
        out.push((None, no_dep));
    }
    out
}

/// Header label for a blocked-fork group.
pub fn blocked_group_label(blocker: Option<&str>) -> String {
    match blocker {
        Some(k) => format!("blocked-by M{k}"),
        None => "blocked (no dependency)".to_string(),
    }
}
