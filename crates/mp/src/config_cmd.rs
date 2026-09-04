use std::path::Path;

use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::config::{ConfigSchemaReport, ProjectConfig, RoleConfig};
use crate::config_docs;
use crate::paths::PlanContext;
use crate::store;

#[derive(Debug, Serialize)]
pub struct ConfigShowReport {
    pub source: String,
    pub config: ProjectConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigFieldIssue {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigValidateReport {
    pub ok: bool,
    pub errors: Vec<ConfigFieldIssue>,
    pub warnings: Vec<ConfigFieldIssue>,
}

/// Report emitted by both `config set` and `config set --dry-run`. The
/// real-set path uses the same shape as the dry-run path so downstream
/// consumers (notably the raul Settings modal — M140) get the same JSON
/// contract regardless of which mode ran. `agent_*_command` is redacted
/// when the request failed so a half-validated command string never leaks
/// to the caller through stdout (low-impact — the agent passes the value
/// in — but explicit redaction is cheaper than auditing the contract).
#[derive(Debug, Serialize)]
pub struct ConfigSetReport {
    pub ok: bool,
    pub dry_run: bool,
    pub key: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ProjectConfig>,
    pub errors: Vec<ConfigFieldIssue>,
    /// M156 ext-review F-13: warnings emitted by the same semantic
    /// gate that drives `errors`. Surfaces the empty-workflow.profile
    /// signal and any future non-blocking semantic hints; mirrors the
    /// `ConfigValidateReport.warnings` shape so consumers can reason
    /// about both commands uniformly.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ConfigFieldIssue>,
}

const FIELD_CONFIG: &str = "config";

fn load_error_issue(e: &anyhow::Error) -> ConfigFieldIssue {
    ConfigFieldIssue {
        field: FIELD_CONFIG.to_string(),
        message: format!("failed to load config: {e:#}"),
    }
}

pub fn config_show(ctx: &PlanContext) -> ConfigShowReport {
    let role = read_session_role(ctx);
    ConfigShowReport {
        source: "project".to_string(),
        config: store::load_config(ctx),
        role,
    }
}

/// M201: emit the typed config schema. The shape is stable and
/// additive: `{ "$schema_version": "1.0", "keys": [ {key, type, default, allowed?, description}, ... ] }`.
/// Keys are sorted by key. `default` reflects `ProjectConfig::default()`
/// for non-keybinds and `KEYBIND_DEFAULTS` for keybinds. `ctx` is
/// accepted for symmetry with `config_show` — the schema is independent
/// of the project state, so loading the config is unnecessary.
pub fn config_schema(_ctx: &PlanContext) -> ConfigSchemaReport {
    config_docs::build_schema_report()
}

fn read_session_role(ctx: &PlanContext) -> Option<String> {
    let session_path = ctx.plan_dir.join(".mp").join("session.json");
    let content = store::read_text_bounded(&session_path, store::MAX_PLAN_FILE_BYTES).ok()?;
    let session: serde_json::Value = serde_json::from_str(&content).ok()?;
    session
        .get("role")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn config_get(ctx: &PlanContext, key: &str) -> Result<Value> {
    let cfg = store::load_config(ctx);
    if let Some(action) = key.strip_prefix("keybinds.") {
        return config_get_keybind(&cfg, action);
    }
    if let Some(rest) = key.strip_prefix("agent.") {
        // M147: agent.automation.* is its own namespace — dispatch BEFORE
        // `config_get_agent`, which expects agent.<role>.<field>.
        if let Some(field) = rest.strip_prefix("automation.") {
            return config_get_automation(&cfg, field);
        }
        return config_get_agent(&cfg, rest);
    }
    // M209: autopilot section. Same dotted-key shape as agent.*.
    if let Some(rest) = key.strip_prefix("autopilot.") {
        return config_get_autopilot(&cfg, rest);
    }
    // M154: review-side integrations — same dotted-key surface as
    // agent.automation.*. The accessor methods own the defaults so
    // `mp config get review.hunk` returns the effective value (not
    // null) even when the section is absent.
    if let Some(rest) = key.strip_prefix("review.") {
        return match rest {
            "hunk" => Ok(json!(cfg.review_hunk_enabled())),
            "hunk_author" => Ok(json!(cfg.review_hunk_author())),
            other => bail!("unknown review field: review.{other} (expected hunk|hunk_author)"),
        };
    }
    if let Some(rest) = key.strip_prefix("sort.") {
        // M182 S4: read the per-lane sort key (empty string when
        // the lane has no explicit preference — raul treats that as
        // "fall back to SortKey::Id"). The expected key shape is
        // `sort.<lane>`; we ignore any extra `<key>` suffix because
        // the `get` query is lane-level.
        let lane = rest.split_once('.').map(|(l, _)| l).unwrap_or(rest);
        return Ok(cfg
            .sort
            .get(lane)
            .map(|s| json!(s.clone()))
            .unwrap_or_else(|| json!("")));
    }
    // M204: read per-lane filter selections as a JSON object
    // `{dimension: [values...]}`. Empty / unset lane returns `{}`.
    if let Some(rest) = key.strip_prefix("filter.") {
        let lane = rest.split_once('.').map(|(l, _)| l).unwrap_or(rest);
        return Ok(cfg
            .filter
            .get(lane)
            .map(|dims| {
                let obj: serde_json::Map<String, Value> = dims
                    .iter()
                    .map(|(d, vs)| {
                        (
                            d.clone(),
                            Value::Array(vs.iter().cloned().map(Value::String).collect()),
                        )
                    })
                    .collect();
                Value::Object(obj)
            })
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())));
    }
    match key {
        "workflow.profile" => Ok(json!(cfg.workflow.profile)),
        "workflow.plan.location" => Ok(json!(cfg.plan_location())),
        "workflow.plan.in_repo" => Ok(json!(cfg.workflow.plan.in_repo.unwrap_or(true))),
        "workflow.gates.strictness" => Ok(json!(cfg.strictness())),
        "workflow.steps.code_review" => Ok(json!(cfg.code_review_enabled())),
        "git.auto_commit" => Ok(json!(cfg.git.auto_commit.unwrap_or(false))),
        "git.commit_on_milestone_complete" => {
            Ok(json!(cfg.git.commit_on_milestone_complete.unwrap_or(false)))
        }
        "git.auto_push" => Ok(json!(cfg.git.auto_push.unwrap_or(false))),
        "next.prefer" => Ok(json!(cfg.next_prefer())),
        "ui.color" => Ok(json!(cfg.ui.color.unwrap_or(true))),
        "ui.icons" => Ok(json!(cfg.ui.icons.as_deref().unwrap_or("unicode"))),
        "ui.theme" => Ok(json!(cfg.ui.theme.as_deref().unwrap_or("mocha"))),
        "ui.hide_done" => Ok(json!(cfg.ui.hide_done.unwrap_or(false))),
        // M198 WP1 / M229: the autopilot TUI tab is hidden by
        // default. Operators opt in via
        // `mp config set ui.show_autopilot_tab true`. The legacy
        // `ui.show_watch_tab` key was removed by M229's
        // breaking-release cleanup.
        "ui.show_autopilot_tab" => Ok(json!(cfg.ui.show_autopilot_tab.unwrap_or(false))),
        _ => bail!("unknown config key: {key}"),
    }
}

/// Apply a single key/value mutation in memory (no write). Shared by
/// `config set` and `config set --dry-run`.
fn apply_config_set(cfg: &mut ProjectConfig, key: &str, value: &str) -> Result<()> {
    if let Some(action) = key.strip_prefix("keybinds.") {
        return set_keybind(cfg, action, value);
    }
    if let Some(rest) = key.strip_prefix("agent.") {
        // M147: agent.automation.<field> dispatches BEFORE the
        // agent.<role>.<field> path because `automation` is not a
        // valid role name in M149's dispatch.
        if let Some(field) = rest.strip_prefix("automation.") {
            return set_automation_field(cfg, field, value);
        }
        return set_agent_field(cfg, rest, value);
    }
    // M154: review.<field> applies the same dotted-key shape as
    // agent.automation.<field>. `hunk` is a bool (validated via
    // parse_bool); `hunk_author` is a free-form string with a
    // non-empty guard so a typo can't accidentally clear it.
    if let Some(rest) = key.strip_prefix("review.") {
        match rest {
            "hunk" => cfg.review.hunk = parse_bool(value)?,
            "hunk_author" => {
                if value.is_empty() {
                    bail!("review.hunk_author cannot be empty (use 'mp' for the default)");
                }
                cfg.review.hunk_author = value.to_string();
            }
            other => bail!("unknown review field: review.{other} (expected hunk|hunk_author)"),
        }
        return Ok(());
    }
    // M209: autopilot.<field> path mirrors agent.<role>.<field>.
    if let Some(rest) = key.strip_prefix("autopilot.") {
        return set_autopilot_field(cfg, rest, value);
    }
    // M182 S4 (external review F-10): the sort-rebind contract is
    // `sort.<lane> <sortkey>` — the lane is the first dot-segment after
    // `sort.` (extra segments tolerated, to stay consistent with
    // `config_get`, which also takes the first segment) and the VALUE
    // is the sort key. This matches the data model
    // (`sort: BTreeMap<lane, sortkey>`), the `config_get sort.<lane>`
    // read path, and what raul's `persist_sort_rebind_choice` writes.
    // The prior three-segment `sort.<lane>.<key>` requirement rejected
    // raul's two-segment write and broke the whole confirm flow.
    // Unknown lanes or sort keys surface as a structured validation
    // error so raul's menu can show a useful hint.
    if let Some(rest) = key.strip_prefix("sort.") {
        let lane = rest.split_once('.').map(|(l, _)| l).unwrap_or(rest);
        let valid_sort_keys = ["id", "lifecycle", "priority", "updated"];
        let valid_lanes = [
            "overview",
            "milestones",
            "path",
            "tweaks",
            "ideas",
            "grooming",
            "backlog",
            "settings",
        ];
        if !valid_lanes.contains(&lane) {
            bail!(
                "unknown sort lane: {lane} (expected one of {})",
                valid_lanes.join(", ")
            );
        }
        if !valid_sort_keys.contains(&value) {
            bail!(
                "invalid sort key for {lane}: {value} (expected one of {})",
                valid_sort_keys.join(", ")
            );
        }
        cfg.sort.insert(lane.to_string(), value.to_string());
        return Ok(());
    }
    // M204: per-lane filter selection. The value is a JSON object
    // `{dimension: [values...]}`. An empty `{}` clears the lane's
    // filter. Unknown lanes surface a structured error; unknown
    // dimensions / non-string values surface as well so a malformed
    // payload can't slip past validate.
    if let Some(rest) = key.strip_prefix("filter.") {
        let lane = rest.split_once('.').map(|(l, _)| l).unwrap_or(rest);
        let valid_lanes = [
            "overview",
            "milestones",
            "path",
            "tweaks",
            "ideas",
            "grooming",
            "backlog",
            "settings",
        ];
        if !valid_lanes.contains(&lane) {
            bail!(
                "unknown filter lane: {lane} (expected one of {})",
                valid_lanes.join(", ")
            );
        }
        let parsed: Value = serde_json::from_str(value).map_err(|e| {
            anyhow::anyhow!(
                "filter.{lane} value must be a JSON object {{dimension: [values...]}} (e.g. '{{\"lifecycle\":[\"approved\"]}}'): {e}"
            )
        })?;
        let obj = parsed
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("filter.{lane} value must be a JSON object"))?;
        let mut inner: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        for (dim, val) in obj {
            let arr = val.as_array().ok_or_else(|| {
                anyhow::anyhow!("filter.{lane}.{dim} must be a JSON array of strings")
            })?;
            let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for v in arr {
                let s = v
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("filter.{lane}.{dim} entries must be strings"))?
                    .to_string();
                if !s.is_empty() {
                    set.insert(s);
                }
            }
            if !set.is_empty() {
                inner.insert(dim.clone(), set);
            }
        }
        if inner.is_empty() {
            cfg.filter.remove(lane);
        } else {
            cfg.filter.insert(lane.to_string(), inner);
        }
        return Ok(());
    }
    match key {
        "workflow.profile" => {
            if value.is_empty() {
                bail!("workflow.profile cannot be empty");
            }
            cfg.workflow.profile = value.to_string();
        }
        "workflow.plan.location" => {
            cfg.workflow.plan.location = Some(value.to_string());
        }
        "workflow.plan.in_repo" => {
            cfg.workflow.plan.in_repo = Some(parse_bool(value)?);
        }
        "workflow.gates.strictness" => {
            cfg.workflow.gates.strictness = Some(value.to_string());
        }
        "workflow.steps.code_review" => {
            cfg.workflow.steps.code_review = Some(parse_bool(value)?);
        }
        "git.auto_commit" => cfg.git.auto_commit = Some(parse_bool(value)?),
        "git.commit_on_milestone_complete" => {
            cfg.git.commit_on_milestone_complete = Some(parse_bool(value)?);
        }
        "git.auto_push" => cfg.git.auto_push = Some(parse_bool(value)?),
        "next.prefer" => cfg.next.prefer = Some(value.to_string()),
        "ui.color" => cfg.ui.color = Some(parse_bool(value)?),
        "ui.icons" => cfg.ui.icons = Some(parse_icons(value)?),
        "ui.theme" => cfg.ui.theme = Some(value.to_string()),
        "ui.hide_done" => cfg.ui.hide_done = Some(parse_bool(value)?),
        // M198 WP1 / M229: same shape as `ui.hide_done` — bool,
        // defaults to `false` (Autopilot tab hidden). The on-disk
        // config never sees `null`; an explicit `false` is a valid
        // value the operator can stage and the doctor + TUI
        // surfaces can render as the "default" state.
        "ui.show_autopilot_tab" => cfg.ui.show_autopilot_tab = Some(parse_bool(value)?),
        _ => bail!("unknown config key: {key}"),
    }
    Ok(())
}

pub fn config_set(
    ctx: &PlanContext,
    key: &str,
    value: &str,
    dry_run: bool,
) -> Result<ConfigSetReport> {
    let mut cfg = match store::try_load_config(ctx) {
        Ok(cfg) => cfg,
        Err(e) => {
            return Ok(ConfigSetReport {
                ok: false,
                dry_run,
                key: key.to_string(),
                value: value.to_string(),
                config: None,
                errors: vec![load_error_issue(&e)],
                warnings: Vec::new(),
            });
        }
    };
    if let Err(e) = apply_config_set(&mut cfg, key, value) {
        return Ok(ConfigSetReport {
            ok: false,
            dry_run,
            key: key.to_string(),
            value: value.to_string(),
            config: None,
            errors: vec![ConfigFieldIssue {
                field: key.to_string(),
                message: e.to_string(),
            }],
            warnings: Vec::new(),
        });
    }
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    collect_semantic_issues(&cfg, &mut errors, &mut warnings);
    if !errors.is_empty() {
        return Ok(ConfigSetReport {
            ok: false,
            dry_run,
            key: key.to_string(),
            value: value.to_string(),
            config: None,
            errors,
            warnings,
        });
    }
    if dry_run {
        return Ok(ConfigSetReport {
            ok: true,
            dry_run: true,
            key: key.to_string(),
            value: value.to_string(),
            config: Some(redact_agent_commands(cfg)),
            errors: Vec::new(),
            warnings,
        });
    }
    if let Err(e) = store::write_config(ctx, &cfg) {
        return Ok(ConfigSetReport {
            ok: false,
            dry_run: false,
            key: key.to_string(),
            value: value.to_string(),
            config: None,
            errors: vec![ConfigFieldIssue {
                field: FIELD_CONFIG.to_string(),
                message: format!("failed to write config: {e:#}"),
            }],
            warnings,
        });
    }
    Ok(ConfigSetReport {
        ok: true,
        dry_run: false,
        key: key.to_string(),
        value: value.to_string(),
        config: Some(redact_agent_commands(cfg)),
        errors: Vec::new(),
        warnings,
    })
}

/// Validate the project's current config, or a candidate file via `file`.
/// Never writes. Parse failures and semantic issues become structured errors.
pub fn config_validate(ctx: &PlanContext, file: Option<&Path>) -> ConfigValidateReport {
    let (cfg, mut errors) = match file {
        Some(path) => load_config_from_path(path),
        None => match store::try_load_config(ctx) {
            Ok(cfg) => (Some(cfg), Vec::new()),
            Err(e) => (None, vec![load_error_issue(&e)]),
        },
    };

    let mut warnings = Vec::new();
    if let Some(cfg) = cfg.as_ref() {
        collect_semantic_issues(cfg, &mut errors, &mut warnings);
    }

    ConfigValidateReport {
        ok: errors.is_empty(),
        errors,
        warnings,
    }
}

fn load_config_from_path(path: &Path) -> (Option<ProjectConfig>, Vec<ConfigFieldIssue>) {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return (
                None,
                vec![ConfigFieldIssue {
                    field: "file".to_string(),
                    message: format!("config file not found: {} ({e})", path.display()),
                }],
            );
        }
    };
    if !meta.is_file() {
        return (
            None,
            vec![ConfigFieldIssue {
                field: "file".to_string(),
                message: format!("config path is not a regular file: {}", path.display()),
            }],
        );
    }
    let s = match store::read_text_bounded(path, store::MAX_PLAN_FILE_BYTES) {
        Ok(s) => s,
        Err(e) => {
            return (
                None,
                vec![ConfigFieldIssue {
                    field: "file".to_string(),
                    message: format!("failed to read {}: {e:#}", path.display()),
                }],
            );
        }
    };
    match serde_json::from_str::<ProjectConfig>(&s) {
        Ok(cfg) => (Some(cfg), Vec::new()),
        Err(e) => (
            None,
            vec![ConfigFieldIssue {
                field: FIELD_CONFIG.to_string(),
                message: format!("failed to parse {}: {e}", path.display()),
            }],
        ),
    }
}

/// Replace `agent.<role>.command` arrays with `"<redacted: N tokens>"` so
/// argv-shaped secrets (which a user may legitimately pass through
/// `mp config set agent.runner.command`) do not land in stdout. The count
/// is preserved so a config-shape diagnostic still surfaces the right size.
fn redact_agent_commands(mut cfg: ProjectConfig) -> ProjectConfig {
    for rc in [&mut cfg.agent.runner, &mut cfg.agent.coordinator] {
        if let Some(cmd) = rc.command.as_ref() {
            let n = cmd.len();
            rc.command = Some(vec![format!("<redacted: {n} tokens>")]);
        }
    }
    cfg
}

/// Iterate `(name, &RoleConfig)` over every agent role. Lets both the
/// semantic gate and `set_agent_field` share the same role enumeration so
/// adding a third role is a one-line change here.
fn roles_iter(cfg: &ProjectConfig) -> [(&'static str, &RoleConfig); 2] {
    [
        ("runner", &cfg.agent.runner),
        ("coordinator", &cfg.agent.coordinator),
    ]
}

/// Validate an `agent.<role>.command` argv vector. Mirrors the rule
/// enforced by `parse_command_argv` so a hand-edited config that contains
/// a bare token with whitespace is rejected by `validate` / `set --dry-run`
/// before herdr hits the runtime "no such file" path.
fn validate_command_argv(cmd: &[String]) -> Result<()> {
    if cmd.is_empty() {
        bail!("agent command cannot be empty");
    }
    for tok in cmd {
        if tok.is_empty() {
            bail!("agent command entries must be non-empty");
        }
        if tok.chars().any(|c| c.is_whitespace()) {
            bail!(
                "agent command entries must not contain whitespace (got {tok:?}); use the JSON-array form to pass flags"
            );
        }
    }
    Ok(())
}

/// Semantic checks that mirror the constraints enforced by `config set`.
fn collect_semantic_issues(
    cfg: &ProjectConfig,
    errors: &mut Vec<ConfigFieldIssue>,
    warnings: &mut Vec<ConfigFieldIssue>,
) {
    if let Some(icons) = cfg.ui.icons.as_deref() {
        if !matches!(icons, "none" | "ascii" | "unicode") {
            errors.push(ConfigFieldIssue {
                field: "ui.icons".to_string(),
                message: format!("expected one of none|ascii|unicode, got {icons}"),
            });
        }
    }

    if let Some(theme) = cfg.ui.theme.as_deref() {
        if !crate::config::UI_THEMES.contains(&theme) {
            errors.push(ConfigFieldIssue {
                field: "ui.theme".to_string(),
                message: format!(
                    "expected one of {}, got {theme}",
                    crate::config::UI_THEMES.join("|")
                ),
            });
        }
    }

    if cfg.workflow.profile.is_empty() {
        warnings.push(ConfigFieldIssue {
            field: "workflow.profile".to_string(),
            message: "workflow.profile is empty; falling back to default (\"full\")".to_string(),
        });
    } else if !matches!(cfg.workflow.profile.as_str(), "full" | "hybrid" | "session") {
        errors.push(ConfigFieldIssue {
            field: "workflow.profile".to_string(),
            message: format!(
                "unknown profile: {} (expected full, hybrid, or session)",
                cfg.workflow.profile
            ),
        });
    }

    if let Some(strictness) = cfg.workflow.gates.strictness.as_deref() {
        if !matches!(strictness, "relaxed" | "full") {
            errors.push(ConfigFieldIssue {
                field: "workflow.gates.strictness".to_string(),
                message: format!("expected one of relaxed|full, got {strictness}"),
            });
        }
    }

    for action in cfg.keybinds.keys() {
        if crate::config::KEYBIND_ACTIONS.contains(&action.as_str()) {
            continue;
        }
        if action == "focus_content" {
            // M200: `focus_content` is a TUI-internal reserved action; the
            // field is intentionally not user-rebindable. Emit a non-blocking
            // deprecation warning so users with a stale config line learn
            // the key is gone (without forcing an error on validate). The
            // warning names the field and points at CHANGELOG.md, with no
            // milestone IDs or internal paths in the user-visible text.
            warnings.push(ConfigFieldIssue {
                field: "keybinds.focus_content".to_string(),
                message: "'keybinds.focus_content' is deprecated and no longer has effect; \
                     remove it from your config to silence this warning. \
                     See CHANGELOG.md for details."
                    .to_string(),
            });
            continue;
        }
        errors.push(ConfigFieldIssue {
            field: format!("keybinds.{action}"),
            message: format!(
                "unknown keybind action: keybinds.{action} (expected one of: {})",
                crate::config::KEYBIND_ACTIONS.join(", ")
            ),
        });
    }

    for (role, rc) in roles_iter(cfg) {
        if let Some(harness) = rc.harness.as_deref() {
            if !crate::config::WATCH_HARNESSES.contains(&harness) {
                errors.push(ConfigFieldIssue {
                    field: format!("agent.{role}.harness"),
                    message: format!(
                        "agent.{role}.harness must be one of {} (got {harness})",
                        crate::config::WATCH_HARNESSES.join("|")
                    ),
                });
            }
        }
        if let Some(cmd) = rc.command.as_ref() {
            if let Err(e) = validate_command_argv(cmd) {
                errors.push(ConfigFieldIssue {
                    field: format!("agent.{role}.command"),
                    message: e.to_string(),
                });
            }
        }
    }

    // M147: enum validation for [agent.automation] — applies at validate
    // time and from `set` via the same `errors` channel so a hand-edited
    // config with a typo is rejected uniformly with `set --dry-run`.
    if let Some(bs) = cfg.agent.automation.branch_strategy.as_deref() {
        if !crate::config::BRANCH_STRATEGIES.contains(&bs) {
            errors.push(ConfigFieldIssue {
                field: "agent.automation.branch_strategy".to_string(),
                message: format!(
                    "agent.automation.branch_strategy must be one of {} (got {bs:?})",
                    crate::config::BRANCH_STRATEGIES.join("|")
                ),
            });
        }
    }
    if let Some(ar) = cfg.agent.automation.auto_remediate.as_deref() {
        if !crate::config::AUTO_REMEDIATE_VALUES.contains(&ar) {
            errors.push(ConfigFieldIssue {
                field: "agent.automation.auto_remediate".to_string(),
                message: format!(
                    "agent.automation.auto_remediate must be one of {} (got {ar:?})",
                    crate::config::AUTO_REMEDIATE_VALUES.join("|")
                ),
            });
        }
    }
}

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => bail!("expected boolean, got {value}"),
    }
}

fn parse_icons(value: &str) -> Result<String> {
    match value {
        "none" | "ascii" | "unicode" => Ok(value.to_string()),
        _ => bail!("expected one of none|ascii|unicode, got {value}"),
    }
}

/// Validate a `keybinds.<action>` name against the known action set.
fn validate_keybind_action(action: &str) -> Result<()> {
    if crate::config::KEYBIND_ACTIONS.contains(&action) {
        Ok(())
    } else if action == "focus_content" {
        // M200: `focus_content` was removed from the user-rebindable set,
        // so explicit `mp config set keybinds.focus_content ...` must be
        // rejected (exit 1). The error message mirrors the validate-path
        // deprecation text so users see one canonical explanation.
        bail!(
            "'keybinds.focus_content' is deprecated and no longer user-rebindable; \
             see CHANGELOG.md for details."
        )
    } else {
        bail!(
            "unknown keybind action: keybinds.{action} (expected one of: {})",
            crate::config::KEYBIND_ACTIONS.join(", ")
        )
    }
}

/// Read a single `keybinds.<action>` value. Returns the stored combo string,
/// or the canonical default from `KEYBIND_DEFAULTS` when the action is
/// unset. Existing user overrides win over the canonical default.
fn config_get_keybind(cfg: &ProjectConfig, action: &str) -> Result<Value> {
    validate_keybind_action(action)?;
    if let Some(v) = cfg.keybinds.get(action) {
        return Ok(json!(v));
    }
    // M200: surface the canonical default instead of null so the user
    // auditing their config sees the effective value. raul applies the
    // same default at runtime via `Keybinds::default()`; the canonical
    // string lives in `KEYBIND_DEFAULTS` (mp cannot depend on raul).
    for (name, default) in crate::config::KEYBIND_DEFAULTS {
        if *name == action {
            return Ok(json!(default));
        }
    }
    Ok(Value::Null)
}

/// Set a single `keybinds.<action>` binding. mp stores the raw combo string;
/// raul parses and validates it at load time (falling back to the default on
/// a malformed string). An empty value clears the override.
fn set_keybind(cfg: &mut ProjectConfig, action: &str, value: &str) -> Result<()> {
    validate_keybind_action(action)?;
    if value.is_empty() {
        cfg.keybinds.remove(action);
    } else {
        cfg.keybinds.insert(action.to_string(), value.to_string());
    }
    // M200: drop a stale `keybinds.focus_content` line on any successful
    // keybind write. The key is no longer user-rebindable (see the
    // validate-path warning); self-healing the moment the user touches
    // any keybind keeps the config from carrying a no-op line forward.
    // `keybinds.focus_content` is never written here because
    // `validate_keybind_action` rejects the action with exit 1.
    if cfg.keybinds.contains_key("focus_content") {
        cfg.keybinds.remove("focus_content");
    }
    Ok(())
}

// --- M149 mp watch agent config (`agent.<role>.<field>`) ---

/// Split `agent.<role>.<field>` into its `(role, field)` parts.
fn split_agent_key(rest: &str) -> Result<(&str, &str)> {
    let (role, field) = rest
        .split_once('.')
        .ok_or_else(|| anyhow::anyhow!("expected agent.<role>.<field>, got agent.{rest}"))?;
    if !matches!(role, "runner" | "coordinator") {
        bail!("unknown agent role: {role} (expected runner or coordinator)");
    }
    Ok((role, field))
}

fn role_mut<'a>(cfg: &'a mut ProjectConfig, role: &str) -> &'a mut RoleConfig {
    match role {
        "runner" => &mut cfg.agent.runner,
        "coordinator" => &mut cfg.agent.coordinator,
        // unreachable: split_agent_key rejects unknown roles
        _ => unreachable!("validated role in split_agent_key"),
    }
}

fn config_get_agent(cfg: &ProjectConfig, rest: &str) -> Result<Value> {
    let (role, field) = split_agent_key(rest)?;
    let rc = match role {
        "runner" => cfg.runner_config(),
        "coordinator" => cfg.coordinator_config(),
        _ => unreachable!("validated role in split_agent_key"),
    };
    Ok(match field {
        "harness" => json!(rc.harness),
        "command" => json!(rc.command),
        "model" => json!(rc.model),
        "thinking_level" => json!(rc.thinking_level),
        _ => bail!("unknown agent field: agent.{role}.{field}"),
    })
}

fn set_agent_field(cfg: &mut ProjectConfig, rest: &str, value: &str) -> Result<()> {
    let (role, field) = split_agent_key(rest)?;
    let rc = role_mut(cfg, role);
    match field {
        "harness" => {
            if !crate::config::WATCH_HARNESSES.contains(&value) {
                bail!(
                    "agent.{role}.harness must be one of {} (got {value})",
                    crate::config::WATCH_HARNESSES.join("|")
                );
            }
            rc.harness = Some(value.to_string());
        }
        "command" => {
            let parsed = parse_command_argv(value)?;
            validate_command_argv(&parsed)?;
            rc.command = Some(parsed);
        }
        "model" => rc.model = Some(value.to_string()),
        "thinking_level" => rc.thinking_level = Some(value.to_string()),
        _ => bail!("unknown agent field: agent.{role}.{field}"),
    }
    Ok(())
}

/// Parse `agent.<role>.command` value. Accepts a JSON array of strings
/// (`["opencode", "--flag"]`). Single-token shorthands are wrapped into a
/// one-element array (`opencode` → `["opencode"]`) for ergonomic CLI use.
/// Bare tokens that contain whitespace are rejected: `opencode --flag`
/// would silently become a single argv element with an embedded space
/// (`["opencode --flag"]`), which herdr would then fail to exec with a
/// confusing "no such file" error. Users who need flag-bearing commands
/// must use the JSON-array form.
fn parse_command_argv(value: &str) -> Result<Vec<String>> {
    let trimmed = value.trim();
    if trimmed.starts_with('[') {
        let parsed: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
            anyhow::anyhow!(
                "agent command must be a JSON array of strings (e.g. '[\"opencode\"]'): {e}"
            )
        })?;
        let arr = parsed
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("agent command must be a JSON array, got {value}"))?;
        return arr
            .iter()
            .map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow::anyhow!("agent command entries must be strings"))
            })
            .collect();
    }
    if trimmed.is_empty() {
        bail!("agent command cannot be empty");
    }
    if trimmed.chars().any(|c| c.is_whitespace()) {
        bail!(
            "agent command with whitespace must be a JSON array (e.g. [\"opencode\", \"--flag\"]); got bare token {value:?}"
        );
    }
    Ok(vec![trimmed.to_string()])
}

// --- M147 agent.automation.<field> ---

/// M147: read a single `agent.automation.<field>` value. The accessor
/// methods (`commit_after_execute`, `push_after_review`,
/// `automation_branch_strategy`, `automation_auto_remediate`) own the
/// defaults so `mp config get agent.automation.<field>` returns the
/// effective value (not `null`) — same surface the agent sees at
/// runtime.
fn config_get_automation(cfg: &ProjectConfig, field: &str) -> Result<Value> {
    validate_automation_field(field)?;
    Ok(match field {
        "commit_after_execute" => json!(cfg.commit_after_execute()),
        "push_after_review" => json!(cfg.push_after_review()),
        "branch_strategy" => json!(cfg.automation_branch_strategy()),
        "auto_remediate" => json!(cfg.automation_auto_remediate()),
        _ => bail!("unknown automation field: agent.automation.{field}"),
    })
}

/// M147: apply one `agent.automation.<field> = <value>` mutation. Bool
/// fields accept the standard `true|false|1|0|yes|no` token set; enum
/// fields are validated against `BRANCH_STRATEGIES` / `AUTO_REMEDIATE_VALUES`
/// and rejected at apply time so a typo like `branch_strategy = "foo"`
/// never lands on disk.
fn set_automation_field(cfg: &mut ProjectConfig, field: &str, value: &str) -> Result<()> {
    validate_automation_field(field)?;
    match field {
        "commit_after_execute" => {
            cfg.agent.automation.commit_after_execute = Some(parse_bool(value)?)
        }
        "push_after_review" => cfg.agent.automation.push_after_review = Some(parse_bool(value)?),
        "branch_strategy" => {
            if !crate::config::BRANCH_STRATEGIES.contains(&value) {
                bail!(
                    "agent.automation.branch_strategy must be one of {} (got {value:?})",
                    crate::config::BRANCH_STRATEGIES.join("|")
                );
            }
            cfg.agent.automation.branch_strategy = Some(value.to_string());
        }
        "auto_remediate" => {
            if !crate::config::AUTO_REMEDIATE_VALUES.contains(&value) {
                bail!(
                    "agent.automation.auto_remediate must be one of {} (got {value:?})",
                    crate::config::AUTO_REMEDIATE_VALUES.join("|")
                );
            }
            cfg.agent.automation.auto_remediate = Some(value.to_string());
        }
        _ => bail!("unknown automation field: agent.automation.{field}"),
    }
    Ok(())
}

/// M147: enumerate the four M147 knobs in one place so `set` and `get`
/// share the same surface — adding a fifth knob is a one-line change
/// here plus `AgentAutomationConfig`.
fn validate_automation_field(field: &str) -> Result<()> {
    match field {
        "commit_after_execute"
        | "push_after_review"
        | "branch_strategy"
        | "auto_remediate" => Ok(()),
        other => bail!(
            "unknown automation field: agent.automation.{other} (expected one of: commit_after_execute, push_after_review, branch_strategy, auto_remediate)"
        ),
    }
}

// --- M209: autopilot section --------------------------------------------

/// M209: `mp config get autopilot.<rest>`. The dispatch mirrors
/// `agent.<role>.<field>`: `autopilot.topology` (string choice),
/// `autopilot.refresh_secs` (non-negative integer), and
/// `autopilot.roles.<role>.<field>` where `<role>` is one of
/// `orchestrator | runner | reviewer` and `<field>` is one of
/// `model | harness | skill | extras`.
fn config_get_autopilot(cfg: &ProjectConfig, rest: &str) -> Result<Value> {
    if rest == "topology" {
        return Ok(json!(cfg
            .autopilot
            .topology
            .clone()
            .unwrap_or_else(default_topology)));
    }
    if rest == "refresh_secs" {
        return Ok(json!(cfg.autopilot.refresh_secs));
    }
    if let Some(role_rest) = rest.strip_prefix("roles.") {
        let (role, field) = split_autopilot_role_field(role_rest)?;
        let ovr = cfg.autopilot.roles.get(role).cloned().unwrap_or_default();
        return Ok(match field {
            "model" => json!(ovr.model),
            "harness" => json!(ovr.harness),
            "skill" => json!(ovr.skill),
            "extras" => json!(ovr.extras),
            other => bail!(
                "unknown autopilot role field: autopilot.roles.<role>.{other} (expected model|harness|skill|extras)"
            ),
        });
    }
    bail!(
        "unknown autopilot key: autopilot.{rest} (expected topology|refresh_secs|roles.<role>.<field>)"
    )
}

/// M209: apply one `autopilot.<rest> = <value>` mutation.
fn set_autopilot_field(cfg: &mut ProjectConfig, rest: &str, value: &str) -> Result<()> {
    if rest == "topology" {
        validate_topology(value)?;
        cfg.autopilot.topology = Some(value.to_string());
        return Ok(());
    }
    if rest == "refresh_secs" {
        let n: i64 = value.parse().map_err(|_| {
            anyhow::anyhow!("autopilot.refresh_secs must be a non-negative integer (got {value:?})")
        })?;
        if n < 0 {
            bail!("autopilot.refresh_secs cannot be negative (got {n})");
        }
        cfg.autopilot.refresh_secs = Some(n as u64);
        return Ok(());
    }
    if let Some(role_rest) = rest.strip_prefix("roles.") {
        let (role, field) = split_autopilot_role_field(role_rest)?;
        validate_autopilot_role(role)?;
        validate_autopilot_role_field(field)?;
        let entry = cfg.autopilot.roles.entry(role.to_string()).or_default();
        match field {
            "model" => entry.model = Some(value.to_string()),
            "harness" => entry.harness = Some(value.to_string()),
            "skill" => entry.skill = Some(value.to_string()),
            "extras" => bail!(
                "autopilot.roles.<role>.extras must be a JSON object (set per key instead: \
                 e.g. `mp autopilot config set autopilot.roles.<role>.extras.<key> <value>`)"
            ),
            other => bail!(
                "unknown autopilot role field: autopilot.roles.<role>.{other} (expected model|harness|skill|extras)"
            ),
        }
        // Drop empty entries so a fully-cleared role doesn't leave
        // an empty object on disk (parity with `agent.<role>`).
        if entry.is_empty() {
            cfg.autopilot.roles.remove(role);
        }
        return Ok(());
    }
    bail!(
        "unknown autopilot key: autopilot.{rest} (expected topology|refresh_secs|roles.<role>.<field>)"
    )
}

fn split_autopilot_role_field(rest: &str) -> Result<(&str, &str)> {
    let (role, field) = rest.split_once('.').ok_or_else(|| {
        anyhow::anyhow!("expected autopilot.roles.<role>.<field>, got autopilot.roles.{rest}")
    })?;
    Ok((role, field))
}

fn default_topology() -> String {
    // Keep the literal string, not the enum, so the config getter
    // reports the same shape the user wrote.
    "three-agent".to_string()
}

fn validate_topology(value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("autopilot.topology cannot be empty");
    }
    let parsed = value.parse::<crate::autopilot::role::Topology>();
    match parsed {
        Ok(_) => Ok(()),
        Err(_) => bail!(
            "autopilot.topology must be one of one-agent|two-agent|three-agent (got {value:?})"
        ),
    }
}

fn validate_autopilot_role(role: &str) -> Result<()> {
    match role {
        "orchestrator" | "runner" | "reviewer" => Ok(()),
        other => bail!(
            "unknown autopilot role: autopilot.roles.{other} (expected orchestrator|runner|reviewer)"
        ),
    }
}

fn validate_autopilot_role_field(field: &str) -> Result<()> {
    match field {
        "model" | "harness" | "skill" | "extras" => Ok(()),
        other => bail!(
            "unknown autopilot role field: autopilot.roles.<role>.{other} (expected model|harness|skill|extras)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(command: Vec<String>) -> ProjectConfig {
        let mut cfg = ProjectConfig::default();
        cfg.agent.runner.command = Some(command);
        cfg
    }

    /// F-01: set and dry-run share the same semantic gate, so a value
    /// that fails validation under dry-run also fails under real set,
    /// and *both* surface the full error list (not just the first).
    #[test]
    fn collect_semantic_issues_collects_all_errors() {
        let mut cfg = ProjectConfig::default();
        cfg.ui.icons = Some("emoji".into());
        cfg.workflow.profile = "bogus".into();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        collect_semantic_issues(&cfg, &mut errors, &mut warnings);
        let fields: Vec<_> = errors.iter().map(|e| e.field.clone()).collect();
        assert!(
            fields.contains(&"ui.icons".to_string()),
            "expected ui.icons error; got {fields:?}"
        );
        assert!(
            fields.contains(&"workflow.profile".to_string()),
            "expected workflow.profile error; got {fields:?}"
        );
    }

    /// F-02: whitespace inside an agent command argv entry is rejected by
    /// the semantic gate, mirroring the parse_command_argv rule.
    #[test]
    fn collect_semantic_issues_rejects_whitespace_in_argv() {
        let cfg = cfg_with(vec!["opencode --flag".into()]);
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        collect_semantic_issues(&cfg, &mut errors, &mut warnings);
        assert!(
            errors.iter().any(|e| e.field == "agent.runner.command"),
            "expected agent.runner.command error; got {:?}",
            errors
        );
    }

    /// F-03: empty `workflow.profile` becomes a warning, not an error —
    /// the runtime default ("full") still applies. Without this the
    /// `warnings` field of the JSON contract would be permanently empty.
    #[test]
    fn empty_profile_is_warning_not_error() {
        let mut cfg = ProjectConfig::default();
        cfg.workflow.profile = String::new();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        collect_semantic_issues(&cfg, &mut errors, &mut warnings);
        assert!(
            errors.is_empty(),
            "empty profile must not error; got {errors:?}"
        );
        assert!(
            warnings.iter().any(|w| w.field == "workflow.profile"),
            "expected workflow.profile warning; got {warnings:?}"
        );
    }

    /// F-04: redact_agent_commands replaces argv-shaped secrets with a
    /// token-count marker so they don't land in stdout.
    #[test]
    fn redact_replaces_command_with_count_marker() {
        let cfg = cfg_with(vec!["opencode".into(), "--flag".into()]);
        let redacted = redact_agent_commands(cfg);
        assert_eq!(
            redacted.agent.runner.command,
            Some(vec!["<redacted: 2 tokens>".to_string()])
        );
        assert!(redacted.agent.coordinator.command.is_none());
    }

    /// F-05: load_error_issue is the single source of truth for the
    /// `field: "config"` error emitted by every config-loader failure.
    #[test]
    fn load_error_issue_uses_config_field() {
        let e = anyhow::anyhow!("boom");
        let issue = load_error_issue(&e);
        assert_eq!(issue.field, "config");
        assert!(issue.message.contains("boom"));
    }

    // --- M154: [review] section round-trip ---------------------------------

    /// M154 AC-01: `review.hunk` round-trips through apply_config_set +
    /// config_get. Default off, opt-in via `true|1|yes`. Non-bool
    /// values are rejected by parse_bool before the round-trip starts
    /// (mirrors the existing parse_bool contract for other booleans).
    #[test]
    fn review_hunk_round_trips_through_apply_config_set() {
        // 1. default-off on a fresh config.
        let mut cfg = ProjectConfig::default();
        assert!(
            !cfg.review_hunk_enabled(),
            "review.hunk must default to false"
        );

        // 2. set review.hunk=true; round-trip via the accessor (not the
        // raw field) because the accessor owns the default fallback.
        apply_config_set(&mut cfg, "review.hunk", "true").unwrap();
        assert!(
            cfg.review_hunk_enabled(),
            "review.hunk must read back true after set"
        );

        // 3. set review.hunk=false; round-trip back.
        apply_config_set(&mut cfg, "review.hunk", "false").unwrap();
        assert!(
            !cfg.review_hunk_enabled(),
            "review.hunk must read back false after set"
        );

        // 4. invalid bool is rejected at apply time (not later).
        let err = apply_config_set(&mut cfg, "review.hunk", "yes please")
            .expect_err("non-bool review.hunk must be rejected");
        assert!(
            err.to_string().contains("expected boolean"),
            "expected parse_bool error; got {err}"
        );
    }

    /// M154 AC-01: `review.hunk_author` round-trips. Default "mp".
    /// Empty string is rejected at apply time so a typo can't
    /// accidentally clear the field.
    #[test]
    fn review_hunk_author_round_trips_through_apply_config_set() {
        let mut cfg = ProjectConfig::default();
        assert_eq!(cfg.review_hunk_author(), "mp");

        apply_config_set(&mut cfg, "review.hunk_author", "reviewer:alice").unwrap();
        assert_eq!(cfg.review_hunk_author(), "reviewer:alice");

        // Empty author is a guardable error — the parser used to silently
        // accept "" before M154.
        let err = apply_config_set(&mut cfg, "review.hunk_author", "")
            .expect_err("empty review.hunk_author must be rejected");
        assert!(
            err.to_string().contains("hunk_author cannot be empty"),
            "expected empty-author guard; got {err}"
        );

        // Unknown review.<field> errors with the documented surface.
        let err = apply_config_set(&mut cfg, "review.bogus", "true")
            .expect_err("unknown review.* field must be rejected");
        assert!(
            err.to_string().contains("expected hunk|hunk_author"),
            "expected unknown-field guard; got {err}"
        );
    }

    /// M154 AC-01: an absent `[review]` section on disk deserializes to
    /// `review.hunk = false` / `review.hunk_author = "mp"` (defaults).
    /// This is the backward-compat case: pre-M154 projects don't have
    /// the section, and `mp config get review.hunk` still returns `false`
    /// (the documented default).
    #[test]
    fn review_section_absent_round_trips_via_toml() {
        // Minimal pre-M154 config — no [review] block.
        let pre_m154 = r#"
[workflow]
profile = "full"
"#;
        let cfg: ProjectConfig = toml::from_str(pre_m154).unwrap();
        assert!(
            !cfg.review_hunk_enabled(),
            "absent [review] must default hunk=false"
        );
        assert_eq!(
            cfg.review_hunk_author(),
            "mp",
            "absent [review] must default hunk_author=mp"
        );

        // Post-M154 config — the section is present, values honored.
        let post_m154 = r#"
[review]
hunk = true
hunk_author = "mp-coordinator"
"#;
        let cfg: ProjectConfig = toml::from_str(post_m154).unwrap();
        assert!(
            cfg.review_hunk_enabled(),
            "review.hunk=true must deserialize true"
        );
        assert_eq!(cfg.review_hunk_author(), "mp-coordinator");
    }
}
