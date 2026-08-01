use std::collections::HashSet;

use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};

use crate::paths::{self, PlanContext};
use crate::plan_gaps;
use crate::store;
use crate::validate::effective_execution_status;

#[derive(Debug, Serialize)]
pub struct GraphReport {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub baseline_order: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GraphNode {
    pub r#type: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criterion_status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GraphEdge {
    pub r#type: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize)]
pub struct GraphExplainReport {
    pub milestone: String,
    pub display: String,
    pub blocked: bool,
    pub waiting_on: Vec<String>,
    pub downstream: Vec<String>,
    pub coverage_gaps: Vec<String>,
    pub reasons: Vec<String>,
}

pub fn build_graph(
    ctx: &PlanContext,
    milestone_filter: Option<&str>,
    with_steps: bool,
    with_ac: bool,
) -> Result<GraphReport> {
    let milestones = store::load_all_milestones(ctx)?;
    let baseline = crate::path_engine::build_path(ctx, 1000)?.baseline_milestone_order;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let filter = milestone_filter.map(paths::normalize_milestone_id);

    for (_, m) in &milestones {
        let mid = paths::normalize_milestone_id(&m.milestone.id);
        if filter.as_ref().is_some_and(|f| f != &mid) {
            continue;
        }
        nodes.push(GraphNode {
            r#type: "milestone".to_string(),
            id: mid.clone(),
            display: Some(format!(
                "{} — {}",
                paths::display_milestone_id(&m.milestone.id),
                m.milestone.title
            )),
            milestone: None,
            status: Some(crate::validate::effective_execution_status(m)),
            criterion_status: None,
        });
        for dep in &m.milestone.depends_on {
            if dep.is_empty() || dep == "none" {
                continue;
            }
            edges.push(GraphEdge {
                r#type: "depends_on".to_string(),
                from: paths::normalize_milestone_id(dep),
                to: mid.clone(),
            });
        }
        if with_steps {
            for step in &m.steps {
                nodes.push(GraphNode {
                    r#type: "step".to_string(),
                    id: step.id.clone(),
                    display: None,
                    milestone: Some(mid.clone()),
                    status: Some(step.status.clone()),
                    criterion_status: None,
                });
                for dep in &step.depends_on_steps {
                    edges.push(GraphEdge {
                        r#type: "depends_on_steps".to_string(),
                        from: format!("{mid}/{dep}"),
                        to: format!("{mid}/{}", step.id),
                    });
                }
            }
        }
        if with_ac {
            let coverage = plan_gaps::coverage_report(m);
            for ac in coverage.acceptance_criteria {
                nodes.push(GraphNode {
                    r#type: "ac".to_string(),
                    id: ac.id.clone(),
                    display: None,
                    milestone: Some(mid.clone()),
                    status: None,
                    criterion_status: Some(ac.status),
                });
                for step_id in ac.covered_by {
                    edges.push(GraphEdge {
                        r#type: "covers".to_string(),
                        from: format!("{mid}/{step_id}"),
                        to: format!("{mid}/{}", ac.id),
                    });
                }
            }
        }
    }

    Ok(GraphReport {
        nodes,
        edges,
        baseline_order: baseline,
    })
}

pub fn graph_explain(ctx: &PlanContext, milestone_id: &str) -> Result<GraphExplainReport> {
    let norm = paths::normalize_milestone_id(milestone_id);
    let milestones = store::load_all_milestones(ctx)?;
    let m = milestones
        .iter()
        .find(|(_, m)| paths::normalize_milestone_id(&m.milestone.id) == norm)
        .map(|(_, m)| m)
        .ok_or_else(|| anyhow::anyhow!("milestone {norm} not found"))?;

    let done_ids: HashSet<String> = milestones
        .iter()
        // M100 ER-8: route through `effective_execution_status` so
        // migrated milestones whose raw field is empty register as done.
        .filter(|(_, m)| effective_execution_status(m) == "done")
        .map(|(_, m)| paths::normalize_milestone_id(&m.milestone.id))
        .collect();

    let waiting_on: Vec<String> = m
        .milestone
        .depends_on
        .iter()
        .filter(|dep| !dep.is_empty() && *dep != "none")
        .map(|dep| paths::normalize_milestone_id(dep))
        .filter(|dep| !done_ids.contains(dep))
        .collect();

    let downstream: Vec<String> = milestones
        .iter()
        .filter(|(_, other)| {
            other
                .milestone
                .depends_on
                .iter()
                .any(|dep| paths::normalize_milestone_id(dep) == norm)
        })
        .map(|(_, other)| paths::normalize_milestone_id(&other.milestone.id))
        .collect();

    let coverage = plan_gaps::coverage_report(m);
    let coverage_gaps: Vec<String> = coverage
        .acceptance_criteria
        .into_iter()
        .filter(|ac| ac.status == "uncovered")
        .map(|ac| ac.id)
        .collect();

    let mut reasons = Vec::new();
    // M100 ER-8: route through `effective_execution_status` so migrated
    // milestones whose raw field is empty correctly register as blocked
    // (via the overlay `m.milestone.blocked`).
    if effective_execution_status(m) == "blocked" {
        reasons.push(format!("blocked: {}", m.milestone.block_reason));
    }
    if !waiting_on.is_empty() {
        reasons.push(format!("waiting on milestones: {}", waiting_on.join(", ")));
    }
    if !coverage_gaps.is_empty() {
        reasons.push(format!("uncovered AC: {}", coverage_gaps.join(", ")));
    }

    Ok(GraphExplainReport {
        milestone: norm.clone(),
        display: format!(
            "{} — {}",
            paths::display_milestone_id(&m.milestone.id),
            m.milestone.title
        ),
        blocked: !waiting_on.is_empty()
            // M100 ER-8: route through `effective_execution_status` so
            // migrated milestones whose raw field is empty register the
            // blocked / deferred overlay via the canonical helper.
            || {
                let e = effective_execution_status(m);
                e == "blocked" || e == "deferred"
            },
        waiting_on,
        downstream,
        coverage_gaps,
        reasons,
    })
}

pub fn graph_dot(report: &GraphReport) -> String {
    let mut out = String::from("digraph plan {\n");
    for node in &report.nodes {
        let label = node.display.as_deref().unwrap_or(&node.id);
        out.push_str(&format!("  \"{}\" [label=\"{}\"];\n", node.id, label));
    }
    for edge in &report.edges {
        let style = if edge.r#type == "depends_on" {
            ""
        } else {
            " [style=dashed]"
        };
        out.push_str(&format!("  \"{}\" -> \"{}\"{style};\n", edge.from, edge.to));
    }
    out.push_str("}\n");
    out
}

pub fn graph_json_value(report: &GraphReport) -> Value {
    serde_json::to_value(report).unwrap_or(json!({}))
}
