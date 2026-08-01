use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

use crate::model::MilestoneFile;
use crate::paths::PlanContext;
use crate::store;
use crate::track_kind;

/// All artifact-type filters `mp search` accepts. Single source of
/// truth — the CLI in `commands/search.rs` validates against this list,
/// and `search_plan`'s `wants` checks must stay aligned with it. Keep
/// these two in sync; the order is the order shown in the CLI's
/// "invalid --type" error message.
pub const VALID_SEARCH_TYPES: &[&str] = &[
    "milestone",
    "title",
    "step",
    "ac",
    "wp",
    "idea",
    "backlog",
    "track",
    "decision",
];

/// Default snippet context (chars before+after the match position).
/// Tuned for AC descriptions and step actions in the dogfood plan;
/// callers can override via the env-tuneable constant for dense fields.
const DEFAULT_SNIPPET_CONTEXT: usize = 60;

/// L5 remediation: groups the parameters that are constant across all
/// `push_if_match` call sites in a single search_plan invocation.
/// Reduces the function signature from 10 parameters to 8 (removes the
/// clippy suppression). The 4 per-call data fields (text, id, title,
/// matched_field) are kept as direct parameters.
struct HitContext<'a> {
    results: &'a mut Vec<SearchResult>,
    query: &'a str,
    artifact_type: &'a str,
    id: &'a str,
    title: &'a str,
    matched_field: &'a str,
    source: &'a str,
    parent_milestone_id: Option<&'a str>,
    suggested_action: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub score: f64,
    pub artifact_type: String,
    pub id: String,
    pub title: String,
    pub matched_field: String,
    pub snippet: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_milestone_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
    /// Full matched fragment when the caller passed `--include object`.
    /// Omitted from JSON otherwise to keep the default response compact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<Value>,
}

/// DIY fuzzy scorer — no external crate.
/// Returns (score, match_position) or None if no match.
///
/// Scoring tiers:
///   1. Exact substring match   → 0.85–1.0  (weighted by position + density)
///   2. Sequential char match   → 0.40–0.85 (weighted by coverage + gap penalty)
///
/// Tier 2 matches may skip characters between query chars: gaps are
/// allowed but penalized. That is deliberate — for an agent prompt
/// scanning plan content, exact-keyword match is too strict, while
/// subsequence match over different whitespace/glyphs is closer to
/// what "fuzzy search" means to the caller. `snippet` depends on
/// `match_pos` from this function returning a real CHAR index into
/// the original `text`, which all tiers preserve.
///
/// Note on Tier 2 permissiveness: Tier 2 fires on any single query
/// character that appears in sequence in the text (matches > 0). That
/// means very short queries like "a" or "the" match almost every plan
/// artifact and `--type all` returns a noisy top-20 list. This is a
/// deliberate trade-off — prefer tier-1 substring match for exact
/// lookups, and supply a longer/more specific query when the plan is
/// large. Callers should treat short queries as exploratory, not as
/// a precision tool.
pub fn fuzzy_match(query: &str, text: &str) -> Option<(f64, usize)> {
    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    let t: Vec<char> = text.chars().flat_map(|c| c.to_lowercase()).collect();
    if q.is_empty() || t.is_empty() {
        return None;
    }
    let qs: String = q.iter().collect();
    let ts: String = t.iter().collect();

    // Tier 1: exact substring match.
    // Rewritten so the intermediate value stays within [0, 1] (no
    // overshoot-then-clamp like the previous `1.0 - x*0.3 + y*0.1`).
    // `match_pos` is converted from a byte offset (str::find) into a
    // char index so `snippet()` can index the original chars vec.
    if let Some(byte_pos) = ts.find(&qs) {
        let char_pos = ts[..byte_pos].chars().count();
        let pos_ratio = char_pos as f64 / t.len().max(1) as f64;
        let score = (0.85 + (1.0 - pos_ratio) * 0.15).min(1.0);
        return Some((score, char_pos));
    }

    // Tier 2: character-by-character sequential match (gaps allowed;
    // see fuzzy_match doc comment above).
    let mut ti = 0;
    let mut matches = 0;
    let mut first_match = t.len();
    let mut gaps = 0usize;
    let mut prev_pos = None;
    for &qc in &q {
        while ti < t.len() && t[ti] != qc {
            ti += 1;
        }
        if ti < t.len() {
            if matches == 0 {
                first_match = ti;
            }
            if let Some(pp) = prev_pos {
                gaps += ti.saturating_sub(pp + 1);
            }
            prev_pos = Some(ti);
            matches += 1;
            ti += 1;
        } else {
            break;
        }
    }
    if matches > 0 {
        let coverage = matches as f64 / q.len() as f64;
        let pos_ratio = first_match as f64 / t.len().max(1) as f64;
        let gap_penalty = 1.0 - (gaps as f64 / t.len().max(1) as f64).min(0.5);
        let score = (coverage * (0.85 - pos_ratio * 0.3) * gap_penalty).clamp(0.4, 0.85);
        return Some((score, first_match));
    }

    None
}

fn snippet(text: &str, match_pos: usize, context: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let start = match_pos.saturating_sub(context);
    let end = (match_pos + context).min(chars.len());
    let snip: String = chars[start..end].iter().collect();
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < chars.len() { "…" } else { "" };
    format!("{prefix}{snip}{suffix}")
}

/// L5 remediation: called with HitContext (8 constant fields) + text to score.
/// The context groups the parameters that are the same at every call site within
/// a search_plan invocation, eliminating the clippy suppression.
fn push_if_match(ctx: HitContext, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some((score, pos)) = fuzzy_match(ctx.query, text) {
        ctx.results.push(SearchResult {
            score,
            artifact_type: ctx.artifact_type.to_string(),
            id: ctx.id.to_string(),
            title: ctx.title.to_string(),
            matched_field: ctx.matched_field.to_string(),
            // L4 remediation: snippet context is now a constant instead
            // of a magic 60. Per-artifact-type overrides can extend the
            // signature with a `snippet_context: usize` parameter; for
            // now we use the default for every artifact type.
            snippet: snippet(text, pos, DEFAULT_SNIPPET_CONTEXT),
            source: ctx.source.to_string(),
            parent_milestone_id: ctx.parent_milestone_id.map(str::to_string),
            suggested_action: ctx.suggested_action.map(str::to_string),
            object: None,
        });
    }
}

fn dedup_sort_limit(mut results: Vec<SearchResult>, limit: usize) -> Vec<SearchResult> {
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    results.retain(|r| {
        seen.insert((
            r.artifact_type.clone(),
            r.id.clone(),
            r.matched_field.clone(),
        ))
    });
    results.truncate(limit);
    results
}

/// Search plan artifacts. `type_filter` may be: None (all), or one of:
/// "milestone", "title", "step", "ac", "wp", "idea", "backlog", "track",
/// "decision". `include_object` attaches the full matched fragment under
/// `hit.object` for fragment-first reads (M93).
pub fn search_plan(
    ctx: &PlanContext,
    query: &str,
    type_filter: Option<&str>,
    limit: usize,
    include_object: bool,
) -> Result<Vec<SearchResult>> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    let filter_all = type_filter.is_none();
    let wants = |t: &str| filter_all || type_filter == Some(t);

    // M1 fix: hoist the milestone load to function scope when either
    // the scan branch or the include_object branch needs it. This
    // avoids a second load_all_milestones round-trip when both run.
    let needs_milestones =
        wants("milestone") || wants("ac") || wants("wp") || wants("title") || wants("step");
    let loaded_milestones: Option<Vec<MilestoneFile>> = if needs_milestones || include_object {
        match store::load_all_milestones(ctx) {
            Ok(loaded) => Some(loaded.into_iter().map(|(_, m)| m).collect()),
            Err(_) => None,
        }
    } else {
        None
    };

    // ── Milestones + ACs + steps + WPs (M95: ac, wp, title scopes) ──
    if needs_milestones {
        if let Some(ref milestones) = loaded_milestones {
            for m in milestones.iter() {
                let mid_display =
                    format!("M{}", crate::paths::normalize_milestone_id(&m.milestone.id));
                let source = format!(
                    "{}/{}.json",
                    ctx.milestones_dir().display(),
                    m.milestone.slug
                );
                let milestone_suggested = format!("mp show milestone {mid_display} --summary");

                // ── Milestone-level (broad: title + intent + problem + scope lines) ──
                // parent_milestone_id is omitted on milestone hits — a milestone IS
                // the parent, not nested. group_by_milestone derives the group key
                // from r.id for milestone artifact_type.
                if wants("milestone") {
                    push_if_match(
                        HitContext {
                            results: &mut results,
                            query: &q,
                            artifact_type: "milestone",
                            id: &mid_display,
                            title: &m.milestone.title,
                            matched_field: "title",
                            source: &source,
                            parent_milestone_id: None,
                            suggested_action: Some(&milestone_suggested),
                        },
                        &m.milestone.title,
                    );
                    push_if_match(
                        HitContext {
                            results: &mut results,
                            query: &q,
                            artifact_type: "milestone",
                            id: &mid_display,
                            title: &m.milestone.title,
                            matched_field: "intent.outcome",
                            source: &source,
                            parent_milestone_id: None,
                            suggested_action: Some(&milestone_suggested),
                        },
                        &m.intent.outcome,
                    );
                    push_if_match(
                        HitContext {
                            results: &mut results,
                            query: &q,
                            artifact_type: "milestone",
                            id: &mid_display,
                            title: &m.milestone.title,
                            matched_field: "problem.description",
                            source: &source,
                            parent_milestone_id: None,
                            suggested_action: Some(&milestone_suggested),
                        },
                        &m.problem.description,
                    );
                    for line in &m.scope.in_scope {
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "milestone",
                                id: &mid_display,
                                title: &m.milestone.title,
                                matched_field: "scope.in_scope",
                                source: &source,
                                parent_milestone_id: None,
                                suggested_action: Some(&milestone_suggested),
                            },
                            line,
                        );
                    }
                    for line in &m.scope.out_of_scope {
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "milestone",
                                id: &mid_display,
                                title: &m.milestone.title,
                                matched_field: "scope.out_of_scope",
                                source: &source,
                                parent_milestone_id: None,
                                suggested_action: Some(&milestone_suggested),
                            },
                            line,
                        );
                    }
                }

                // ── Title only (narrow M95 scope) ──
                // title ⊆ milestone, so this only adds hits when the caller asked
                // for title scope without the broader milestone scope (avoids the
                // double-push + dedup churn when both are requested).
                if wants("title") && !wants("milestone") {
                    push_if_match(
                        HitContext {
                            results: &mut results,
                            query: &q,
                            artifact_type: "milestone",
                            id: &mid_display,
                            title: &m.milestone.title,
                            matched_field: "title",
                            source: &source,
                            parent_milestone_id: None,
                            suggested_action: Some(&milestone_suggested),
                        },
                        &m.milestone.title,
                    );
                }

                // ── Acceptance criteria (M95: first-class artifact type) ──
                if wants("ac") {
                    for ac in &m.acceptance_criteria {
                        let ac_id = format!("{mid_display}/{}", ac.id);
                        let ac_suggested = format!("mp milestone ac show {mid_display} {}", ac.id);
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "acceptance_criterion",
                                id: &ac_id,
                                title: &m.milestone.title,
                                matched_field: "description",
                                source: &source,
                                parent_milestone_id: Some(&mid_display),
                                suggested_action: Some(&ac_suggested),
                            },
                            &ac.description,
                        );
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "acceptance_criterion",
                                id: &ac_id,
                                title: &m.milestone.title,
                                matched_field: "verification",
                                source: &source,
                                parent_milestone_id: Some(&mid_display),
                                suggested_action: Some(&ac_suggested),
                            },
                            &ac.verification,
                        );
                    }
                }

                // ── Work packages (M95: new artifact type) ──
                if wants("wp") {
                    for wp in &m.work_packages {
                        let wp_id = format!("{mid_display}/{}", wp.id);
                        let wp_suggested =
                            format!("mp show milestone {mid_display} --fields work_packages");
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "work_package",
                                id: &wp_id,
                                title: &wp.name,
                                matched_field: "name",
                                source: &source,
                                parent_milestone_id: Some(&mid_display),
                                suggested_action: Some(&wp_suggested),
                            },
                            &wp.name,
                        );
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "work_package",
                                id: &wp_id,
                                title: &wp.name,
                                matched_field: "goal",
                                source: &source,
                                parent_milestone_id: Some(&mid_display),
                                suggested_action: Some(&wp_suggested),
                            },
                            &wp.goal,
                        );
                    }
                }

                // ── Steps (M95: extend to action + done_when + tests) ──
                if wants("step") {
                    for step in &m.steps {
                        let step_id = format!("{mid_display}/{}", step.id);
                        let step_suggested =
                            format!("mp milestone step show {mid_display} {}", step.id);
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "step",
                                id: &step_id,
                                title: &m.milestone.title,
                                matched_field: "action",
                                source: &source,
                                parent_milestone_id: Some(&mid_display),
                                suggested_action: Some(&step_suggested),
                            },
                            &step.action,
                        );
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "step",
                                id: &step_id,
                                title: &m.milestone.title,
                                matched_field: "done_when",
                                source: &source,
                                parent_milestone_id: Some(&mid_display),
                                suggested_action: Some(&step_suggested),
                            },
                            &step.done_when,
                        );
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "step",
                                id: &step_id,
                                title: &m.milestone.title,
                                matched_field: "tests",
                                source: &source,
                                parent_milestone_id: Some(&mid_display),
                                suggested_action: Some(&step_suggested),
                            },
                            &step.tests,
                        );
                    }
                }
            }
        }
    }

    // ── Ideas ────────────────────────────────────────────────────
    if wants("idea") {
        if let Ok(ideas) = store::load_ideas(ctx) {
            for idea in &ideas.ideas {
                let source = ctx.ideas_path().display().to_string();
                let suggested = format!("mp idea show {}", idea.id);
                push_if_match(
                    HitContext {
                        results: &mut results,
                        query: &q,
                        artifact_type: "idea",
                        id: &idea.id,
                        title: &idea.title,
                        matched_field: "title",
                        source: &source,
                        parent_milestone_id: None,
                        suggested_action: Some(&suggested),
                    },
                    &idea.title,
                );
                if !idea.body.is_empty() {
                    push_if_match(
                        HitContext {
                            results: &mut results,
                            query: &q,
                            artifact_type: "idea",
                            id: &idea.id,
                            title: &idea.title,
                            matched_field: "body",
                            source: &source,
                            parent_milestone_id: None,
                            suggested_action: Some(&suggested),
                        },
                        &idea.body,
                    );
                }
            }
        }
    }

    // ── Backlog ──────────────────────────────────────────────────
    if wants("backlog") {
        if let Ok(backlog) = store::load_backlog(ctx) {
            for item in &backlog.items {
                let source = ctx.backlog_path().display().to_string();
                let suggested = format!("mp backlog show {}", item.id);
                push_if_match(
                    HitContext {
                        results: &mut results,
                        query: &q,
                        artifact_type: "backlog",
                        id: &item.id,
                        title: &item.description,
                        matched_field: "description",
                        source: &source,
                        parent_milestone_id: None,
                        suggested_action: Some(&suggested),
                    },
                    &item.description,
                );
            }
        }
    }

    // ── Tracks ───────────────────────────────────────────────────
    if wants("track") {
        for &tk in &track_kind::TrackKind::ALL {
            let kind = tk.as_str();
            if let Ok(track) = store::load_track(ctx, kind) {
                for item in &track.items {
                    let source = ctx.track_path(kind).display().to_string();
                    let item_id = format!("{}-{}", tk.prefix(), item.id);
                    let suggested = format!("mp track show {kind} {}", item.id);
                    push_if_match(
                        HitContext {
                            results: &mut results,
                            query: &q,
                            artifact_type: "track",
                            id: &item_id,
                            title: &item.title,
                            matched_field: "title",
                            source: &source,
                            parent_milestone_id: None,
                            suggested_action: Some(&suggested),
                        },
                        &item.title,
                    );
                    if !item.problem.is_empty() {
                        push_if_match(
                            HitContext {
                                results: &mut results,
                                query: &q,
                                artifact_type: "track",
                                id: &item_id,
                                title: &item.title,
                                matched_field: "problem",
                                source: &source,
                                parent_milestone_id: None,
                                suggested_action: Some(&suggested),
                            },
                            &item.problem,
                        );
                    }
                }
            }
        }
    }

    // ── Decisions ────────────────────────────────────────────────
    if wants("decision") {
        if let Ok(decisions) = store::load_decisions(ctx) {
            for d in &decisions.decisions {
                let source = ctx.decisions_path().display().to_string();
                let suggested = format!("mp decision show {}", d.id);
                push_if_match(
                    HitContext {
                        results: &mut results,
                        query: &q,
                        artifact_type: "decision",
                        id: &d.id,
                        title: &d.summary,
                        matched_field: "summary",
                        source: &source,
                        parent_milestone_id: None,
                        suggested_action: Some(&suggested),
                    },
                    &d.summary,
                );
            }
        }
    }

    let mut results = dedup_sort_limit(results, limit);

    // ── Optional: attach full fragment under hit.object ──
    if include_object {
        // M1 fix: reuse `loaded_milestones` (loaded once at function
        // scope above) instead of re-loading. If only non-milestone
        // artifact types are scanned, loaded_milestones is None and
        // we synthesize an empty Vec for attach_objects.
        let milestone_refs: Vec<&MilestoneFile> = loaded_milestones
            .as_ref()
            .map(|v| v.iter().collect())
            .unwrap_or_default();
        let mut fragments = load_non_milestone_fragments(ctx)?;
        attach_objects(&milestone_refs, &mut fragments, &mut results);
    }

    Ok(results)
}

/// Pre-loaded fragments for the non-milestone artifact types. Built once
/// per `attach_objects` call so per-hit lookups in the load_hot loop
/// below stay O(1) rather than re-loading each time.
struct NonMilestoneFragments {
    ideas: Vec<crate::model::IdeaEntry>,
    backlog_items: Vec<crate::model::BacklogItem>,
    tracks: std::collections::HashMap<String, Vec<crate::model::TrackItem>>,
    decisions: Vec<crate::model::DecisionEntry>,
}

fn load_non_milestone_fragments(ctx: &PlanContext) -> Result<NonMilestoneFragments> {
    let ideas = store::load_ideas(ctx).map(|f| f.ideas).unwrap_or_default();
    let backlog_items = store::load_backlog(ctx)
        .map(|f| f.items)
        .unwrap_or_default();
    let mut tracks: std::collections::HashMap<String, Vec<crate::model::TrackItem>> =
        std::collections::HashMap::new();
    for &tk in &track_kind::TrackKind::ALL {
        if let Ok(t) = store::load_track(ctx, tk.as_str()) {
            tracks.insert(tk.as_str().to_string(), t.items);
        }
    }
    let decisions = store::load_decisions(ctx)
        .map(|f| f.decisions)
        .unwrap_or_default();
    Ok(NonMilestoneFragments {
        ideas,
        backlog_items,
        tracks,
        decisions,
    })
}

/// For each hit, attach the full matched fragment under `hit.object`.
/// Lets agents skip the round-trip to `mp show` / `mp milestone ac show`.
///
/// M1 remediation: milestones and non-milestone fragments are passed in
/// by the caller (already loaded for the scan above) so this loop stays
/// load-free; the prior implementation re-loaded the entire plan.
///
/// L2 remediation: milestone lookups go through a `HashMap` keyed by
/// the normalized id, so per-hit resolution is O(1) instead of O(n) over
/// the milestone list. For plans with N milestones and H hits this
/// drops attach_objects from O(N·H) to O(N + H).
fn attach_objects(
    milestones: &[&MilestoneFile],
    fragments: &mut NonMilestoneFragments,
    results: &mut [SearchResult],
) {
    let by_id: HashMap<String, &MilestoneFile> = milestones
        .iter()
        .map(|m| (crate::paths::normalize_milestone_id(&m.milestone.id), *m))
        .collect();
    for r in results.iter_mut() {
        r.object = match r.artifact_type.as_str() {
            "milestone" => {
                let id = r
                    .parent_milestone_id
                    .as_deref()
                    .or(Some(r.id.as_str()))
                    .unwrap_or("");
                let id = id.trim_start_matches('M');
                by_id.get(id).map(|m| to_json_value(*m))
            }
            "acceptance_criterion" => {
                let (mid_raw, ac_id) = split_parented_id(&r.id);
                by_id.get(&mid_raw).and_then(|m| {
                    m.acceptance_criteria
                        .iter()
                        .find(|ac| ac.id == ac_id)
                        .map(to_json_value)
                })
            }
            "step" => {
                let (mid_raw, step_id) = split_parented_id(&r.id);
                by_id
                    .get(&mid_raw)
                    .and_then(|m| m.steps.iter().find(|s| s.id == step_id).map(to_json_value))
            }
            "work_package" => {
                let (mid_raw, wp_id) = split_parented_id(&r.id);
                by_id.get(&mid_raw).and_then(|m| {
                    m.work_packages
                        .iter()
                        .find(|w| w.id == wp_id)
                        .map(to_json_value)
                })
            }
            "idea" => fragments
                .ideas
                .iter()
                .find(|i| i.id == r.id)
                .map(to_json_value),
            "backlog" => fragments
                .backlog_items
                .iter()
                .find(|i| i.id == r.id)
                .map(to_json_value),
            "track" => {
                // r.id is "<prefix>-<track_id>" — split to kind + id.
                let (kind, tid) = split_track_id(&r.id);
                fragments
                    .tracks
                    .get(&kind)
                    .and_then(|items| items.iter().find(|i| i.id == tid))
                    .map(to_json_value)
            }
            "decision" => fragments
                .decisions
                .iter()
                .find(|d| d.id == r.id)
                .map(to_json_value),
            _ => None,
        };
    }
}

/// Split a track hit id of the form "<prefix>-<track_id>" into the
/// (`kind`-name, id-in-kind) pair. Returns the original id unchanged
/// when there is no `-` separator.
fn split_track_id(id: &str) -> (String, String) {
    match id.split_once('-') {
        Some((p, rest)) => {
            let kind = track_kind::TrackKind::ALL
                .iter()
                .find(|tk| tk.prefix() == p)
                .map(|tk| tk.as_str().to_string())
                .unwrap_or_default();
            (kind, rest.to_string())
        }
        None => (String::new(), id.to_string()),
    }
}

/// Split a hit id of the form "M94/AC-01" or "M94/WP1" into
/// (milestone_id_normalized, fragment_id). For non-parented ids (idea,
/// backlog, track, decision) — which `attach_objects` never feeds in —
/// returns `("", original_id)`. The empty-parent convention is what
/// differentiates top-level artifacts from milestone-nested fragments in
/// downstream consumers (e.g. `r.parent_milestone_id.is_some()` checks).
fn split_parented_id(id: &str) -> (String, String) {
    if let Some((parent, frag)) = id.split_once('/') {
        let normalized = parent.trim_start_matches('M').to_string();
        (normalized, frag.to_string())
    } else {
        (String::new(), id.to_string())
    }
}

/// Serialize a value to `serde_json::Value`. Used uniformly by
/// `attach_objects` (for `--include object`) and `group_by_milestone`
/// (for embedded hit rows). Panics on failure because the inputs are
/// in-domain structs owned by this crate — a serialize failure here
/// is a programming bug, not a runtime condition.
fn to_json_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("in-domain fragment must serialize")
}

/// Group hits by their parent milestone. For `artifact_type=milestone`,
/// the group key is the hit's own id (milestones group under themselves).
/// For nested types (`acceptance_criterion`, `step`, `work_package`) the
/// key is `parent_milestone_id`. Other types (idea, backlog, track,
/// decision) fall under a synthetic "(none)" group, ordered LAST so the
/// milestone groups appear first in their natural order.
pub fn group_by_milestone(results: Vec<SearchResult>) -> Value {
    let mut milestone_groups: std::collections::BTreeMap<String, Vec<&SearchResult>> =
        std::collections::BTreeMap::new();
    let mut none_group: Vec<&SearchResult> = Vec::new();
    for r in &results {
        if r.artifact_type == "milestone" {
            milestone_groups.entry(r.id.clone()).or_default().push(r);
        } else if let Some(parent) = r.parent_milestone_id.clone() {
            milestone_groups.entry(parent).or_default().push(r);
        } else {
            none_group.push(r);
        }
    }
    // BTreeMap sorts keys lexicographically ("M10" before "M2"). Sort
    // the emitted groups by numeric milestone id so output order is
    // chronological regardless of id width. Unparseable ids fall back
    // to a high sort key and keep relative lexical order.
    let mut groups_json: Vec<Value> = milestone_groups
        .into_iter()
        .map(|(milestone, hits)| {
            json!({
                "milestone": milestone,
                "hits": hits.into_iter().map(to_json_value).collect::<Vec<_>>(),
            })
        })
        .collect();
    groups_json.sort_by_key(|g| {
        g.get("milestone")
            .and_then(|v| v.as_str())
            .and_then(|s| s.trim_start_matches('M').parse::<u64>().ok())
            .unwrap_or(u64::MAX)
    });
    if !none_group.is_empty() {
        groups_json.push(json!({
            "milestone": "(none)",
            "hits": none_group.into_iter().map(to_json_value).collect::<Vec<_>>(),
        }));
    }
    json!({ "groups": groups_json })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_parented_id_handles_qualified_and_bare() {
        assert_eq!(
            split_parented_id("M94/AC-01"),
            ("94".to_string(), "AC-01".to_string())
        );
        assert_eq!(
            split_parented_id("M01/WP1"),
            ("01".to_string(), "WP1".to_string())
        );
        assert_eq!(
            split_parented_id("M01/S1"),
            ("01".to_string(), "S1".to_string())
        );
        assert_eq!(
            split_parented_id("ID-17"),
            ("".to_string(), "ID-17".to_string())
        );
    }

    #[test]
    fn fuzzy_match_substring_scores_high() {
        let (score, _) = fuzzy_match("install", "Install via cargo").unwrap();
        assert!(score > 0.8, "substring match should be ≥ 0.85, got {score}");
    }

    #[test]
    fn fuzzy_match_sequential_chars_lower() {
        // 5 of 5 chars matched with a 1-char gap; clamp pushes to 0.85.
        let (score, _) = fuzzy_match("istal", "Install").unwrap();
        assert!(
            score >= 0.4,
            "sequential match should be ≥ 0.4, got {score}"
        );
    }

    #[test]
    fn fuzzy_match_no_match_returns_none() {
        assert!(fuzzy_match("xyz", "abc").is_none());
    }

    #[test]
    fn fuzzy_match_tier2_catches_initials_in_order() {
        // Tier 2 fires whenever any single query char is found in
        // sequence in the text (matches > 0). Initials of "mp" appear
        // in order in "Markdown Preview renderer", so Tier 2 returns
        // a hit before any narrower match could. Pin this so a future
        // refactor that tightens Tier 2's coverage gate is forced to
        // think about what falls through (today: nothing useful).
        let result = fuzzy_match("mp", "Markdown Preview renderer");
        let (score, _) = result.expect("Tier 2 catches the initials in order");
        assert!(
            score > 0.4,
            "Tier 2 should fire on initials-in-order; got {score}"
        );
    }

    #[test]
    fn fuzzy_match_query_longer_than_text_returns_none() {
        // Edge case: query has more chars than text. None of the tiers
        // can return a match for this input.
        let result = fuzzy_match("longer than text", "ab");
        // A substring match on "longer than text" in "ab" can't exist;
        // the char-sequential pass will run out of input text after
        // 'a' or 'b' and break out. The function returns None.
        assert!(result.is_none(), "expected None, got {result:?}");
    }
}
