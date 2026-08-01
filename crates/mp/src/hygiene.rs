use chrono::{Duration, NaiveDate};
use serde::Serialize;

use crate::paths::{self, PlanContext};
use crate::store;
use crate::track_kind;

#[derive(Debug, Serialize)]
pub struct HygieneReport {
    pub stale_days: u32,
    pub findings: Vec<HygieneFinding>,
}

#[derive(Debug, Serialize)]
pub struct HygieneFinding {
    pub kind: String,
    pub id: String,
    pub reason: String,
    pub suggested_action: String,
}

pub fn run_hygiene(ctx: &PlanContext, stale_days: u32) -> anyhow::Result<HygieneReport> {
    let cutoff = store::today();
    let cutoff_date = NaiveDate::parse_from_str(&cutoff, "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::Utc::now().date_naive())
        - Duration::days(stale_days as i64);

    let mut findings = Vec::new();

    if let Ok(ideas) = store::load_ideas(ctx) {
        for idea in &ideas.ideas {
            if idea.status != "open" {
                continue;
            }
            if let Ok(created) = NaiveDate::parse_from_str(&idea.created, "%Y-%m-%d") {
                if created < cutoff_date {
                    findings.push(HygieneFinding {
                        kind: "idea".to_string(),
                        id: idea.id.clone(),
                        reason: format!("open for more than {stale_days} days"),
                        suggested_action: format!("mp idea dismiss {}", idea.id),
                    });
                }
            }
        }
    }

    for (_, m) in store::load_all_milestones(ctx)? {
        let mid = paths::normalize_milestone_id(&m.milestone.id);
        if matches!(m.milestone.spec_status.as_str(), "draft" | "interview") {
            if let Ok(updated) = NaiveDate::parse_from_str(&m.milestone.updated, "%Y-%m-%d") {
                if updated < cutoff_date {
                    findings.push(HygieneFinding {
                        kind: "milestone".to_string(),
                        id: mid.clone(),
                        reason: format!(
                            "spec_status {} unchanged for {stale_days}+ days",
                            m.milestone.spec_status
                        ),
                        suggested_action: format!("mp milestone groom {mid}"),
                    });
                }
            }
        }
    }

    if let Ok(backlog) = store::load_backlog(ctx) {
        for item in backlog.items.iter().filter(|i| i.status == "active") {
            if item.suggested_when.is_empty() {
                findings.push(HygieneFinding {
                    kind: "backlog".to_string(),
                    id: item.id.clone(),
                    reason: "active backlog item missing suggested_when".to_string(),
                    suggested_action: format!("mp backlog show {}", item.id),
                });
            }
        }
    }

    for &tk in &track_kind::TrackKind::ALL {
        let kind = tk.as_str();
        if let Ok(track) = store::load_track(ctx, kind) {
            for item in track.items.iter().filter(|i| i.status == "in-progress") {
                if let Ok(created) = NaiveDate::parse_from_str(&item.created, "%Y-%m-%d") {
                    if created < cutoff_date {
                        findings.push(HygieneFinding {
                            kind: "track".to_string(),
                            id: item.id.clone(),
                            reason: format!("in-progress {kind} item older than {stale_days} days"),
                            suggested_action: format!("mp track done {kind} {}", item.id),
                        });
                    }
                }
            }
        }
    }

    Ok(HygieneReport {
        stale_days,
        findings,
    })
}
