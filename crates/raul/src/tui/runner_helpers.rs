//! Data-loading and subprocess side-effect helpers shared by the action reducer,
//! mode handlers, and integration tests.

use anyhow::Result;

use crate::mp_runner::MpRunner;
use crate::reads;

use super::app::{
    AnnotationInfo, App, BacklogLine, InboxLine, Lane, MilestoneSummary, PreflightGate,
};

/// Outcome of a review menu action.
///
/// Unlike `Result<()>`, carries the raw mp output for the "Approve milestone"
/// case so callers can detect M121 gate errors and format a focused
/// flash_message.
#[must_use]
pub enum ReviewActionOutcome {
    /// Action succeeded; caller should refresh the milestone detail.
    Ok,
    /// Approve was called but `mp milestone approve` returned an M121 gate
    /// error. Contains the number of failing ACs, the milestone id, and the
    /// full untruncated combined output (stdout ∪ stderr) so the caller can
    /// stash it as `app.last_action_error` and surface it via `?` for
    /// diagnostic context.
    M121GateError {
        ac_count: usize,
        ms_id: String,
        full: String,
    },
    /// Action failed for any other reason; message is user-facing.
    OtherError(String),
}

// ============================================================================
// Data loading (per-lane fetches + cargo manifest cache)
// ============================================================================

pub fn load_dashboard(runner: &MpRunner, app: &mut App) -> Result<()> {
    // M181 S2: the Overview lane now reads one consolidated snapshot
    // (`mp overview`) instead of fanning out `mp status` + `mp inbox`.
    // The cache stores the raw payload under `overview` so a hit
    // short-circuits the subprocess on subsequent lane visits.
    if let Some(cached) = app.lane_cache.get(&Lane::Overview) {
        let raw = &cached["overview"];
        if raw.is_object() {
            let typed = crate::overview_snapshot::parse(raw);
            app.load_overview_snapshot(typed);
            return Ok(());
        }
    }

    let raw = reads::overview(runner)?;
    let typed = crate::overview_snapshot::parse(&raw);
    let cache_value = serde_json::json!({ "overview": raw });
    app.lane_cache.put(Lane::Overview, cache_value);
    app.load_overview_snapshot(typed);
    Ok(())
}

pub fn load_backlog(runner: &MpRunner, app: &mut App) -> Result<()> {
    if let Some(cached) = app.lane_cache.get(&Lane::Backlog) {
        let data = &cached["data"];
        if data.is_object() {
            let backlog = parse_backlog_lines(data);
            app.load_backlog(backlog);
            return Ok(());
        }
    }

    let data = reads::list_backlog(runner)?;
    let backlog = parse_backlog_lines(&data);
    let cache_value = serde_json::json!({ "data": data });
    app.lane_cache.put(Lane::Backlog, cache_value);
    app.load_backlog(backlog);
    Ok(())
}

fn parse_backlog_lines(data: &serde_json::Value) -> Vec<BacklogLine> {
    data["backlog"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|item| BacklogLine {
                    id: item["id"].as_str().unwrap_or("?").to_string(),
                    title: reads::backlog_summary(item),
                    priority: item["priority"].as_str().unwrap_or("?").to_string(),
                    status: item["status"].as_str().unwrap_or("?").to_string(),
                    resolution: item["resolution"].as_str().unwrap_or("").to_string(),
                    // M203 S4: project the second-line preview from the
                    // payload. `mp list backlog` always emits the key
                    // post-M203; the parser tolerates older payloads
                    // (no `preview`) by defaulting to "".
                    preview: item["preview"].as_str().unwrap_or("").to_string(),
                    // M205: parse the timestamp fields straight from the
                    // model payload (BacklogItem + IdeaEntry both
                    // expose `created` and `resolved_at` /
                    // `tags` — the JSON projection already includes
                    // them via `serde_json::to_value`). Empty strings
                    // for items predating the fields; the sort logic
                    // treats "" as "unknown" and sinks those rows to
                    // the bottom of Created / ResolvedAt under both
                    // directions.
                    created_at: item["created"].as_str().unwrap_or("").to_string(),
                    resolved_at: item["resolved_at"].as_str().unwrap_or("").to_string(),
                    tags: item["tags"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn load_backlog_detail(runner: &MpRunner, app: &mut App, id: &str) -> Result<()> {
    let data = reads::list_backlog(runner)?;
    // M184 F-02: `mp list backlog` emits `{"backlog":[...]}` (same key
    // `parse_backlog_lines` uses). Older drafts used `items`; accept
    // either so Enter on Backlog/Ideas always populates detail.
    let items = data["backlog"]
        .as_array()
        .or_else(|| data["items"].as_array());
    if let Some(items) = items {
        for item in items {
            if item["id"].as_str() == Some(id) {
                app.backlog_detail = Some(item.clone());
                return Ok(());
            }
        }
    }
    Ok(())
}

pub fn load_milestones(runner: &MpRunner, app: &mut App) -> Result<()> {
    // M143: cache hit short-circuits the mp call. We store the raw mp
    // payload (NOT the filtered Vec) so toggling `hide_done` between two
    // lane visits re-applies the filter against the cached raw data
    // rather than returning a stale view.
    if let Some(cached) = app.lane_cache.get(&Lane::Milestones) {
        let data = &cached["data"];
        if data.is_object() {
            let milestones = parse_milestone_summaries(data);
            app.load_milestones(milestones);
            return Ok(());
        }
    }

    let data = reads::list_milestones(runner, Some("-id"))?;
    let milestones = parse_milestone_summaries(&data);
    let cache_value = serde_json::json!({ "data": data });
    app.lane_cache.put(Lane::Milestones, cache_value);
    app.load_milestones(milestones);
    Ok(())
}

/// M179 S3 / AC-02: load the Watch picker. Sources `mp list
/// milestones` (the same shell-out the Milestones lane uses, but
/// unfiltered by `hide_done` — the Watch picker must surface
/// every drivable milestone regardless of the user's Overview
/// hide-done preference) and replaces the picker's candidate
/// list. The picker's selection is preserved where it still
/// resolves in the new candidate set; ids that no longer
/// exist are dropped.
pub fn load_watch_picker(runner: &MpRunner, app: &mut App) -> Result<()> {
    // M143: cache hit short-circuits the mp call. Cache key is
    // `Lane::Watch`; the picker source is `mp list milestones`
    // (no sort — the picker renders in canonical M122 order).
    if let Some(cached) = app.lane_cache.get(&Lane::Watch) {
        let data = &cached["data"];
        if data.is_object() {
            app.watch.refresh_candidates(data);
            let _ = crate::tui::watch::restore_latest_status(runner, app)?;
            return Ok(());
        }
    }
    let data = reads::list_milestones(runner, None)?;
    let cache_value = serde_json::json!({ "data": data });
    app.lane_cache.put(Lane::Watch, cache_value);
    app.watch.refresh_candidates(&data);
    let _ = crate::tui::watch::restore_latest_status(runner, app)?;
    Ok(())
}

fn parse_milestone_summaries(data: &serde_json::Value) -> Vec<MilestoneSummary> {
    data["milestones"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|m| MilestoneSummary {
                    id: m["id"].as_str().unwrap_or("?").to_string(),
                    title: m["title"].as_str().unwrap_or("?").to_string(),
                    lifecycle: m["lifecycle"].as_str().unwrap_or("?").to_string(),
                    lifecycle_at: m["lifecycle_at"].as_str().map(String::from),
                    // M172 S2: parse the depends_on array out of the
                    // milestone JSON. The list-milestones payload
                    // already exposes this field (the same shape
                    // `mp show milestone` returns), so we don't need
                    // an extra round-trip per summary. Empty for
                    // milestones with no edges.
                    depends_on: m["depends_on"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    // M182 S2: parse priority + updated. The
                    // list-milestones projection (M182 S1) carries
                    // both; raul reads them here so the sort-rebind
                    // menu's priority/updated options can order rows
                    // without a per-milestone `show` round-trip.
                    // `priority` defaults to "normal" for milestones
                    // created pre-priority (the field was optional
                    // until M122); `updated` defaults to "" so
                    // ascending order sinks older milestones to the
                    // bottom.
                    priority: m["priority"].as_str().unwrap_or("normal").to_string(),
                    updated: m["updated"].as_str().unwrap_or("").to_string(),
                    // M205: parse the on-disk creation date from the
                    // list-milestones projection (added in the same
                    // step — the projection now emits `created`).
                    // Empty for milestones predating the field; the
                    // Created sort sinks those rows to the bottom
                    // under both ascending and descending
                    // directions.
                    created: m["created"].as_str().unwrap_or("").to_string(),
                    // M174 fix: cancellation overlay + audit
                    // fields. `cancelled` defaults to false for
                    // pre-M174 milestones; the date / reason
                    // strings are `None` for those and for
                    // cancellations done before the audit fields
                    // landed. The Milestones lane reads these to
                    // render the `[cancelled YYYY-MM-DD: <reason>]`
                    // badge for milestones like M174 where the
                    // work shipped via a different design.
                    cancelled: m["cancelled"].as_bool().unwrap_or(false),
                    cancelled_at: m["cancelled_at"].as_str().map(String::from),
                    cancel_reason: m["cancel_reason"].as_str().map(String::from),
                    // M202 S14: parse the 12-stage mp-flow timeline
                    // out of the projection. We keep only the
                    // status string (not the `at` timestamp) — the
                    // lane view only needs status to render the
                    // Stage cell. Pre-M202 milestones emit `{}`
                    // here (the list projection always emits the
                    // key, even when the underlying map is empty).
                    flow_stages: m["flow_stages"]
                        .as_object()
                        .map(|obj| {
                            obj.iter()
                                .filter_map(|(slug, stage)| {
                                    stage
                                        .get("status")
                                        .and_then(|s| s.as_str())
                                        .map(|s| (slug.clone(), s.to_string()))
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn load_milestone_detail(runner: &MpRunner, app: &mut App, id: &str) -> Result<()> {
    let detail = reads::show_milestone(runner, id)?;
    app.load_milestone_detail(detail);
    Ok(())
}

pub fn load_path_data(runner: &MpRunner, app: &mut App) -> Result<()> {
    // External-review F-08: Path participates in the per-lane TTL cache
    // (same shape as Backlog).
    if let Some(cached) = app.lane_cache.get(&Lane::Path) {
        let data = &cached["data"];
        if data.is_object() || data.is_array() {
            app.load_path_data(data.clone());
            return Ok(());
        }
    }

    let mut data = reads::path_lanes(runner)?;
    // CLI parity: inject status rollup so the Path tree can show the
    // collapsed complete-milestone footer. Status failure must not
    // block the tree — footer is simply omitted.
    if let Ok(status) = reads::status(runner) {
        if let Some(obj) = data.as_object_mut() {
            obj.insert("status".to_string(), status);
        }
    }
    let cache_value = serde_json::json!({ "data": data });
    app.lane_cache.put(Lane::Path, cache_value);
    app.load_path_data(data);
    Ok(())
}

pub fn load_annotations(runner: &MpRunner, app: &mut App, target: &str) -> Result<()> {
    // Always retain the complete thread. `App::visible_annotations` is the
    // single filter projection used by render, cursor, actions, and mouse.
    let data = reads::list_annotations(runner, false, Some(target))?;
    let annotations: Vec<AnnotationInfo> = data["annotations"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|a| AnnotationInfo {
                    id: a["id"].as_str().unwrap_or("?").to_string(),
                    target: a["target"].as_str().unwrap_or("?").to_string(),
                    kind: a["kind"].as_str().unwrap_or("?").to_string(),
                    status: a["status"].as_str().unwrap_or("?").to_string(),
                    author: a["author"].as_str().unwrap_or("").to_string(),
                    body: a["body"].as_str().unwrap_or("").to_string(),
                    created_at: a["created_at"].as_str().unwrap_or("").to_string(),
                    resolved_at: a["resolved_at"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    app.load_annotations(annotations);
    Ok(())
}

// ============================================================================
// mp shell-outs (single place that touches subprocess for write paths)
// ============================================================================

pub fn create_annotation(
    runner: &MpRunner,
    app: &mut App,
    target: &str,
    kind: &str,
    body: &str,
) -> Result<()> {
    let payload = serde_json::json!({
        "target": target,
        "kind": kind,
        "body": body,
        "author": "raul-tui",
    });
    runner.run_stdin("annotation", &["create", "--json", "@-"], &payload)?;
    // External-review F-09: annotation mutations clear Overview + Milestones.
    app.lane_cache.invalidate(&Lane::Overview);
    app.lane_cache.invalidate(&Lane::Milestones);
    Ok(())
}

/// M172 S6: shell out to `mp milestone update <ms_id> --json @-`
/// with the merged `{"depends_on": [...]}` array. The source
/// milestone (the one that will gain a new depends_on edge) is
/// `ms_id`; `dep_id` is what the user typed into the input overlay.
///
/// **M172 external review (F-06):** pre-fix, the payload was a
/// one-element `["dep_id"]` array, but `mp milestone update`
/// REPLACES the existing `depends_on` array rather than appending —
/// so the submit silently deleted every prior dependency edge on
/// the milestone. The fix is to read the current `depends_on` from
/// `mp show milestone <ms_id>`, append the new edge (deduped), and
/// send the merged array. Cycle detection is the underlying tool's
/// responsibility (the merged payload is the same shape that
/// `mp` would write by hand).
pub fn set_dependency(runner: &MpRunner, app: &mut App, ms_id: &str, dep_id: &str) -> Result<()> {
    // Read the existing depends_on so we don't clobber it.
    let detail = reads::show_milestone(runner, ms_id)?;
    let existing: Vec<String> = detail["milestone"]["depends_on"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let mut merged: Vec<String> = existing;
    if !merged.iter().any(|d| d == dep_id) {
        merged.push(dep_id.to_string());
    }
    let payload = serde_json::json!({
        "depends_on": merged,
    });
    runner.run_stdin("milestone", &["update", ms_id, "--json", "@-"], &payload)?;
    app.lane_cache.invalidate(&Lane::Overview);
    app.lane_cache.invalidate(&Lane::Milestones);
    Ok(())
}

pub fn resolve_annotation(runner: &MpRunner, app: &mut App, id: &str) -> Result<()> {
    runner.run_raw("annotation", &["resolve", id])?;
    app.lane_cache.invalidate(&Lane::Overview);
    app.lane_cache.invalidate(&Lane::Milestones);
    Ok(())
}

pub fn reopen_annotation(runner: &MpRunner, app: &mut App, id: &str) -> Result<()> {
    runner.run_raw("annotation", &["reopen", id])?;
    app.lane_cache.invalidate(&Lane::Overview);
    app.lane_cache.invalidate(&Lane::Milestones);
    Ok(())
}

pub fn check_approval_status(runner: &MpRunner, app: &mut App, milestone_id: &str) -> Result<()> {
    let data = reads::list_annotations(runner, false, Some(milestone_id))?;
    let annotations = data["annotations"].as_array();
    let mut blocked = false;
    let mut ann_id: Option<String> = None;
    if let Some(arr) = annotations {
        for a in arr {
            let kind = a["kind"].as_str().unwrap_or("");
            let status = a["status"].as_str().unwrap_or("");
            if kind == "approval-request" && (status == "open" || status == "addressed") {
                blocked = true;
                ann_id = Some(a["id"].as_str().unwrap_or("?").to_string());
                break;
            }
        }
    }
    app.approval_blocked = blocked;
    app.approval_annotation_id = ann_id;
    Ok(())
}

pub fn co_approval_approve(runner: &MpRunner, app: &mut App, milestone_id: &str) -> Result<()> {
    runner.run_raw("milestone", &["approve", milestone_id])?;
    invalidate_after_lifecycle_write(app);
    Ok(())
}

pub fn create_approval_annotation(
    runner: &MpRunner,
    app: &mut App,
    milestone_id: &str,
) -> Result<()> {
    let payload = serde_json::json!({
        "target": milestone_id,
        "kind": "approval-request",
        "body": "Approval requested via raul TUI",
        "author": "raul-tui",
    });
    runner.run_stdin("annotation", &["create", "--json", "@-"], &payload)?;
    app.lane_cache.invalidate(&Lane::Overview);
    app.lane_cache.invalidate(&Lane::Milestones);
    Ok(())
}

pub fn execute_review_action(
    runner: &MpRunner,
    app: &mut App,
    ms_id: &str,
    action: &str,
) -> ReviewActionOutcome {
    match action {
        "Approve milestone" => {
            // M163: use `run_raw_capture` so we can inspect both stdout
            // and stderr — mp emits JSON failure payloads on stdout for
            // some commands and on stderr for others. The previous
            // `run_raw_allow_failure` only returned stdout, so M121
            // errors that landed on stderr were silently truncated to an
            // empty message.
            let (stdout, stderr, status) =
                match runner.run_raw_capture("milestone", &["approve", ms_id]) {
                    Ok(pair) => pair,
                    Err(e) => return ReviewActionOutcome::OtherError(e.to_string()),
                };
            // Build the union: stderr first (when mp writes the JSON
            // there, it's the only signal), then stdout. Either side
            // alone is enough for detection.
            let combined = merge_outputs(&stdout, &stderr);
            if let Some(m121) = detect_m121(&stdout, &stderr) {
                let full = String::from_utf8_lossy(&combined).trim().to_string();
                return ReviewActionOutcome::M121GateError {
                    ac_count: m121,
                    ms_id: ms_id.to_string(),
                    full,
                };
            }
            if let Err(e) =
                parse_mp_capture_response(&stdout, &stderr, &status, "milestone approve")
            {
                return ReviewActionOutcome::OtherError(e.to_string());
            }
            invalidate_after_lifecycle_write(app);
            ReviewActionOutcome::Ok
        }
        "Block milestone" => {
            let (stdout, stderr, status) = match runner.run_raw_capture(
                "milestone",
                &["block", ms_id, "--reason", "Blocked via raul TUI"],
            ) {
                Ok(pair) => pair,
                Err(e) => return ReviewActionOutcome::OtherError(e.to_string()),
            };
            if let Err(e) = parse_mp_capture_response(&stdout, &stderr, &status, "milestone block")
            {
                return ReviewActionOutcome::OtherError(e.to_string());
            }
            invalidate_after_lifecycle_write(app);
            ReviewActionOutcome::Ok
        }
        "Unblock milestone" => {
            let (stdout, stderr, status) =
                match runner.run_raw_capture("milestone", &["unblock", ms_id]) {
                    Ok(pair) => pair,
                    Err(e) => return ReviewActionOutcome::OtherError(e.to_string()),
                };
            if let Err(e) =
                parse_mp_capture_response(&stdout, &stderr, &status, "milestone unblock")
            {
                return ReviewActionOutcome::OtherError(e.to_string());
            }
            invalidate_after_lifecycle_write(app);
            ReviewActionOutcome::Ok
        }
        "Request grooming" => {
            let payload = serde_json::json!({
                "target": ms_id,
                "kind": "grooming-request",
                "body": "Grooming requested via raul TUI",
                "author": "raul-tui",
            });
            let (stdout, stderr, status) =
                match runner.run_stdin_capture("annotation", &["create", "--json", "@-"], &payload)
                {
                    Ok(pair) => pair,
                    Err(e) => return ReviewActionOutcome::OtherError(e.to_string()),
                };
            if let Err(e) =
                parse_mp_capture_response(&stdout, &stderr, &status, "annotation create")
            {
                return ReviewActionOutcome::OtherError(e.to_string());
            }
            app.lane_cache.invalidate(&Lane::Overview);
            app.lane_cache.invalidate(&Lane::Milestones);
            ReviewActionOutcome::Ok
        }
        // M172 S6: "Set dependency" opens the input overlay and asks
        // the user for a milestone ID. The submit handler shells out
        // to `mp milestone update <id> --json @-` with the new
        // dependency appended. The handler is wired in
        // `apply_action::SubmitInput` below; here we just open the
        // overlay so the menu closes and the input takes focus.
        "Set dependency" => {
            // The `target` field carries the source milestone ID
            // (the milestone that will RECEIVE the new dependency).
            // The user types the dependency milestone ID into the
            // input overlay.
            app.start_input(ms_id.to_string(), "set-dependency".to_string());
            ReviewActionOutcome::Ok
        }
        _ => ReviewActionOutcome::Ok,
    }
}

/// M182 S4: persist a sort-rebind choice to `mp config set
/// sort.<lane> <sortkey>`. The function name is the canonical lane
/// string (e.g. `milestones`, `backlog`, `tweaks`); the value is the
/// `SortKey::label()` output (`id` / `lifecycle` / `priority` /
/// `updated`). On success the bound choice lives in `App::lane_sort_key`
/// AND in `config.json`; the next raul launch loads it via
/// `load_persisted_sort_keys` so the chosen order survives restart.
pub fn persist_sort_rebind_choice(runner: &MpRunner, app: &mut App) -> Result<()> {
    let Some(key) = app.sort_rebind_highlight() else {
        // Menu not open or empty — `confirm` was called with no
        // selection. The dispatcher (modes::normal) gates this
        // action on `app.sort_rebind_open()`, so reaching here is a
        // no-op. Defensive bail: avoid a silent zero-state write.
        app.cancel_sort_rebind();
        return Ok(());
    };
    let lane = app.active_lane;
    let lane_key = match_lane_label(&lane);
    let value = key.label();
    let config_key = format!("sort.{lane_key}");
    runner.run_raw("config", &["set", &config_key, value])?;
    // Mirror the in-memory bind so the render reflects the user's
    // choice immediately (the parser-only path would otherwise lag
    // until the next refresh).
    app.lane_sort_key.insert(lane, key);
    app.confirm_sort_rebind();
    Ok(())
}

/// M182 S4: load per-lane sort keys from `mp config get
/// sort.<lane>` on startup. Each lane's value is parsed into a
/// `SortKey`; missing or empty config falls back to
/// `SortKey::Id` (the documented default).
///
/// The loader is best-effort: any failure (mp missing, config
/// corrupt, lane name typo) is logged via the caller's existing
/// flash-message surface and the lane keeps its in-memory default.
/// This matches the pre-existing `load_config` policy — we never
/// fail raul startup over a config read.
pub fn load_persisted_sort_keys(runner: &MpRunner, app: &mut App) -> Result<()> {
    let valid_lanes: [(Lane, &str); 3] = [
        (Lane::Milestones, "milestones"),
        (Lane::Backlog, "backlog"),
        (Lane::Ideas, "ideas"),
    ];
    for (lane, lane_key) in valid_lanes {
        let key = format!("sort.{lane_key}");
        // M182 S4 (external review F-03): `mp config get` returns the
        // JSON envelope `{"key": "...", "value": "<value>"}`, not the
        // bare value. Pre-fix the loader passed the envelope string
        // to the SortKey::label match, which then fell into the
        // "unknown sort key" branch and silently dropped every
        // persisted binding. Parse the JSON envelope to extract the
        // `value` field; on any parse failure, fall through to the
        // empty-value path (lane stays on its default).
        let raw = runner.run_raw("config", &["get", &key]).unwrap_or_default();
        let value = parse_config_get_value(&raw).unwrap_or_default();
        let sort_key = match value.as_str() {
            "id" => crate::tui::app::SortKey::Id,
            // M205: pre-M205 bindings persisted as "lifecycle" — the
            // Stage sort replaces the legacy Lifecycle variant
            // (Stage column owns that signal now). The "stage"
            // label is the canonical post-M205 spelling; pre-M205
            // "lifecycle" values fall through to the unknown branch
            // below and silently fall back to the per-lane default
            // (per AC-08 — legacy migration must not break existing
            // users).
            "stage" => crate::tui::app::SortKey::Stage,
            "priority" => crate::tui::app::SortKey::Priority,
            "updated" => crate::tui::app::SortKey::Updated,
            // M205: cross-lane Created sort.
            "created" => crate::tui::app::SortKey::Created,
            // Backlog-shaped sort key (status ranks backlog/ideas rows).
            "status" => crate::tui::app::SortKey::Status,
            // M205: Backlog-only ResolvedAt sort.
            "resolved-at" => crate::tui::app::SortKey::ResolvedAt,
            // M205: Ideas-only Tags sort.
            "tags" => crate::tui::app::SortKey::Tags,
            // Alphabetical title sort (cross-lane).
            "title" => crate::tui::app::SortKey::Title,
            "" => continue,
            other => {
                // Unknown sort key — the lane stays on its default.
                // A future config-cleanup milestone can prune stale
                // sort.* entries; we don't write back here so a
                // transient typo can't clobber the user's real choice.
                // Pre-M205 "lifecycle" values land here silently — the
                // per-lane default (Id) is the M182 S3 fallback
                // (AC-08: legacy lifecycle sort falls back to default).
                eprintln!("raul: ignoring unknown sort key for lane {lane_key}: {other:?}");
                continue;
            }
        };
        app.lane_sort_key.insert(lane, sort_key);
    }
    Ok(())
}

/// Parse `mp config get <key>` output. The command returns
/// `{"key": "<key>", "value": "<value>"}` as a JSON object; we
/// extract the `value` field as a string. Returns `None` when the
/// input isn't valid JSON (raw shell output, mp missing, etc.) so
/// the caller can fall back to the lane default.
fn parse_config_get_value(raw: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(raw).ok()?;
    v.get("value")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
}

/// Map a [`Lane`] to its canonical `mp config set sort.<lane>`
/// segment. Mirrors `Lane::compact_label()` but in lowercase, and
/// special-cases `Overview` / `Path` / `Settings` to a sentinel that
/// fails the mp-side lane validation. The M172 S5 sort menu only
/// opens on lanes that expose it, so the sentinel is unreachable in
/// practice — but the function is `pub` for completeness (a future
/// milestone might surface sort on Overview via a custom toggle).
pub fn match_lane_label(lane: &Lane) -> String {
    match lane {
        Lane::Overview => "overview".to_string(),
        Lane::Milestones => "milestones".to_string(),
        Lane::Path => "path".to_string(),
        Lane::Backlog => "backlog".to_string(),
        Lane::Ideas => "ideas".to_string(),
        Lane::Watch => "watch".to_string(),
        Lane::Settings => "settings".to_string(),
    }
}

pub fn invalidate_after_lifecycle_write(app: &mut App) {
    app.lane_cache.invalidate(&Lane::Milestones);
    app.lane_cache.invalidate(&Lane::Overview);
}

/// Combine stdout + stderr into a single buffer for M121 detection.
///
/// mp writes its structured JSON failure payloads on stdout for some
/// subcommands and stderr for others (the contract is not consistent
/// across the CLI surface). The M163 review-action path needs to find
/// M121 markers regardless of which stream carried them — so we merge
/// the two and search the union.
///
/// Preference order: stderr is included first when non-empty (its JSON
/// is the authoritative signal for the commands that emit on stderr),
/// followed by stdout. Whitespace separator avoids accidentally
/// concatenating two JSON objects into something that still parses but
/// hides one of them.
fn merge_outputs(stdout: &[u8], stderr: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(stdout.len() + stderr.len() + 2);
    if !stderr.is_empty() {
        out.extend_from_slice(stderr);
        if !stderr.ends_with(b"\n") {
            out.push(b'\n');
        }
    }
    if !stdout.is_empty() {
        out.extend_from_slice(stdout);
    }
    out
}

/// Count M121 error entries in either captured output stream. Each stream is
/// parsed as a complete JSON document first so pretty-printed gate reports
/// remain valid; newline-delimited JSON is retained as a fallback for mixed
/// diagnostic output. When both streams repeat the same report, the larger
/// per-document count wins instead of double-counting the gate errors.
fn detect_m121(stdout: &[u8], stderr: &[u8]) -> Option<usize> {
    [stderr, stdout]
        .into_iter()
        .filter_map(count_m121_in_stream)
        .max()
}

fn count_m121_in_stream(stream: &[u8]) -> Option<usize> {
    let mut best = serde_json::from_slice::<serde_json::Value>(stream)
        .ok()
        .and_then(|value| count_m121_in(&value));

    for value in serde_json::Deserializer::from_slice(stream)
        .into_iter::<serde_json::Value>()
        .flatten()
    {
        if let Some(count) = count_m121_in(&value) {
            best = Some(best.map_or(count, |current| current.max(count)));
        }
    }

    if best.is_none() {
        for line in stream.split(|byte| *byte == b'\n') {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(line.trim_ascii()) {
                if let Some(count) = count_m121_in(&value) {
                    best = Some(best.map_or(count, |current| current.max(count)));
                }
            }
        }
    }

    best
}

fn count_m121_in(value: &serde_json::Value) -> Option<usize> {
    let errors = value.get("errors")?.as_array()?;
    let n = errors
        .iter()
        .filter(|error| error.get("code").and_then(|code| code.as_str()) == Some("M121"))
        .count();
    (n > 0).then_some(n)
}

/// Focused flash message for an M121 gate error. The shape is fixed so
/// the AC-04 regex can pin it; the value is computed from the
/// structured JSON (via `count_m121_in`) so a partial `mp` output or a
/// different milestone id never silently slips through.
pub fn format_m121_flash_message(ms_id: &str, ac_count: usize) -> String {
    format!(
        "Cannot approve M{ms_id}: {ac_count} AC(s) have unresolved verifications. Run: mp plan verify-ac {ms_id}"
    )
}

pub fn parse_mp_ok_response(stdout: &[u8], stderr: &[u8], label: &str) -> Result<()> {
    parse_mp_response(stdout, stderr, true, Some(0), label)
}

fn parse_mp_capture_response(
    stdout: &[u8],
    stderr: &[u8],
    status: &std::process::ExitStatus,
    label: &str,
) -> Result<()> {
    parse_mp_response(stdout, stderr, status.success(), status.code(), label)
}

fn parse_mp_response(
    stdout: &[u8],
    stderr: &[u8],
    success: bool,
    code: Option<i32>,
    label: &str,
) -> Result<()> {
    for stream in [stdout, stderr] {
        if let Ok(response) = serde_json::from_slice::<serde_json::Value>(stream) {
            if response["ok"].as_bool() == Some(true) && success {
                return Ok(());
            }
            if let Some(err) = response.get("error").and_then(|error| error.as_str()) {
                anyhow::bail!("{label}: {err}");
            }
        }
    }

    let stdout_text = String::from_utf8_lossy(stdout);
    let stderr_text = String::from_utf8_lossy(stderr);
    let combined = match (stdout_text.trim().is_empty(), stderr_text.trim().is_empty()) {
        (false, false) => format!("{} | {}", stdout_text.trim(), stderr_text.trim()),
        (false, true) => stdout_text.trim().to_string(),
        (true, false) => stderr_text.trim().to_string(),
        (true, true) => "no output from mp".to_string(),
    };
    let code = code.unwrap_or(-1);
    anyhow::bail!("{label} exited with code {code}: {combined}")
}

// ============================================================================
// Lane-level + navigation glue
// ============================================================================

/// M169: load Settings lane state from `mp config show`.
///
/// **M169-rev (HIGH fix):** no-op when `app.settings` is already populated.
/// `load_data_for_lane` is called unconditionally after every lane-nav action
/// (`NextLane` / `PreviousLane` / `JumpLane`), and Tab on the Settings lane
/// is a no-op for `tab_move_down()` but still fires this loader — without
/// the guard, every Tab/click on the Settings tab while already on it
/// silently overwrites the user's staged edits and spawns an extra
/// `mp config show` subprocess. Re-entering the lane from another lane
/// is still a fresh load because `select_lane` clears `app.settings`
/// when leaving Settings.
///
/// **M201:** also fetch `mp config schema` once at lane-open and cache
/// the parsed result on `SettingsState.schema`. The schema is the
/// single source of truth for per-key type, default, allowed, and
/// description — see `modes::settings::schema`. A failed schema fetch
/// is non-fatal: the lane still opens, the renderer surfaces a
/// clear warning, and the user can update `mp` (the hint names
/// `mp --version` per AC-08).
/// M201: fetch `mp config schema` through the runner and parse it
/// into the typed `SettingsSchema`. Lives in the runner layer (not
/// the per-mode handler tree) so per-mode handlers stay pure.
fn fetch_settings_schema(
    runner: &MpRunner,
) -> Result<super::modes::settings::schema::SettingsSchema, String> {
    let raw = runner
        .run_raw("config", &["schema"])
        .map_err(|e| format!("mp config schema unavailable: {e}"))?;
    super::modes::settings::schema::SettingsSchema::from_json(&raw)
}

pub fn load_settings_lane(runner: &MpRunner, app: &mut App) -> Result<()> {
    use super::mode::SettingsState;
    if app.settings.is_some() {
        return Ok(());
    }
    let data: serde_json::Value = runner.run("config", &["show"])?;
    let config = data
        .get("config")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    // M201: fetch and cache the schema once. Failure is non-fatal —
    // the renderer surfaces a clear error and the lane stays usable.
    // The fetch lives here (the runner layer), NOT in the per-mode
    // handler tree — per-mode handlers must stay pure (no MpRunner).
    let (cached_schema, warning) = match fetch_settings_schema(runner) {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(e)),
    };

    app.settings = Some(SettingsState {
        config,
        schema: cached_schema,
        selected_idx: 0,
        focus: super::mode::SettingsFocus::Fields,
        edit: None,
        staged_edits: std::collections::BTreeMap::new(),
        schema_warning: warning,
    });
    app.content = super::app::ContentState::List;
    app.touch();
    Ok(())
}

pub fn load_data_for_lane(runner: &MpRunner, app: &mut App) -> Result<()> {
    if let Some(mp_dir) = mp_dir_for_runner(runner) {
        app.lane_cache.check_and_update_mtime(&mp_dir);
    }
    match app.active_lane {
        Lane::Overview => load_dashboard(runner, app),
        Lane::Milestones => load_milestones(runner, app),
        Lane::Backlog | Lane::Ideas => load_backlog(runner, app),
        Lane::Path => load_path_data(runner, app),
        // M169: Settings lane loads config from `mp config show`.
        Lane::Settings => load_settings_lane(runner, app),
        // M179: Watch lane data is owned by the Watch module
        // (S3). The data model is selection-driven, not list-
        // driven, so refresh uses `mp list milestones` (the
        // picker source). S7 adds the periodic poller.
        Lane::Watch => load_watch_picker(runner, app),
    }
}

pub fn mp_dir_for_runner(runner: &MpRunner) -> Option<std::path::PathBuf> {
    if let Some(dir) = runner.mp_dir() {
        return Some(dir.to_path_buf());
    }
    if let Some(root) = runner.project_root() {
        return Some(root.join("master-plan"));
    }
    None
}

pub fn navigate_from_inbox_item(app: &mut App, runner: &MpRunner, item: &InboxLine) -> Result<()> {
    use super::inbox_nav::{apply_inbox_navigation, InboxNavFollowUp};

    match item.kind.as_str() {
        "milestone" => {
            app.select_lane(Lane::Milestones);
            load_milestones(runner, app)?;
        }
        "backlog" => {
            app.select_lane(Lane::Backlog);
            load_backlog(runner, app)?;
        }
        _ => {}
    }

    match apply_inbox_navigation(app, item) {
        InboxNavFollowUp::LoadMilestoneDetail(id) => {
            load_milestone_detail(runner, app, &id)?;
        }
        InboxNavFollowUp::None => {}
    }
    Ok(())
}

/// Run `mp plan verify-ac <ms_id>` and parse the result into a `PreflightGate`.
/// Per-AC statuses drive the gate because the top-level `ok` and
/// `unresolvable` fields do not fully represent the approval predicate.
pub fn load_preflight_gate(runner: &MpRunner, ms_id: &str) -> PreflightGate {
    let (stdout, stderr, status) = match runner.run_raw_capture("plan", &["verify-ac", ms_id]) {
        Ok(output) => output,
        Err(error) => return failed_preflight(error.to_string()),
    };

    if !status.success() {
        let output = String::from_utf8_lossy(&merge_outputs(&stdout, &stderr))
            .trim()
            .to_string();
        let code = status.code().unwrap_or(-1);
        return failed_preflight(if output.is_empty() {
            format!("mp plan verify-ac exited with code {code} and no output")
        } else {
            format!("mp plan verify-ac exited with code {code}: {output}")
        });
    }

    let json = match find_verify_ac_payload(&stdout, &stderr) {
        Some(value) => value,
        None => return failed_preflight("mp plan verify-ac returned malformed JSON".to_string()),
    };
    if json.get("ok").and_then(|value| value.as_bool()) == Some(false) {
        return failed_preflight(
            "mp plan verify-ac reported ok=false; the approval gate is closed until the report resolves".to_string(),
        );
    }
    let acs = match json.get("acs").and_then(|value| value.as_array()) {
        Some(acs) if !acs.is_empty() => acs,
        _ => {
            return failed_preflight(
                "mp plan verify-ac returned no acceptance criteria".to_string(),
            )
        }
    };
    let failing_count = acs
        .iter()
        .filter(|entry| {
            let status = entry
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            !is_passing_ac_status(status)
        })
        .count()
        .max(
            json.get("unresolvable")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as usize,
        );

    PreflightGate {
        open: failing_count == 0,
        unresolvable_count: failing_count,
        error: None,
    }
}

fn failed_preflight(error: String) -> PreflightGate {
    PreflightGate {
        open: false,
        unresolvable_count: 0,
        error: Some(error),
    }
}

fn is_passing_ac_status(status: &str) -> bool {
    matches!(status, "resolved" | "manual" | "runtime" | "inline")
}

fn find_verify_ac_payload(stdout: &[u8], stderr: &[u8]) -> Option<serde_json::Value> {
    for stream in [stdout, stderr] {
        if let Some(value) = find_verify_ac_payload_in_stream(stream) {
            return Some(value);
        }
    }
    for stream in [stdout, stderr] {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(stream) {
            return Some(value);
        }
    }
    None
}

fn find_verify_ac_payload_in_stream(stream: &[u8]) -> Option<serde_json::Value> {
    for value in serde_json::Deserializer::from_slice(stream)
        .into_iter::<serde_json::Value>()
        .flatten()
    {
        if value.get("acs").and_then(|acs| acs.as_array()).is_some() {
            return Some(value);
        }
    }

    stream
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line.trim_ascii()).ok())
        .find(|value| value.get("acs").and_then(|acs| acs.as_array()).is_some())
}

#[cfg(test)]
mod tests {
    //! M202 unit tests. The `parse_milestone_summaries` helper is
    //! the single chokepoint for converting `mp list milestones`
    //! JSON rows into `MilestoneSummary` values; its flow_stages
    //! parsing pin lives here so a regression in the helper is
    //! caught by the model-side test instead of by the lane
    //! renderer's surface.
    use super::{parse_backlog_lines, parse_milestone_summaries};
    use std::collections::BTreeMap;

    fn payload_with(milestone: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "milestones": [milestone] })
    }

    // ── M203 S4: parse_backlog_lines populates the preview field ──

    #[test]
    fn parse_backlog_lines_populates_preview() {
        let payload = serde_json::json!({
            "backlog": [
                {
                    "id": "BL-01",
                    "description": "Title\nContinuation detail",
                    "priority": "high",
                    "status": "active",
                    "resolution": "",
                    "preview": "Continuation detail"
                },
                {
                    "id": "BL-02",
                    "description": "Resolved row",
                    "priority": "medium",
                    "status": "resolved",
                    "resolution": "shipped",
                    "preview": "resolved · shipped"
                },
                {
                    "id": "BL-03",
                    "description": "Empty resolution resolved row",
                    "priority": "low",
                    "status": "resolved",
                    "resolution": "",
                    "preview": "resolved"
                },
                {
                    "id": "BL-04",
                    "description": "Single line, no continuation",
                    "priority": "low",
                    "status": "active",
                    "resolution": "",
                    "preview": ""
                },
                {
                    // Legacy payload (no `preview` key) — parser must
                    // default to empty string.
                    "id": "BL-05",
                    "description": "Legacy row",
                    "priority": "low",
                    "status": "active",
                    "resolution": ""
                }
            ]
        });
        let lines = parse_backlog_lines(&payload);
        assert_eq!(lines.len(), 5);

        // Active continuation lands in `preview` verbatim (the parser
        // does not re-derive it from `description` — that's the mp
        // projection's job).
        assert_eq!(lines[0].preview, "Continuation detail");

        // Resolved with a resolution projects the resolution chip.
        assert_eq!(lines[1].preview, "resolved · shipped");

        // Empty resolution collapses to "resolved".
        assert_eq!(lines[2].preview, "resolved");

        // Active item with no continuation projects "".
        assert_eq!(lines[3].preview, "");

        // Legacy payload (no `preview` key) defaults to "".
        assert_eq!(lines[4].preview, "");
        // And all other fields still parse.
        assert_eq!(lines[4].id, "BL-05");
        assert_eq!(lines[4].resolution, "");
    }

    #[test]
    fn parse_milestone_summaries_populates_flow_stages() {
        let payload = payload_with(serde_json::json!({
            "id": "01",
            "title": "Sample",
            "lifecycle": "complete",
            "lifecycle_at": "2026-09-01T00:00:00Z",
            "depends_on": [],
            "priority": "normal",
            "updated": "2026-09-01",
            "cancelled": false,
            "cancelled_at": null,
            "cancel_reason": null,
            "flow_stages": {
                "draft": {"status": "done", "at": "2026-08-01T00:00:00Z"},
                "groom": {"status": "done", "at": "2026-08-02T00:00:00Z"},
                "specify": {"status": "done", "at": "2026-08-03T00:00:00Z"},
                "approve": {"status": "done", "at": "2026-08-04T00:00:00Z"},
                "execute": {"status": "done", "at": "2026-08-05T00:00:00Z"},
                "self-review": {"status": "done", "at": "2026-08-06T00:00:00Z"},
                "complete": {"status": "done", "at": "2026-08-07T00:00:00Z"},
                "external-review": {"status": "in_progress", "at": "2026-08-08T00:00:00Z"}
            }
        }));
        let summaries = parse_milestone_summaries(&payload);
        assert_eq!(summaries.len(), 1);
        let flow = &summaries[0].flow_stages;
        // Every canonical stage slug from the projection must be
        // present as a status string.
        assert_eq!(flow.len(), 8, "got: {flow:?}");
        assert_eq!(flow.get("draft").map(String::as_str), Some("done"));
        assert_eq!(
            flow.get("external-review").map(String::as_str),
            Some("in_progress")
        );
        // Hand-off is absent from the on-disk payload — the parser
        // must not invent it.
        assert!(flow.get("hand-off").is_none());
    }

    #[test]
    fn parse_milestone_summaries_treats_empty_flow_stages_as_empty_map() {
        // Pre-M202 fixtures always emit `flow_stages: {}`. The
        // helper must return an empty BTreeMap (the default) so the
        // lane renderer falls back to `pending` for every stage.
        let payload = payload_with(serde_json::json!({
            "id": "01",
            "title": "Legacy pre-M202",
            "lifecycle": "complete",
            "lifecycle_at": null,
            "depends_on": [],
            "priority": "normal",
            "updated": "2026-06-01",
            "cancelled": false,
            "cancelled_at": null,
            "cancel_reason": null,
            "flow_stages": {}
        }));
        let summaries = parse_milestone_summaries(&payload);
        assert_eq!(summaries.len(), 1);
        let expected: BTreeMap<String, String> = BTreeMap::new();
        assert_eq!(summaries[0].flow_stages, expected);
    }
}
