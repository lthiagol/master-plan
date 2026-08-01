use anyhow::{bail, Context, Result};

use crate::assets;
use crate::model::{DomainMeta, DomainSpecFile};
use crate::paths::{self, PlanContext};
use crate::store;

#[derive(Debug, serde::Serialize)]
pub struct DomainSummary {
    pub id: String,
    pub title: String,
    pub version: u32,
    pub updated: String,
    pub requirement_count: usize,
}

pub fn list_domains(ctx: &PlanContext) -> Result<Vec<DomainSummary>> {
    let mut out = Vec::new();
    for path in store::list_domain_spec_paths(ctx)? {
        let spec = store::load_domain_spec(
            ctx,
            path.file_stem()
                .and_then(|s| s.to_str())
                .context("invalid domain spec path")?,
        )?;
        out.push(DomainSummary {
            id: spec.domain.id.clone(),
            title: spec.domain.title.clone(),
            version: spec.domain.version,
            updated: spec.domain.updated.clone(),
            requirement_count: spec.requirements.len(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

pub fn show_domain(ctx: &PlanContext, domain: &str) -> Result<DomainSpecFile> {
    paths::assert_domain_id(domain)?;
    store::load_domain_spec(ctx, domain)
}

pub fn init_domain(ctx: &PlanContext, domain: &str, title: Option<&str>) -> Result<DomainSpecFile> {
    paths::assert_domain_id(domain)?;

    let path = ctx.domain_spec_path(domain);
    if path.exists() {
        bail!("domain spec already exists: {}", path.display());
    }

    let title = title.unwrap_or(domain);
    let mut spec: DomainSpecFile = serde_json::from_str(&assets::read_embedded(
        "templates/defaults/spec-domain.json",
    )?)?;
    spec.domain = DomainMeta {
        id: domain.to_string(),
        title: title.to_string(),
        version: 1,
        updated: store::today(),
        summary: format!("Domain spec for {title}."),
    };
    spec.requirements.clear();
    spec.scenarios.clear();
    spec.interface.endpoints.clear();
    spec.interface.config_keys.clear();
    spec.interface.cli_commands.clear();
    spec.interface.entities.clear();
    store::write_domain_spec(ctx, &spec)?;
    Ok(spec)
}
