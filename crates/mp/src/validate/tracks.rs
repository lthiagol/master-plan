use crate::model::{AnnotationFile, AnnotationItem, MilestoneFile, TrackItem};
use crate::paths::PlanContext;
use crate::store;

use super::report::issue;
use super::report::ValidationIssue;

const ANNOTATION_ALLOWED_KINDS: &[&str] = &[
    "review-request",
    "break-down",
    "decouple",
    "change-suggestion",
    "approval-request",
    "note",
];

const ANNOTATION_ALLOWED_STATUSES: &[&str] = &["open", "addressed", "resolved"];

pub(crate) fn validate_track_item(item: &TrackItem, kind: &str, errors: &mut Vec<ValidationIssue>) {
    if item.title.is_empty() {
        errors.push(issue(
            "T1",
            &format!("{kind} {} missing title", item.id),
            None,
        ));
    }
    if item.problem.is_empty() {
        errors.push(issue(
            "T1",
            &format!("{kind} {} missing problem", item.id),
            None,
        ));
    }
    if item.done_when.is_empty() && item.verification.is_empty() {
        errors.push(issue(
            "T1",
            &format!("{kind} {} needs done_when or verification", item.id),
            None,
        ));
    }
    if item.steps.is_empty() && item.status == "in-progress" {
        errors.push(issue(
            "T2",
            &format!(
                "{kind} {} needs at least one step before in-progress",
                item.id
            ),
            None,
        ));
    }
}

pub(crate) fn validate_annotations(
    annotations: &AnnotationFile,
    errors: &mut Vec<ValidationIssue>,
) {
    for item in &annotations.annotations {
        validate_annotation_item(item, errors);
    }
}

fn validate_annotation_item(item: &AnnotationItem, errors: &mut Vec<ValidationIssue>) {
    if item.target.is_empty() {
        errors.push(issue(
            "R1",
            &format!("annotation {} has empty target", item.id),
            None,
        ));
    }
    if item.body.is_empty() {
        errors.push(issue(
            "R1",
            &format!("annotation {} has empty body", item.id),
            None,
        ));
    }
    if !ANNOTATION_ALLOWED_KINDS.contains(&item.kind.as_str()) {
        errors.push(issue(
            "R1",
            &format!(
                "annotation {} has invalid kind \"{}\" (expected: {})",
                item.id,
                item.kind,
                ANNOTATION_ALLOWED_KINDS.join(", ")
            ),
            None,
        ));
    }
    if !ANNOTATION_ALLOWED_STATUSES.contains(&item.status.as_str()) {
        errors.push(issue(
            "R1",
            &format!(
                "annotation {} has invalid status \"{}\" (expected: {})",
                item.id,
                item.status,
                ANNOTATION_ALLOWED_STATUSES.join(", ")
            ),
            None,
        ));
    }
    if item.author.is_empty() {
        errors.push(issue(
            "R1",
            &format!("annotation {} has empty author", item.id),
            None,
        ));
    }
}

pub(crate) fn validate_track_drift(
    ctx: &PlanContext,
    m: &MilestoneFile,
    warnings: &mut Vec<ValidationIssue>,
) {
    for step in &m.steps {
        if step.status != "done" {
            continue;
        }
        let refs = extract_track_refs(&step.action);
        for track_ref in &refs {
            if let Some((kind, id)) = parse_track_ref(track_ref) {
                if let Ok(track) = store::load_track(ctx, &kind) {
                    if let Some(item) = track.items.iter().find(|i| i.id == id) {
                        if item.status != "done" && item.status != "archived" {
                            warnings.push(issue(
                                "W30",
                                &format!(
                                    "step {} (done) references {}-{} which is still \"{}\"",
                                    step.id,
                                    track_prefix_for_display(&kind),
                                    id,
                                    item.status
                                ),
                                Some(m.milestone.id.clone()),
                            ));
                        }
                    }
                }
            }
        }
    }
}

fn extract_track_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let prefix = match c {
            'T' if chars.peek() == Some(&'W') => {
                let _ = chars.next();
                "TW"
            }
            'B' if chars.peek() == Some(&'F') => {
                let _ = chars.next();
                "BF"
            }
            _ => continue,
        };
        if chars.peek() != Some(&'-') {
            continue;
        }
        let _ = chars.next();
        let mut num = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() {
                num.push(chars.next().unwrap());
            } else {
                break;
            }
        }
        if !num.is_empty() {
            refs.push(format!("{}-{}", prefix, num));
        }
    }
    refs
}

fn parse_track_ref(track_ref: &str) -> Option<(String, String)> {
    let (prefix, num) = track_ref.split_once('-')?;
    if num.is_empty() {
        return None;
    }
    let kind = match prefix {
        "TW" => "tweak",
        "BF" => "bugfix",
        _ => return None,
    };
    Some((kind.to_string(), track_ref.to_string()))
}

fn track_prefix_for_display(kind: &str) -> &'static str {
    match kind {
        "tweak" => "TW",
        "bugfix" => "BF",
        _ => "??",
    }
}
