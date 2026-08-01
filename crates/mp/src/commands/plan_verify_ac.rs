use std::collections::HashSet;

use anyhow::Result;
use regex::Regex;
use serde::Serialize;

use crate::milestone;
use crate::paths::PlanContext;
use crate::store;

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedAc {
    pub ac_id: String,
    pub verification: String,
    pub status: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crate_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyAcReport {
    pub ok: bool,
    pub milestone_id: String,
    pub ac_count: usize,
    pub unresolvable: usize,
    pub acs: Vec<ResolvedAc>,
}

pub fn verify_ac(ctx: &PlanContext, milestone_id: &str) -> Result<VerifyAcReport> {
    let path = milestone::load_milestone_path(ctx, milestone_id)?;
    let m = store::load_milestone(&path)?;
    let mut acs = Vec::new();
    let mut unresolvable = 0;

    for ac in &m.acceptance_criteria {
        let result = resolve_ac_verification(ctx, ac);
        if result.status == "UNRESOLVABLE" {
            unresolvable += 1;
        }
        acs.push(result);
    }

    let ok = unresolvable == 0;
    Ok(VerifyAcReport {
        ok,
        milestone_id: m.milestone.id,
        ac_count: acs.len(),
        unresolvable,
        acs,
    })
}

fn resolved(ac_id: &str, verification: &str, detail: &str, target: Option<String>) -> ResolvedAc {
    ResolvedAc {
        ac_id: ac_id.to_string(),
        verification: verification.to_string(),
        status: "resolved".to_string(),
        detail: detail.to_string(),
        target,
        symbol: None,
        crate_name: None,
    }
}

fn unresolvable(
    ac_id: &str,
    verification: &str,
    detail: &str,
    symbol: Option<String>,
    crate_name: Option<String>,
) -> ResolvedAc {
    ResolvedAc {
        ac_id: ac_id.to_string(),
        verification: verification.to_string(),
        status: "UNRESOLVABLE".to_string(),
        detail: detail.to_string(),
        target: None,
        symbol,
        crate_name,
    }
}

fn empty_result(ac_id: &str) -> ResolvedAc {
    ResolvedAc {
        ac_id: ac_id.to_string(),
        verification: String::new(),
        status: "EMPTY".to_string(),
        detail: "verification field is empty".to_string(),
        target: None,
        symbol: None,
        crate_name: None,
    }
}

fn manual_result(ac_id: &str, verification: &str) -> ResolvedAc {
    ResolvedAc {
        ac_id: ac_id.to_string(),
        verification: verification.to_string(),
        status: "manual".to_string(),
        detail: "manual verification (always ok)".to_string(),
        target: None,
        symbol: None,
        crate_name: None,
    }
}

fn resolve_ac_verification(
    ctx: &PlanContext,
    ac: &crate::model::AcceptanceCriterion,
) -> ResolvedAc {
    let verification = ac.verification.trim();

    if verification.is_empty() {
        return empty_result(&ac.id);
    }

    if verification.to_ascii_lowercase().starts_with("manual:") {
        return manual_result(&ac.id, verification);
    }

    let lower = verification.to_ascii_lowercase();

    if lower.starts_with("test -f") || lower.starts_with("test -d") {
        return resolve_test_command(ctx, &ac.id, verification);
    }

    if lower.starts_with("cargo ") {
        if let Some(sub) = first_cargo_subcommand(&lower) {
            let sub: &str = sub.as_str();
            if is_runtime_only_cargo_subcommand(sub) {
                return ResolvedAc {
                    ac_id: ac.id.clone(),
                    verification: verification.to_string(),
                    status: "runtime".to_string(),
                    detail: format!(
                        "cargo subcommand `{}` is a runtime-only verification (no static resolution); review-time check: confirm the command runs and exits 0",
                        sub
                    ),
                    target: None,
                    symbol: None,
                    crate_name: None,
                };
            }
            if sub == "test" || sub == "nextest" {
                return resolve_cargo_test(ctx, &ac.id, verification);
            }
            return ResolvedAc {
                ac_id: ac.id.clone(),
                verification: verification.to_string(),
                status: "UNRESOLVABLE".into(),
                detail: format!(
                    "cargo subcommand `{}` is not in the recognized set ({})",
                    sub,
                    RUNTIME_ONLY_CARGO_SUBCOMMANDS.join(", ")
                ),
                target: None,
                symbol: Some(format!("cargo:{}", sub)),
                crate_name: None,
            };
        }
        return ResolvedAc {
            ac_id: ac.id.clone(),
            verification: verification.to_string(),
            status: "UNRESOLVABLE".into(),
            detail: format!("cannot parse cargo subcommand from: {}", verification),
            target: None,
            symbol: None,
            crate_name: None,
        };
    }

    if lower.starts_with("cargo test") || lower.starts_with("cargo nextest") {
        return resolve_cargo_test(ctx, &ac.id, verification);
    }

    if lower.starts_with("make ") {
        return resolve_make_target(ctx, &ac.id, verification);
    }

    if lower.starts_with("bash ") || lower.starts_with("./") || lower.starts_with("sh ") {
        return resolve_script_path(ctx, &ac.id, verification);
    }

    if lower.starts_with("python") {
        return resolve_python_script(ctx, &ac.id, verification);
    }

    if lower.starts_with("grep ")
        || lower.starts_with("rg ")
        || lower.starts_with("awk ")
        || lower.starts_with("for ")
    {
        return ResolvedAc {
            ac_id: ac.id.clone(),
            verification: verification.to_string(),
            status: "inline".to_string(),
            detail: "inline shell command - not a resolvable test target (syntax checked at review time)".to_string(),
            target: None,
            symbol: None,
            crate_name: None,
        };
    }

    ResolvedAc {
        ac_id: ac.id.clone(),
        verification: verification.to_string(),
        status: "unknown".to_string(),
        detail: format!(
            "unrecognized command form; cannot statically resolve: {}",
            verification
        ),
        target: None,
        symbol: None,
        crate_name: None,
    }
}

fn resolve_test_command(ctx: &PlanContext, ac_id: &str, verification: &str) -> ResolvedAc {
    if let Some(path) = extract_test_path(verification) {
        let full_path = ctx.project_root.join(&path);
        if full_path.exists() {
            return resolved(
                ac_id,
                verification,
                &format!("file exists: {}", path),
                Some(full_path.to_string_lossy().to_string()),
            );
        }
        return unresolvable(
            ac_id,
            verification,
            &format!(
                "file not found: {} (project root: {})",
                path,
                ctx.project_root.display()
            ),
            Some(format!("file:{}", path)),
            None,
        );
    }
    ResolvedAc {
        ac_id: ac_id.to_string(),
        verification: verification.to_string(),
        status: "unresolved".to_string(),
        detail: "cannot parse test -f/-d path".to_string(),
        target: None,
        symbol: None,
        crate_name: None,
    }
}

fn extract_test_path(verification: &str) -> Option<String> {
    if let Some(re) = regex_for(r#"test\s+-([fd])\s+("[^"]+"|'[^']+'|\S+)"#) {
        if let Some(caps) = re.captures(verification) {
            let path = caps.get(2)?;
            let s = path.as_str();
            let unquoted = s.trim_matches('\"').trim_matches('\'');
            return Some(unquoted.to_string());
        }
    }
    None
}

fn resolve_cargo_test(ctx: &PlanContext, ac_id: &str, verification: &str) -> ResolvedAc {
    let crate_name = extract_cargo_package(verification);
    let test_target = extract_cargo_test_target(verification);
    let bin_target = extract_cargo_bin_target(verification);
    let is_lib = verification.contains("--lib");

    if verification.contains("--workspace") || verification.contains("--all") {
        return resolved(ac_id, verification, "workspace-wide test - resolved", None);
    }

    let Some(ref crate_name) = crate_name else {
        return ResolvedAc {
            ac_id: ac_id.to_string(),
            verification: verification.to_string(),
            status: "unresolved".to_string(),
            detail: "missing -p <crate> in cargo test invocation".to_string(),
            target: None,
            symbol: None,
            crate_name: None,
        };
    };

    let crate_dir = ctx.project_root.join("crates").join(crate_name);
    if !crate_dir.is_dir() {
        return unresolvable(
            ac_id,
            verification,
            &format!(
                "crate \"{}\" not found at crates/{}/",
                crate_name, crate_name
            ),
            Some(format!("crate:{}", crate_name)),
            Some(crate_name.clone()),
        );
    }

    if let Some(ref target) = test_target {
        let test_file = crate_dir.join("tests").join(format!("{}.rs", target));
        if test_file.is_file() {
            return resolved(
                ac_id,
                verification,
                &format!(
                    "test target \"{}\" found in crate \"{}\"",
                    target, crate_name
                ),
                Some(test_file.to_string_lossy().to_string()),
            );
        }
        return unresolvable(
            ac_id,
            verification,
            &format!(
                "test target \"{}\" not found in crate \"{}\" (expected tests/{}.rs)",
                target, crate_name, target
            ),
            Some(format!("test:{}", target)),
            Some(crate_name.clone()),
        );
    }

    if let Some(ref bin) = bin_target {
        let bin_file = crate_dir.join("src").join(format!("{}.rs", bin));
        if bin_file.exists() {
            return resolved(
                ac_id,
                verification,
                &format!("bin target \"{}\" found in crate \"{}\"", bin, crate_name),
                Some(bin_file.to_string_lossy().to_string()),
            );
        }
        return unresolvable(
            ac_id,
            verification,
            &format!(
                "bin target \"{}\" not found in crate \"{}\"",
                bin, crate_name
            ),
            Some(format!("bin:{}", bin)),
            Some(crate_name.clone()),
        );
    }

    if is_lib {
        return resolved(
            ac_id,
            verification,
            &format!("lib tests in crate \"{}\"", crate_name),
            Some(crate_dir.join("src/lib.rs").to_string_lossy().to_string()),
        );
    }

    // Check for trailing test name filters (after cargo test args, before &&)
    let test_name = extract_trailing_cargo_test_name(verification);
    if let Some(ref name) = test_name {
        return ResolvedAc {
            ac_id: ac_id.to_string(),
            verification: verification.to_string(),
            status: "resolved".to_string(),
            detail: format!(
                "crate \"{}\" found; test name '{}' not statically verifiable",
                crate_name, name
            ),
            target: Some(crate_dir.to_string_lossy().to_string()),
            symbol: None,
            crate_name: Some(crate_name.clone()),
        };
    }

    resolved(
        ac_id,
        verification,
        &format!("crate \"{}\" found", crate_name),
        Some(crate_dir.to_string_lossy().to_string()),
    )
}

fn extract_cargo_package(verification: &str) -> Option<String> {
    regex_for(r"-p\s+(\S+)")
        .and_then(|re| re.captures(verification))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_cargo_test_target(verification: &str) -> Option<String> {
    regex_for(r"--test\s+(\S+)")
        .and_then(|re| re.captures(verification))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_cargo_bin_target(verification: &str) -> Option<String> {
    regex_for(r"--bin\s+(\S+)")
        .and_then(|re| re.captures(verification))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_trailing_cargo_test_name(verification: &str) -> Option<String> {
    // Find the last cargo arg (--test, --lib, --bin, -p, etc.) and
    // check if there are non-flag words after it, before && or end.
    let trimmed = verification.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    // Skip words that start with - (flags) or are cargo/cargo test prefixes
    let non_flag: Vec<&&str> = parts
        .iter()
        .skip_while(|w| w.starts_with("cargo") || w.starts_with("nextest"))
        .skip(1) // skip "test"
        .filter(|w| {
            !w.starts_with('-')
                && !w.starts_with('&')
                && !w.starts_with('|')
                && !w.starts_with(';')
                && !w.starts_with('>')
                && !w.starts_with('<')
                && !w.starts_with('"')
                && !w.starts_with('\'')
                && !w.starts_with("--")
        })
        .collect();
    if non_flag.is_empty() {
        None
    } else {
        Some(non_flag.iter().map(|w| **w).collect::<Vec<_>>().join(" "))
    }
}

fn regex_for(pattern: &str) -> Option<Regex> {
    Regex::new(pattern).ok()
}

/// M121 F-07: canonical cargo subcommands that are runnable but cannot be
/// statically resolved against the codebase. These get `status: runtime`
/// (vs `status: resolved` or `status: UNRESOLVABLE`); the coordinator's
/// review-time check is to confirm the command runs and exits 0.
const RUNTIME_ONLY_CARGO_SUBCOMMANDS: &[&str] =
    &["build", "check", "clippy", "fmt", "bench", "doc", "run"];

fn first_cargo_subcommand(verification_lc: &str) -> Option<String> {
    let trimmed = verification_lc.trim_start();
    let after_cargo = trimmed.strip_prefix("cargo ")?.trim_start();
    let sub: String = after_cargo
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '=')
        .collect();
    if sub.is_empty() {
        None
    } else {
        Some(sub)
    }
}

fn is_runtime_only_cargo_subcommand(sub: &str) -> bool {
    RUNTIME_ONLY_CARGO_SUBCOMMANDS.contains(&sub)
}

fn resolve_make_target(ctx: &PlanContext, ac_id: &str, verification: &str) -> ResolvedAc {
    let target = regex_for(r"^make\s+(\S+)")
        .and_then(|re| re.captures(verification))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string());

    let Some(ref t) = target else {
        return ResolvedAc {
            ac_id: ac_id.to_string(),
            verification: verification.to_string(),
            status: "unresolved".to_string(),
            detail: "cannot parse make target".to_string(),
            target: None,
            symbol: None,
            crate_name: None,
        };
    };

    let known = known_make_targets(ctx);
    if known.contains(t) {
        resolved(
            ac_id,
            verification,
            &format!("make target \"{}\" found", t),
            Some(t.clone()),
        )
    } else {
        unresolvable(
            ac_id,
            verification,
            &format!("make target \"{}\" not found in Makefile", t),
            Some(format!("make:{}", t)),
            None,
        )
    }
}

fn known_make_targets(ctx: &PlanContext) -> HashSet<String> {
    let makefile = ctx.project_root.join("Makefile");
    if let Ok(content) = std::fs::read_to_string(&makefile) {
        let re = Regex::new(r"^([a-zA-Z0-9_.-]+):").ok();
        let mut targets = HashSet::new();
        if let Some(re) = re {
            for line in content.lines() {
                if let Some(caps) = re.captures(line) {
                    if let Some(name) = caps.get(1) {
                        targets.insert(name.as_str().to_string());
                    }
                }
            }
        }
        if !targets.is_empty() {
            return targets;
        }
    }
    // Fallback: known target list for this project.
    [
        "help",
        "build",
        "build-release",
        "check",
        "check-plan-json",
        "test",
        "test-nextest",
        "test-mp-lib",
        "dev-linker",
        "test-scenarios",
        "test-fixtures",
        "verify-lint",
        "adopt-check",
        "dep-audit",
        "dep-audit-raul",
        "design-check",
        "doctor",
        "clean",
        "install",
        "install-global",
        "mp-flow-lint",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn resolve_script_path(ctx: &PlanContext, ac_id: &str, verification: &str) -> ResolvedAc {
    resolve_file_command(ctx, ac_id, verification, &["bash", "sh"], "script")
}

fn resolve_python_script(ctx: &PlanContext, ac_id: &str, verification: &str) -> ResolvedAc {
    resolve_file_command(
        ctx,
        ac_id,
        verification,
        &["python", "python3"],
        "python script",
    )
}

fn resolve_file_command(
    ctx: &PlanContext,
    ac_id: &str,
    verification: &str,
    prefixes: &[&str],
    label: &str,
) -> ResolvedAc {
    let path = extract_command_path(verification, prefixes);
    match path {
        None => ResolvedAc {
            ac_id: ac_id.to_string(),
            verification: verification.to_string(),
            status: "unresolved".to_string(),
            detail: format!("cannot parse {} path", label),
            target: None,
            symbol: None,
            crate_name: None,
        },
        Some(p) => {
            let full_path = ctx.project_root.join(&p);
            if full_path.is_file() {
                resolved(
                    ac_id,
                    verification,
                    &format!("{} found: {}", label, p),
                    Some(full_path.to_string_lossy().to_string()),
                )
            } else {
                unresolvable(
                    ac_id,
                    verification,
                    &format!(
                        "{} not found: {} (project root: {})",
                        label,
                        p,
                        ctx.project_root.display()
                    ),
                    Some(format!("file:{}", p)),
                    None,
                )
            }
        }
    }
}

fn extract_command_path(verification: &str, prefixes: &[&str]) -> Option<String> {
    let trimmed = verification.trim();
    if trimmed.starts_with("./") {
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        return Some(trimmed[..end].to_string());
    }
    for prefix in prefixes {
        if let Some(after) = trimmed.strip_prefix(prefix) {
            let after = after.trim_start();
            if after.is_empty() {
                return None;
            }
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            return Some(after[..end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn plan_ctx(root: &std::path::Path) -> PlanContext {
        PlanContext {
            project_root: root.to_path_buf(),
            plan_dir: root.join("master-plan"),
        }
    }

    fn test_ac(id: &str, verification: &str) -> crate::model::AcceptanceCriterion {
        crate::model::AcceptanceCriterion {
            id: id.to_string(),
            description: String::new(),
            verification: verification.to_string(),
            evidence: String::new(),
            status: "pending".to_string(),
        }
    }

    #[test]
    fn manual_is_always_ok() {
        let ac = test_ac("AC-01", "manual: content review");
        let result = resolve_ac_verification(&plan_ctx(std::path::Path::new(".")), &ac);
        assert_eq!(result.status, "manual");
    }

    #[test]
    fn empty_verification_is_empty() {
        let ac = test_ac("AC-02", "");
        let result = resolve_ac_verification(&plan_ctx(std::path::Path::new(".")), &ac);
        assert_eq!(result.status, "EMPTY");
    }

    #[test]
    fn nonexistent_crate_is_unresolvable() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ac = test_ac("AC-03", "cargo test -p nonexistent_crate --lib some_test");
        let result = resolve_ac_verification(&plan_ctx(tmp.path()), &ac);
        assert_eq!(result.status, "UNRESOLVABLE");
        assert!(result.detail.contains("not found"));
    }

    #[test]
    fn real_crate_mp_is_resolvable() {
        let repo_root = common_repo_root();
        if !repo_root.join("crates/mp").is_dir() {
            return;
        }
        let ac = test_ac("AC-04", "cargo test -p mp");
        let result = resolve_ac_verification(&plan_ctx(&repo_root), &ac);
        assert_eq!(result.status, "resolved");
    }

    #[test]
    fn nonexistent_test_target_is_unresolvable() {
        let repo_root = common_repo_root();
        if !repo_root.join("crates/mp").is_dir() {
            return;
        }
        let ac = test_ac("AC-05", "cargo test -p mp --test nonexistent_test_suite");
        let result = resolve_ac_verification(&plan_ctx(&repo_root), &ac);
        assert_eq!(result.status, "UNRESOLVABLE");
        assert!(result.detail.contains("not found"));
    }

    #[test]
    fn real_test_target_is_resolvable() {
        let repo_root = common_repo_root();
        if !repo_root
            .join("crates/mp/tests/install_skills_v2.rs")
            .is_file()
        {
            return;
        }
        let ac = test_ac("AC-06", "cargo test -p mp --test install_skills_v2");
        let result = resolve_ac_verification(&plan_ctx(&repo_root), &ac);
        assert_eq!(result.status, "resolved");
        assert!(result.detail.contains("found"));
    }

    #[test]
    fn known_make_target_is_resolvable() {
        let ac = test_ac("AC-07", "make test");
        let result = resolve_ac_verification(&plan_ctx(std::path::Path::new(".")), &ac);
        assert_eq!(result.status, "resolved");
    }

    #[test]
    fn unknown_make_target_is_unresolvable() {
        let ac = test_ac("AC-08", "make bogus_target");
        let result = resolve_ac_verification(&plan_ctx(std::path::Path::new(".")), &ac);
        assert_eq!(result.status, "UNRESOLVABLE");
    }

    fn common_repo_root() -> PathBuf {
        if let Ok(home) = std::env::var("MP_HOME") {
            let p = PathBuf::from(home);
            if p.join("templates/skills").is_dir() {
                return p;
            }
        }
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// M121 F-07: cargo clippy / fmt / build / check are runnable
    /// runtime-only commands; they cannot be statically resolved against
    /// the codebase but are valid verification forms.
    #[test]
    fn cargo_clippy_is_runtime() {
        let ac = test_ac("AC-CLIPPY", "cargo clippy -p mp --all-targets exits 0");
        let result = resolve_ac_verification(&plan_ctx(std::path::Path::new(".")), &ac);
        assert_eq!(result.status, "runtime");
        assert!(result.detail.contains("clippy"));
    }

    #[test]
    fn cargo_fmt_is_runtime() {
        let ac = test_ac("AC-FMT", "cargo fmt -- --check exits 0");
        let result = resolve_ac_verification(&plan_ctx(std::path::Path::new(".")), &ac);
        assert_eq!(result.status, "runtime");
        assert!(result.detail.contains("fmt"));
    }

    #[test]
    fn cargo_build_is_runtime() {
        let ac = test_ac("AC-BUILD", "cargo build -p mp");
        let result = resolve_ac_verification(&plan_ctx(std::path::Path::new(".")), &ac);
        assert_eq!(result.status, "runtime");
    }

    #[test]
    fn cargo_unknown_subcommand_is_unresolvable() {
        let ac = test_ac("AC-BOGUS", "cargo nonexistent-subcommand");
        let result = resolve_ac_verification(&plan_ctx(std::path::Path::new(".")), &ac);
        assert_eq!(result.status, "UNRESOLVABLE");
        assert!(result.detail.contains("nonexistent-subcommand"));
    }
}
