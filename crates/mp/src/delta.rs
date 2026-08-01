use anyhow::{bail, Context, Result};

use crate::model::{DeltaAdded, DomainRequirement, DomainSpecFile, MilestoneFile};
use crate::paths::PlanContext;
use crate::store;
use crate::validate::{issue, ValidationIssue};

pub fn validate_delta_milestone(ctx: &PlanContext, m: &MilestoneFile) -> Vec<ValidationIssue> {
    if !m.is_delta_kind() {
        return vec![];
    }
    let id = m.milestone.id.clone();
    let mut errors = Vec::new();

    if m.delta.domain.is_empty() {
        errors.push(issue(
            "G11",
            "delta.domain is required when change_kind is delta",
            Some(id.clone()),
        ));
        return errors;
    }

    let domain_path = ctx.domain_spec_path(&m.delta.domain);
    if !domain_path.exists() {
        errors.push(issue(
            "G11",
            &format!("domain spec specs/{}.json does not exist", m.delta.domain),
            Some(id.clone()),
        ));
        return errors;
    }

    let spec = match store::load_domain_spec(ctx, &m.delta.domain) {
        Ok(s) => s,
        Err(e) => {
            errors.push(issue(
                "G11",
                &format!("failed to load domain spec: {e}"),
                Some(id.clone()),
            ));
            return errors;
        }
    };

    if m.delta.base_version == 0 {
        errors.push(issue(
            "G11",
            "delta.base_version must be set",
            Some(id.clone()),
        ));
    }

    if spec.domain.version != m.delta.base_version {
        errors.push(issue(
            "G13",
            &format!(
                "domain version {} does not match delta.base_version {} (rebase required)",
                spec.domain.version, m.delta.base_version
            ),
            Some(id.clone()),
        ));
    }

    let req_ids: std::collections::HashSet<&str> =
        spec.requirements.iter().map(|r| r.id.as_str()).collect();

    for modified in &m.delta.modified {
        if !req_ids.contains(modified.target.as_str()) {
            errors.push(issue(
                "G12",
                &format!(
                    "delta modified target {} not found in domain {}",
                    modified.target, m.delta.domain
                ),
                Some(id.clone()),
            ));
        }
    }
    for removed in &m.delta.removed {
        if !req_ids.contains(removed.target.as_str()) {
            errors.push(issue(
                "G12",
                &format!(
                    "delta removed target {} not found in domain {}",
                    removed.target, m.delta.domain
                ),
                Some(id.clone()),
            ));
        }
    }

    errors
}

pub fn merge_delta_into_domain(ctx: &PlanContext, m: &MilestoneFile) -> Result<DomainSpecFile> {
    if !m.is_delta_kind() {
        bail!("milestone is not a delta");
    }

    let gate_errors = validate_delta_milestone(ctx, m);
    if !gate_errors.is_empty() {
        let msg = gate_errors
            .iter()
            .map(|e| format!("{}: {}", e.code, e.message))
            .collect::<Vec<_>>()
            .join("; ");
        bail!(msg);
    }

    let mut spec = store::load_domain_spec(ctx, &m.delta.domain)?;

    for removed in &m.delta.removed {
        spec.requirements.retain(|r| r.id != removed.target);
    }

    for modified in &m.delta.modified {
        let req = spec
            .requirements
            .iter_mut()
            .find(|r| r.id == modified.target)
            .with_context(|| format!("modified target {} missing", modified.target))?;
        req.statement = modified.after.clone();
    }

    for added in &m.delta.added {
        let mut item = added.clone();
        if item.id.is_empty() {
            item.id = next_req_id(&spec.requirements);
        }
        if spec.requirements.iter().any(|r| r.id == item.id) {
            bail!("duplicate requirement id {}", item.id);
        }
        spec.requirements.push(domain_requirement_from_added(&item));
    }

    spec.domain.version = m.delta.base_version.saturating_add(1);
    spec.domain.updated = store::today();
    store::write_domain_spec(ctx, &spec)?;
    Ok(spec)
}

pub fn delta_rebase(ctx: &PlanContext, milestone_id: &str) -> Result<MilestoneFile> {
    let path = crate::milestone::load_milestone_path(ctx, milestone_id)?;
    let mut m = store::load_milestone(&path)?;
    if !m.is_delta_kind() {
        anyhow::bail!("milestone is not a delta");
    }
    if m.delta.domain.is_empty() {
        anyhow::bail!("delta.domain is required");
    }
    let spec = store::load_domain_spec(ctx, &m.delta.domain)?;
    m.delta.base_version = spec.domain.version;
    m.milestone.updated = store::today();
    crate::milestone::write_milestone_synced(ctx, &path, &m)?;
    Ok(m)
}

pub fn delta_rebase_report(ctx: &PlanContext, milestone_id: &str) -> Result<serde_json::Value> {
    let m = delta_rebase(ctx, milestone_id)?;
    Ok(serde_json::json!({
        "ok": true,
        "milestone_id": m.milestone.id,
        "domain": m.delta.domain,
        "base_version": m.delta.base_version,
    }))
}

fn domain_requirement_from_added(added: &DeltaAdded) -> DomainRequirement {
    DomainRequirement {
        id: added.id.clone(),
        statement: added.statement.clone(),
        scenarios: added.scenarios.clone(),
    }
}

fn next_req_id(requirements: &[DomainRequirement]) -> String {
    let max = requirements
        .iter()
        .filter_map(|r| r.id.strip_prefix("REQ-"))
        .filter_map(|s| s.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("REQ-{:02}", max + 1)
}
