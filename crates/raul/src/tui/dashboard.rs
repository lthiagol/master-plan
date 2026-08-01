use serde_json::Value;

use super::app::{
    BlockerLine, DashboardSnapshot, ExecutionCounts, InboxLine, LifecycleCounts, SpecCounts,
};

fn count_from_map(map: &Value, key: &str) -> u64 {
    map.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

/// M146: keep the legacy counter accessors available so older
/// fixtures/tests compile. The live TUI Dashboard now reads
/// `LifecycleCounts` instead — the migration lives in
/// `lifecycle_counts_from_status`.
#[allow(dead_code)]
fn execution_counts_from_status(status: &Value) -> ExecutionCounts {
    let by_exec = &status["milestones"]["by_execution_status"];
    let total = status["milestones"]["total"].as_u64().unwrap_or(0);
    ExecutionCounts {
        total,
        done: count_from_map(by_exec, "done"),
        planned: count_from_map(by_exec, "planned"),
        in_progress: count_from_map(by_exec, "in-progress"),
        blocked: count_from_map(by_exec, "blocked"),
    }
}

#[allow(dead_code)]
fn spec_counts_from_status(status: &Value) -> SpecCounts {
    let by_spec = &status["milestones"]["by_spec_status"];
    SpecCounts {
        ready: count_from_map(by_spec, "ready"),
        review: count_from_map(by_spec, "review"),
        verified: count_from_map(by_spec, "verified"),
    }
}

fn blockers_from_status(status: &Value) -> Vec<BlockerLine> {
    status["blockers"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|b| BlockerLine {
                    milestone: b["milestone"].as_str().unwrap_or("?").to_string(),
                    reason: b["reason"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Build dashboard snapshot from `mp status` and `mp inbox` JSON (unit-testable).
pub fn snapshot_from_status_inbox(status: &Value, inbox: &Value) -> DashboardSnapshot {
    let inbox_items: Vec<InboxLine> = inbox["items"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|item| InboxLine {
                    id: item["id"].as_str().unwrap_or("?").to_string(),
                    kind: item["kind"].as_str().unwrap_or("?").to_string(),
                    display: item["display"].as_str().unwrap_or("").to_string(),
                    reason: item["reason"].as_str().unwrap_or("").to_string(),
                    action: item["action"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let item_count = inbox_items.len() as u64;

    let path_preview: Vec<String> = status["suggested_path"]["preview"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    DashboardSnapshot {
        planning_status: status["planning_status"]
            .as_str()
            .unwrap_or("?")
            .to_string(),
        execution_mode: status["execution"]["mode"]
            .as_str()
            .unwrap_or("?")
            .to_string(),
        inbox_count: item_count,
        pending_review_count: status["pending_review_count"].as_u64().unwrap_or(0),
        track_pending: status["track_pending"].as_u64().unwrap_or(0),
        annotations_open: status["annotations_open"].as_u64().unwrap_or(0),
        next_action: status["suggested_path"]["next_action"]["display"]
            .as_str()
            .unwrap_or("—")
            .to_string(),
        path_preview,
        // M146: the legacy spec/exec counters are still populated
        // (no readers exist for them today, but the fields stay for
        // backcompat with future-proofing). The Plan-overview block
        // surfaces `lifecycle_counts` instead so the TUI stays in
        // sync with the canonical `lifecycle` field the milestone list
        // and detail both render.
        execution_counts: execution_counts_from_status(status),
        spec_counts: spec_counts_from_status(status),
        lifecycle_counts: lifecycle_counts_from_status(status),
        blockers: blockers_from_status(status),
        inbox_items,
    }
}

/// Build `LifecycleCounts` from the `by_lifecycle` block in `mp status`.
/// Reads the canonical post-M100 bucket values
/// (draft / groomed / approved / in-progress / executed / self-reviewed /
/// reviewed / complete / remediation) populated by walking
/// `load_all_milestones` server-side.
///
/// M196: the executor's end-state bucket was renamed from `"done"` to
/// `"executed"`. The `LifecycleCounts` field is still `done` for
/// backward compat with internal callers, but the bucket key it reads
/// from `by_lifecycle` is now `"executed"`.
fn lifecycle_counts_from_status(status: &Value) -> LifecycleCounts {
    let map = &status["milestones"]["by_lifecycle"];
    LifecycleCounts {
        total: status["milestones"]["total"].as_u64().unwrap_or(0),
        draft: count_from_map(map, "draft"),
        groomed: count_from_map(map, "groomed"),
        approved: count_from_map(map, "approved"),
        in_progress: count_from_map(map, "in-progress"),
        // M196: lifecycle bucket renamed from "done" to "executed".
        done: count_from_map(map, "executed"),
        self_reviewed: count_from_map(map, "self-reviewed"),
        reviewed: count_from_map(map, "reviewed"),
        complete: count_from_map(map, "complete"),
        remediation: count_from_map(map, "remediation"),
    }
}

/// Group inbox items by kind for sectioned rendering (stable kind order).
pub fn inbox_kind_order() -> &'static [&'static str] {
    &[
        "spec-review",
        "execution-review",
        "milestone",
        "track",
        "backlog",
        "idea",
        "annotation",
        "validate",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_status() -> Value {
        // BF-15 / M171 AC-06 parity: this fixture mirrors the canonical
        // `mp status --format json.milestones` shape — `by_execution_status`,
        // `by_spec_status`, AND the post-M100 `by_lifecycle` block the
        // Dashboard reads. A future drift between the real shape and
        // the fixture will surface here rather than as a silent
        // count_from_map zero fallback in the Plan overview.
        serde_json::json!({
            "planning_status": "in-execution",
            "inbox_count": 2,
            "pending_review_count": 5,
            "track_pending": 1,
            "annotations_open": 3,
            "execution": { "mode": "autonomous" },
            "milestones": {
                "total": 10,
                "by_execution_status": {
                    "done": 7,
                    "planned": 2,
                    "in-progress": 1,
                    "blocked": 0
                },
                "by_spec_status": {
                    "ready": 2,
                    "review": 1,
                    "verified": 7,
                    "implemented": 0
                },
                "by_lifecycle": {
                    "draft": 0,
                    "groomed": 0,
                    "approved": 2,
                    "in-progress": 1,
                    "executed": 0,
                    "self-reviewed": 0,
                    "reviewed": 0,
                    "complete": 7,
                    "remediation": 0
                }
            },
            "blockers": [
                { "milestone": "42", "reason": "waiting on dependency" }
            ],
            "suggested_path": {
                "next_action": { "display": "M73/S1" },
                "preview": ["M73/S1", "M73/S2"]
            }
        })
    }

    fn fixture_inbox() -> Value {
        serde_json::json!({
            "items": [
                {
                    "kind": "track",
                    "id": "TW-03",
                    "display": "Fix backlog output",
                    "reason": "pending tweak",
                    "action": "mp track show tweak"
                },
                {
                    "kind": "spec-review",
                    "id": "88",
                    "display": "M88 — Example",
                    "reason": "spec_status review — awaiting approval",
                    "action": "mp milestone approve 88"
                }
            ]
        })
    }

    #[test]
    fn snapshot_from_fixture_json() {
        let snap = snapshot_from_status_inbox(&fixture_status(), &fixture_inbox());
        assert_eq!(snap.execution_mode, "autonomous");
        assert_eq!(snap.pending_review_count, 5);
        assert_eq!(snap.next_action, "M73/S1");
        assert_eq!(snap.path_preview.len(), 2);
        assert_eq!(snap.inbox_items[0].id, "TW-03");
        assert_eq!(snap.inbox_items[0].reason, "pending tweak");
        assert_eq!(snap.inbox_items[0].action, "mp track show tweak");
        assert_eq!(snap.execution_counts.total, 10);
        assert_eq!(snap.execution_counts.done, 7);
        assert_eq!(snap.execution_counts.planned, 2);
        assert_eq!(snap.execution_counts.in_progress, 1);
        assert_eq!(snap.spec_counts.ready, 2);
        assert_eq!(snap.spec_counts.review, 1);
        assert_eq!(snap.blockers.len(), 1);
        assert_eq!(snap.blockers[0].milestone, "42");
        // BF-15 / M171 AC-06: lifecycle counts come from the new
        // `by_lifecycle` block in the fixture, in lockstep with the
        // shape `mp status --format json.milestones.by_lifecycle`
        // actually emits.
        assert_eq!(snap.lifecycle_counts.total, 10);
        assert_eq!(snap.lifecycle_counts.approved, 2);
        assert_eq!(snap.lifecycle_counts.complete, 7);
        assert_eq!(snap.lifecycle_counts.in_progress, 1);
    }

    #[test]
    fn snapshot_empty_inbox_and_no_blockers() {
        let status = serde_json::json!({
            "planning_status": "planning",
            "inbox_count": 0,
            "pending_review_count": 0,
            "track_pending": 0,
            "annotations_open": 0,
            "execution": { "mode": "planning" },
            "milestones": {
                "total": 0,
                "by_execution_status": {},
                "by_spec_status": {},
                "by_lifecycle": {
                    "draft": 0,
                    "groomed": 0,
                    "approved": 0,
                    "in-progress": 0,
                    "executed": 0,
                    "self-reviewed": 0,
                    "reviewed": 0,
                    "complete": 0,
                    "remediation": 0
                }
            },
            "blockers": [],
            "suggested_path": {
                "next_action": { "display": "—" },
                "preview": []
            }
        });
        let inbox = serde_json::json!({ "items": [] });
        let snap = snapshot_from_status_inbox(&status, &inbox);
        assert!(snap.inbox_items.is_empty());
        assert!(snap.blockers.is_empty());
        assert_eq!(snap.execution_counts.total, 0);
        // BF-15 / M171 AC-06: lifecycle counts are zero in the empty
        // fixture too — pins the by_lifecycle read path against the
        // missing-key fallback.
        assert_eq!(snap.lifecycle_counts.total, 0);
        assert_eq!(snap.lifecycle_counts.complete, 0);
        assert_eq!(snap.lifecycle_counts.approved, 0);
    }
}
