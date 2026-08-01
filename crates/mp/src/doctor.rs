use serde::Serialize;
use serde_json::{json, Value};

use crate::brownfield;
use crate::harness;
use crate::paths::PlanContext;
use crate::store;

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub mp_home: String,
    pub templates: bool,
    pub schemas: bool,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectDoctor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<DoctorDetected>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harnesses: Option<Vec<HarnessEntry>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Serialize)]
pub struct HarnessEntry {
    pub id: String,
    pub display_name: String,
    pub skill_installed: bool,
    pub spec_grill_installed: bool,
    pub convention_file_installed: bool,
}

#[derive(Debug, Serialize)]
pub struct DoctorDetected {
    pub brownfield_likely: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectDoctor {
    pub plan_dir: String,
    pub profile: String,
    pub plan_location: String,
    pub plan_in_repo: bool,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

pub fn doctor_toolkit() -> DoctorReport {
    let home = crate::assets::toolkit_home();
    let mut report = doctor_toolkit_at(&home);
    append_runtime_discoverability_checks(&mut report, &crate::install::install_dir());
    report.harnesses = Some(check_harnesses());
    report
}

pub fn doctor_install(
    install_dir: &std::path::Path,
    expected_harness_ids: &[String],
) -> DoctorReport {
    let mut report = doctor_toolkit_at(install_dir);
    let harnesses = check_harnesses();
    let bin_path = install_dir.join("bin/mp");

    // M141 AC-05 / external-review F-03: spec-grill is optional. Doctor
    // only requires the 3 base CPD skills + convention file for the
    // expected harnesses. `spec_grill_installed` remains reported for
    // visibility but does not gate install success.
    let all_ok = expected_harness_ids.iter().all(|id| {
        harnesses
            .iter()
            .any(|h| h.id == *id && h.skill_installed && h.convention_file_installed)
    }) && bin_path.is_file()
        && install_dir.join("bin/raul").is_file();

    report.harnesses = Some(harnesses);

    if !all_ok {
        report.ok = false;
        report.checks.push(DoctorCheck {
            name: "harness_install".to_string(),
            ok: false,
            message: format!(
                "expected harnesses {:?} have missing artifacts",
                expected_harness_ids
            ),
        });
    }
    append_runtime_discoverability_checks(&mut report, install_dir);
    report
}

fn doctor_toolkit_at(home: &std::path::Path) -> DoctorReport {
    let mut ok = true;
    let mut checks = Vec::new();

    let key_files: &[(&str, &str)] = &[
        ("templates/defaults/plan.json", "plan default"),
        ("templates/defaults/track.json", "track default"),
        ("templates/defaults/milestone.json", "milestone default"),
        ("templates/AGENTS-TEMPLATE.md", "AGENTS template"),
        ("schemas/milestone.schema.json", "milestone schema"),
        ("schemas/interview-checklist.json", "interview checklist"),
    ];

    for (rel, label) in key_files {
        let check_name = format!("integrity:{rel}");
        let present = crate::assets::embedded_asset(rel)
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        checks.push(DoctorCheck {
            name: check_name,
            ok: present,
            message: if present {
                format!("embedded {label} present")
            } else {
                format!("embedded {label} missing: {rel}")
            },
        });
        ok = ok && present;
    }

    let cpd_skills = ["mp-flow", "mp-runner", "mp-coordinator"];
    for skill_id in cpd_skills {
        let rel = format!("templates/skills/{skill_id}/SKILL.md");
        let present = crate::assets::embedded_asset(&rel).is_some() || home.join(&rel).is_file();
        checks.push(DoctorCheck {
            name: format!("integrity:cpd:{skill_id}"),
            ok: present,
            message: if present {
                format!("{skill_id} skill available (embedded or on disk)")
            } else {
                format!("{skill_id} skill not found at {rel}")
            },
        });
        ok = ok && present;
    }

    let spec_grill_rel = "templates/skills/spec-grill/SKILL.md";
    let spec_grill_present = crate::assets::embedded_asset(spec_grill_rel).is_some()
        || home.join(spec_grill_rel).is_file();
    checks.push(DoctorCheck {
        name: "integrity:spec-grill".to_string(),
        ok: spec_grill_present,
        message: if spec_grill_present {
            "spec-grill skill available (embedded or on disk)".to_string()
        } else {
            "spec-grill skill not found".to_string()
        },
    });

    DoctorReport {
        ok,
        mp_home: home.to_string_lossy().to_string(),
        templates: true,
        schemas: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        project: None,
        detected: None,
        harnesses: None,
        checks,
    }
}

fn check_harnesses() -> Vec<HarnessEntry> {
    harness::default_registry()
        .into_iter()
        .map(|h| {
            let skill_dir = harness::resolved_global_skill_dir(&h);
            // Post-M141: each skill gets its own subdir under the
            // harness's global_skill_dir. The harness is considered
            // "skill_installed" if the three CPD base skills all
            // landed, and "spec_grill_installed" if spec-grill landed.
            let skill_installed = ["mp-flow", "mp-runner", "mp-coordinator"]
                .iter()
                .all(|s| skill_dir.join(s).join("SKILL.md").is_file());
            let spec_grill_installed = skill_dir.join("spec-grill").join("SKILL.md").is_file();
            let convention_file_installed = harness::convention_path(&h).is_file();
            HarnessEntry {
                id: h.id,
                display_name: h.display_name,
                skill_installed,
                spec_grill_installed,
                convention_file_installed,
            }
        })
        .collect()
}

pub fn doctor_project(ctx: &PlanContext) -> DoctorReport {
    let mut report = doctor_toolkit();
    let mut checks = Vec::new();

    let plan_exists = ctx.plan_dir.is_dir();
    checks.push(DoctorCheck {
        name: "plan_dir".to_string(),
        ok: plan_exists,
        message: if plan_exists {
            "plan directory exists".to_string()
        } else {
            format!("missing plan dir: {}", ctx.plan_dir.display())
        },
    });

    let (cfg, config_warn) = if plan_exists {
        match store::try_load_config(ctx) {
            Ok(c) => (c, None),
            Err(e) => (
                Default::default(),
                Some(format!("config.json invalid or unreadable: {e:#}")),
            ),
        }
    } else {
        (Default::default(), None)
    };

    if let Some(warn) = config_warn {
        checks.push(DoctorCheck {
            name: "config_parse".to_string(),
            ok: true,
            message: format!("warning (W50): {warn}"),
        });
    }

    let profile = cfg.profile().to_string();
    let plan_loc = cfg.plan_location().to_string();
    let expected_plan = ctx.project_root.join(&plan_loc);
    checks.push(DoctorCheck {
        name: "plan_location".to_string(),
        ok: !plan_exists || ctx.plan_dir == expected_plan,
        message: format!(
            "config workflow.plan.location={plan_loc}, resolved={}",
            ctx.plan_dir.display()
        ),
    });

    if cfg.workflow.artifacts.brief.unwrap_or(false) {
        let ok = ctx.brief_path().exists();
        checks.push(DoctorCheck {
            name: "brief_artifact".to_string(),
            ok,
            message: if ok {
                "brief.json present".to_string()
            } else {
                "brief.json missing for profile".to_string()
            },
        });
    }

    if cfg.workflow.artifacts.ideas.unwrap_or(false) {
        let ok = ctx.ideas_path().exists();
        checks.push(DoctorCheck {
            name: "ideas_artifact".to_string(),
            ok,
            message: if ok {
                "ideas.json present".to_string()
            } else {
                "ideas.json missing for profile".to_string()
            },
        });
    }

    if cfg.workflow.artifacts.milestones.is_session() {
        let ok = ctx.sessions_dir().is_dir();
        checks.push(DoctorCheck {
            name: "sessions_dir".to_string(),
            ok,
            message: if ok {
                "sessions/ directory present".to_string()
            } else {
                "sessions/ missing for session milestones profile".to_string()
            },
        });
    }

    let in_repo = cfg.workflow.plan.in_repo.unwrap_or(true);
    if !in_repo {
        let gitignore = ctx.project_root.join(".gitignore");
        let needle = format!("{}/", plan_loc.trim_end_matches('/'));
        let ok = gitignore.exists()
            && std::fs::read_to_string(&gitignore)
                .map(|c| c.lines().any(|l| l.trim() == needle.trim_end_matches('/')))
                .unwrap_or(false);
        checks.push(DoctorCheck {
            name: "gitignore_plan".to_string(),
            ok,
            message: if ok {
                "plan path gitignored".to_string()
            } else {
                format!("add {needle} to .gitignore")
            },
        });
    }

    let project_ok = checks.iter().all(|c| c.ok);
    report.ok = report.ok && project_ok;
    report.detected = Some(DoctorDetected {
        brownfield_likely: brownfield::detect_brownfield_likely(&ctx.project_root),
        stack: brownfield::detect_stack(&ctx.project_root),
    });
    report.project = Some(ProjectDoctor {
        plan_dir: ctx.plan_dir.to_string_lossy().to_string(),
        profile,
        plan_location: plan_loc,
        plan_in_repo: in_repo,
        checks: checks.clone(),
    });
    report.checks.extend(checks);
    report
}

pub(crate) fn command_on_path(name: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return true;
        }
    }
    false
}

pub fn append_runtime_discoverability_checks(
    report: &mut DoctorReport,
    install_dir: &std::path::Path,
) {
    let mp_bin = install_dir.join("bin/mp");
    let raul_bin = install_dir.join("bin/raul");
    let env_sh = crate::install::env_snippet_path(install_dir);

    let mp_on_path = command_on_path("mp");
    let mp_layout_ok = mp_bin.is_file();

    let mut mp_message = if mp_on_path {
        "mp resolves on PATH".to_string()
    } else if mp_layout_ok {
        format!("mp not on PATH; installed at {}", mp_bin.display())
    } else {
        "mp not on PATH and install layout missing bin/mp".to_string()
    };
    if !mp_on_path && env_sh.is_file() {
        mp_message.push_str(&format!(" — source {}", env_sh.display()));
    }

    report.checks.push(DoctorCheck {
        name: "runtime:mp_on_path".to_string(),
        ok: mp_on_path,
        message: mp_message,
    });

    let raul_ok = !mp_layout_ok || raul_bin.is_file();
    report.checks.push(DoctorCheck {
        name: "runtime:raul_binary".to_string(),
        ok: raul_ok,
        message: if raul_bin.is_file() {
            format!("raul binary present at {}", raul_bin.display())
        } else if mp_layout_ok {
            format!("raul missing from install layout ({})", raul_bin.display())
        } else {
            "raul binary check skipped (no install layout)".to_string()
        },
    });

    if env_sh.is_file() {
        report.checks.push(DoctorCheck {
            name: "runtime:env_snippet".to_string(),
            ok: true,
            message: format!(
                "agent env snippet at {} (source in harness shells)",
                env_sh.display()
            ),
        });
    } else if mp_layout_ok {
        report.checks.push(DoctorCheck {
            name: "runtime:env_snippet".to_string(),
            ok: false,
            message: format!("missing env snippet at {}", env_sh.display()),
        });
    }

    // Install ok requires mp on PATH (users must `source env.sh`). Do not
    // skip this for tmp installs — tests must prepend install bin to PATH
    // (see `path_with_install_bin` in crates/mp/tests/common).
    if mp_layout_ok && !mp_on_path {
        report.ok = false;
    }
    if mp_layout_ok && !raul_ok {
        report.ok = false;
    }
    if mp_layout_ok && !env_sh.is_file() {
        report.ok = false;
    }
}

pub fn doctor_json(ctx: &PlanContext, include_project: bool) -> Value {
    if include_project && ctx.plan_dir.is_dir() {
        serde_json::to_value(doctor_project(ctx)).unwrap_or(json!({}))
    } else {
        serde_json::to_value(doctor_toolkit()).unwrap_or(json!({}))
    }
}
