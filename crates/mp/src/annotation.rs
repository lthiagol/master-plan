use anyhow::{Context, Result};

use crate::model::AnnotationItem;
use crate::paths::PlanContext;
use crate::store;

const ALLOWED_KINDS: &[&str] = &[
    "review-request",
    "break-down",
    "decouple",
    "change-suggestion",
    "approval-request",
    "note",
];

pub fn annotation_create(
    ctx: &PlanContext,
    target: &str,
    kind: &str,
    body: &str,
    author: &str,
) -> Result<AnnotationItem> {
    if target.is_empty() {
        anyhow::bail!("target is required");
    }
    if !ALLOWED_KINDS.contains(&kind) {
        anyhow::bail!(
            "invalid kind: {kind} (expected one of: {})",
            ALLOWED_KINDS.join(", ")
        );
    }
    if author.is_empty() {
        anyhow::bail!("author is required");
    }
    let mut annotations = store::load_annotations(ctx)?;
    let id = store::next_annotation_id(&annotations);
    let item = AnnotationItem {
        id: id.clone(),
        target: target.to_string(),
        kind: kind.to_string(),
        body: body.to_string(),
        author: author.to_string(),
        status: "open".to_string(),
        created_at: store::today(),
        resolved_at: String::new(),
    };
    annotations.annotations.push(item.clone());
    store::write_annotations(ctx, &annotations)?;
    Ok(item)
}

pub fn annotation_list(
    ctx: &PlanContext,
    open_only: bool,
    target: Option<&str>,
    kind: Option<&str>,
    author: Option<&str>,
) -> Result<Vec<AnnotationItem>> {
    let annotations = store::load_annotations(ctx)?;
    Ok(annotations
        .annotations
        .into_iter()
        .filter(|a| {
            if open_only && a.status != "open" {
                return false;
            }
            if let Some(t) = target {
                if a.target != t {
                    return false;
                }
            }
            if let Some(k) = kind {
                if a.kind != k {
                    return false;
                }
            }
            if let Some(auth) = author {
                if a.author != auth {
                    return false;
                }
            }
            true
        })
        .collect())
}

pub fn annotation_show(ctx: &PlanContext, id: &str) -> Result<AnnotationItem> {
    let annotations = store::load_annotations(ctx)?;
    annotations
        .annotations
        .into_iter()
        .find(|a| a.id == id)
        .with_context(|| format!("annotation {id} not found"))
}

pub fn annotation_update(
    ctx: &PlanContext,
    id: &str,
    body: Option<&str>,
    kind: Option<&str>,
    author: Option<&str>,
) -> Result<AnnotationItem> {
    let mut annotations = store::load_annotations(ctx)?;
    let item = annotations
        .annotations
        .iter_mut()
        .find(|a| a.id == id)
        .with_context(|| format!("annotation {id} not found"))?;

    if item.status != "open" {
        anyhow::bail!(
            "annotation {id} is not open (status: {}); only open annotations can be updated",
            item.status
        );
    }
    if let Some(k) = kind {
        if !ALLOWED_KINDS.contains(&k) {
            anyhow::bail!(
                "invalid kind: {k} (expected one of: {})",
                ALLOWED_KINDS.join(", ")
            );
        }
        item.kind = k.to_string();
    }
    if let Some(b) = body {
        item.body = b.to_string();
    }
    if let Some(a) = author {
        if a.is_empty() {
            anyhow::bail!("author cannot be empty");
        }
        item.author = a.to_string();
    }
    let out = item.clone();
    store::write_annotations(ctx, &annotations)?;
    Ok(out)
}

pub fn annotation_resolve(ctx: &PlanContext, id: &str) -> Result<AnnotationItem> {
    let mut annotations = store::load_annotations(ctx)?;
    let item = annotations
        .annotations
        .iter_mut()
        .find(|a| a.id == id)
        .with_context(|| format!("annotation {id} not found"))?;

    match item.status.as_str() {
        "open" | "addressed" => {
            item.status = "resolved".to_string();
            item.resolved_at = store::today();
        }
        "resolved" => {
            anyhow::bail!("annotation {id} is already resolved");
        }
        other => {
            anyhow::bail!(
                "cannot resolve annotation {id} from status: {other} (expected open or addressed)"
            );
        }
    }
    let out = item.clone();
    store::write_annotations(ctx, &annotations)?;
    Ok(out)
}

pub fn annotation_reopen(ctx: &PlanContext, id: &str) -> Result<AnnotationItem> {
    let mut annotations = store::load_annotations(ctx)?;
    let item = annotations
        .annotations
        .iter_mut()
        .find(|a| a.id == id)
        .with_context(|| format!("annotation {id} not found"))?;

    match item.status.as_str() {
        "resolved" => {
            item.status = "open".to_string();
            item.resolved_at = String::new();
        }
        "addressed" => {
            anyhow::bail!("cannot reopen annotation {id} from addressed status; only resolved annotations can be reopened");
        }
        "open" => {
            anyhow::bail!("annotation {id} is already open");
        }
        other => {
            anyhow::bail!("cannot reopen annotation {id} from status: {other}");
        }
    }
    let out = item.clone();
    store::write_annotations(ctx, &annotations)?;
    Ok(out)
}

pub fn annotation_remove(ctx: &PlanContext, id: &str) -> Result<()> {
    let mut annotations = store::load_annotations(ctx)?;
    let len_before = annotations.annotations.len();
    annotations.annotations.retain(|a| a.id != id);
    if annotations.annotations.len() == len_before {
        anyhow::bail!("annotation {id} not found");
    }
    store::write_annotations(ctx, &annotations)?;
    Ok(())
}

pub fn annotation_mark_addressed(ctx: &PlanContext, id: &str) -> Result<AnnotationItem> {
    let mut annotations = store::load_annotations(ctx)?;
    let item = annotations
        .annotations
        .iter_mut()
        .find(|a| a.id == id)
        .with_context(|| format!("annotation {id} not found"))?;

    match item.status.as_str() {
        "open" => {
            item.status = "addressed".to_string();
        }
        "addressed" => {
            anyhow::bail!("annotation {id} is already addressed");
        }
        "resolved" => {
            anyhow::bail!("annotation {id} is resolved; cannot mark addressed");
        }
        other => {
            anyhow::bail!("cannot mark annotation {id} addressed from status: {other}");
        }
    }
    let out = item.clone();
    store::write_annotations(ctx, &annotations)?;
    Ok(out)
}

pub fn has_open_approval_requests(
    ctx: &PlanContext,
    milestone_id: &str,
) -> Result<Vec<AnnotationItem>> {
    let annotations = store::load_annotations(ctx)?;
    let norm = crate::paths::normalize_milestone_id(milestone_id);
    let prefix = format!("M{}", norm);
    Ok(annotations
        .annotations
        .into_iter()
        .filter(|a| {
            a.kind == "approval-request"
                && a.status != "resolved"
                && (a.target == prefix || a.target.starts_with(&format!("{}/", prefix)))
        })
        .collect())
}
