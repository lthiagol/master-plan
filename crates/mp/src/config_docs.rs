//! M201: per-key prose + shape for the typed config schema
//! (`mp config schema`).
//!
//! Each entry is `(key, ty, default, allowed?, description)`. `default`
//! is a static fallback string — `mp` overrides it at emit time using
//! `ProjectConfig::default()` (non-keybind rows) and `KEYBIND_DEFAULTS`
//! (keybind rows). The fallback is what `make test-scenarios` golden
//! fixtures pin against an empty project, so it must match the live
//! defaults line-for-line.
//!
//! Descriptions are user-facing prose: no internal paths, no milestone
//! IDs, no lesson codes, no repo-internal doc pointers. The
//! consumer-surface lint (`make consumer-surface-lint`) flags leaks.
//!
//! The list mirrors `crates/raul/src/tui/modes/settings::SETTINGS_KEYS`
//! after M200 dropped `focus_content`. Adding a new setting is a
//! three-place change: the `Keybinds` struct in raul, `SETTINGS_KEYS`,
//! and `KEY_DESCRIPTIONS` here — plus the `KEYBIND_DEFAULTS` table when
//! the new key is a keybind.

use crate::config::{ConfigSchemaReport, SchemaEntry, CONFIG_SCHEMA_VERSION};

/// M201: 45 per-key rows. The list is the single source of truth that
/// `mp config schema` projects into the `keys` array (sorted by key at
/// emit time).
#[allow(clippy::type_complexity)]
pub const KEY_DESCRIPTIONS: &[(&str, &str, &str, Option<&[&str]>, &str)] = &[
    // ---- ui -------------------------------------------------------------
    (
        "agent.automation.auto_remediate",
        "choice",
        "none",
        Some(AUTO_REMEDIATE),
        "How aggressive the auto-remediation loop should be when the runner surfaces a fixable finding.",
    ),
    (
        "agent.automation.branch_strategy",
        "choice",
        "current",
        Some(BRANCH_STRATEGIES),
        "Branch strategy the runner uses when a milestone needs its own branch.",
    ),
    (
        "agent.automation.commit_after_execute",
        "bool",
        "false",
        None,
        "When true, the runner commits after each executed step that touched the plan.",
    ),
    (
        "agent.automation.push_after_review",
        "bool",
        "false",
        None,
        "When true, the runner pushes the review branch automatically once it lands cleanly.",
    ),
    (
        "git.auto_commit",
        "bool",
        "false",
        None,
        "Auto-commit plan mutations after each step that touches the repo.",
    ),
    (
        "git.auto_push",
        "bool",
        "false",
        None,
        "When auto-commit is on, push the resulting commit too.",
    ),
    (
        "git.commit_on_milestone_complete",
        "bool",
        "false",
        None,
        "Auto-commit a milestone-completion marker when a milestone closes out.",
    ),
    (
        "keybinds.approve",
        "keybind",
        "p",
        None,
        "Approve the currently selected milestone from the review menu.",
    ),
    (
        "keybinds.create_annotation",
        "keybind",
        "A",
        None,
        "Open the annotation composer on the focused milestone.",
    ),
    (
        "keybinds.cycle_sort",
        "keybind",
        "o",
        None,
        "Cycle through the available sort orders for the active list lane.",
    ),
    (
        "keybinds.down",
        "keybind",
        "Down, j",
        None,
        "Move the list cursor one row down. Vim-style `j` is mirrored as an alias.",
    ),
    (
        "keybinds.enter",
        "keybind",
        "Enter",
        None,
        "Activate the focused row — open the editor on a setting, expand a milestone, or commit a search.",
    ),
    (
        "keybinds.escape",
        "keybind",
        "Esc",
        None,
        "Cancel the active overlay or dismiss the help screen.",
    ),
    (
        "keybinds.filter",
        "keybind",
        "f",
        None,
        "Open the lane-specific filter input.",
    ),
    (
        "keybinds.grooming_preset",
        "keybind",
        "g",
        None,
        "Cycle through the grooming presets on the Backlog lane.",
    ),
    (
        "keybinds.help",
        "keybind",
        "?",
        None,
        "Toggle the keyboard-help overlay.",
    ),
    (
        "keybinds.hide_done",
        "keybind",
        "h",
        None,
        "Toggle the hide-done filter on list lanes.",
    ),
    (
        "keybinds.lifecycle_filter",
        "keybind",
        "F",
        None,
        "Open the multi-select lifecycle filter on the Milestones lane.",
    ),
    (
        "keybinds.next_item",
        "keybind",
        "n",
        None,
        "Move to the next sibling item within the focused section.",
    ),
    (
        "keybinds.next_lane",
        "keybind",
        "Right, l, Tab",
        None,
        "Move the lane focus one step to the right. Wraps at the end.",
    ),
    (
        "keybinds.next_section",
        "keybind",
        "]",
        None,
        "Jump to the next section header in the active list.",
    ),
    (
        "keybinds.open_settings",
        "keybind",
        "Ctrl-O",
        None,
        "Jump directly to the Settings lane.",
    ),
    (
        "keybinds.page_down",
        "keybind",
        "PageDown",
        None,
        "Page the active list down by one viewport.",
    ),
    (
        "keybinds.page_up",
        "keybind",
        "PageUp",
        None,
        "Page the active list up by one viewport.",
    ),
    (
        "keybinds.prev_item",
        "keybind",
        "p",
        None,
        "Move to the previous sibling item within the focused section.",
    ),
    (
        "keybinds.prev_section",
        "keybind",
        "[",
        None,
        "Jump to the previous section header in the active list.",
    ),
    (
        "keybinds.previous_lane",
        "keybind",
        "Left, BackTab",
        None,
        "Move the lane focus one step to the left. Wraps at the start.",
    ),
    (
        "keybinds.quit",
        "keybind",
        "q, Q",
        None,
        "Quit the TUI.",
    ),
    (
        "keybinds.refresh",
        "keybind",
        "Ctrl-R",
        None,
        "Reload the active lane from disk.",
    ),
    (
        "keybinds.reopen",
        "keybind",
        "R",
        None,
        "Reopen a closed milestone — undoes a previous resolve/approve on the focused row.",
    ),
    (
        "keybinds.resolve",
        "keybind",
        "r",
        None,
        "Mark the focused milestone as resolved.",
    ),
    (
        "keybinds.review_menu",
        "keybind",
        "m",
        None,
        "Open the per-milestone review menu.",
    ),
    (
        "keybinds.search",
        "keybind",
        "/",
        None,
        "Open the live substring search input on list lanes.",
    ),
    (
        "keybinds.up",
        "keybind",
        "Up, k",
        None,
        "Move the list cursor one row up. Vim-style `k` is mirrored as an alias.",
    ),
    (
        "next.prefer",
        "choice",
        "milestone",
        Some(NEXT_PREFER),
        "Which lane `mp next` should prefer when both have work available.",
    ),
    (
        "ui.color",
        "bool",
        "true",
        None,
        "Enable ANSI color for status, decisions, and read paths.",
    ),
    (
        "ui.hide_done",
        "bool",
        "false",
        None,
        "Hide milestones in the done lifecycle across all list lanes.",
    ),
    (
        "ui.icons",
        "choice",
        "unicode",
        Some(UI_ICONS),
        "Glyph set used by list decorators and section markers.",
    ),
    (
        "ui.show_watch_tab",
        "bool",
        "false",
        None,
        "Show the Watch lane in the tab bar. The `mp watch` command is unaffected — only the TUI surface reacts.",
    ),
    (
        "ui.theme",
        "choice",
        "mocha",
        Some(UI_THEMES),
        "Theme palette name. Picked up at startup; relaunch to apply.",
    ),
    (
        "workflow.gates.strictness",
        "choice",
        "relaxed",
        Some(STRICTNESS),
        "How strictly gate failures block the workflow.",
    ),
    (
        "workflow.plan.in_repo",
        "bool",
        "true",
        None,
        "Keep `master-plan/` inside the project repo (vs. an out-of-repo location).",
    ),
    (
        "workflow.plan.location",
        "path",
        "master-plan",
        None,
        "Path to the plan directory, relative to the project root.",
    ),
    (
        "workflow.profile",
        "choice",
        "full",
        Some(WORKFLOW_PROFILES),
        "Workflow profile — which artifacts and gates the project runs.",
    ),
    (
        "workflow.steps.code_review",
        "bool",
        "true",
        None,
        "Run an external review pass on milestones before they reach approved.",
    ),
];

/// M201: enum values that the user-facing descriptions reference.
pub const UI_THEMES: &[&str] = &["mocha", "macchiato", "frappe", "latte", "dracula"];
pub const UI_ICONS: &[&str] = &["none", "ascii", "unicode"];
pub const BRANCH_STRATEGIES: &[&str] = &["per-milestone", "current", "none"];
pub const AUTO_REMEDIATE: &[&str] = &["none", "low", "medium", "high", "all"];
pub const NEXT_PREFER: &[&str] = &["milestone", "track"];
pub const STRICTNESS: &[&str] = &["relaxed", "full"];
pub const WORKFLOW_PROFILES: &[&str] = &["full", "hybrid", "session"];

/// M201: build a `ConfigSchemaReport` from `KEY_DESCRIPTIONS` + live
/// defaults. The result is deterministic — entries are sorted by key —
/// and the `default` for each row reflects `ProjectConfig::default()`
/// for non-keybinds and `KEYBIND_DEFAULTS` for keybinds. `allowed` is
/// populated only for `choice` rows.
pub fn build_schema_report() -> ConfigSchemaReport {
    use crate::config::{ProjectConfig, KEYBIND_DEFAULTS};
    use std::collections::BTreeMap;

    let cfg = ProjectConfig::default();
    let keybind_defaults: BTreeMap<&str, &str> = KEYBIND_DEFAULTS.iter().copied().collect();

    // Sort by key — `KEY_DESCRIPTIONS` may list keys in any order; the
    // emitted schema must be deterministic across runs.
    let mut sorted: Vec<_> = KEY_DESCRIPTIONS.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));

    let mut keys: Vec<SchemaEntry> = Vec::with_capacity(sorted.len());
    for (key, ty, fallback, allowed, description) in sorted {
        let default = match ty {
            "keybind" => keybind_defaults
                .get(key.strip_prefix("keybinds.").unwrap_or(key))
                .copied()
                .unwrap_or(fallback)
                .to_string(),
            "bool" => match key {
                // Each branch MUST match the `unwrap_or` accessor used by
                // `config_cmd::config_get` for the same key — see
                // `schema_bool_defaults_match_project_config_default` in the
                // tests below for the regression guard.
                "ui.color" => cfg.ui.color.unwrap_or(true).to_string(),
                "ui.hide_done" => cfg.ui.hide_done.unwrap_or(false).to_string(),
                "ui.show_watch_tab" => cfg.ui.show_watch_tab.unwrap_or(false).to_string(),
                "git.auto_commit" => cfg.git.auto_commit.unwrap_or(false).to_string(),
                "git.auto_push" => cfg.git.auto_push.unwrap_or(false).to_string(),
                "git.commit_on_milestone_complete" => cfg
                    .git
                    .commit_on_milestone_complete
                    .unwrap_or(false)
                    .to_string(),
                "agent.automation.commit_after_execute" => cfg
                    .agent
                    .automation
                    .commit_after_execute
                    .unwrap_or(false)
                    .to_string(),
                "agent.automation.push_after_review" => cfg
                    .agent
                    .automation
                    .push_after_review
                    .unwrap_or(false)
                    .to_string(),
                "workflow.plan.in_repo" => cfg.workflow.plan.in_repo.unwrap_or(true).to_string(),
                "workflow.steps.code_review" => cfg
                    .workflow
                    .steps
                    .code_review
                    .unwrap_or(false)
                    .to_string(),
                _ => fallback.to_string(),
            },
            "integer" => fallback.to_string(),
            _ => fallback.to_string(),
        };
        let allowed_owned = allowed.map(|a| a.iter().map(|s| s.to_string()).collect());
        keys.push(SchemaEntry {
            key: key.to_string(),
            ty: ty.to_string(),
            default,
            allowed: allowed_owned,
            description: description.to_string(),
        });
    }

    ConfigSchemaReport {
        schema_version: CONFIG_SCHEMA_VERSION,
        keys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F-01: KEY_DESCRIPTIONS must hold exactly 45 rows after M200 dropped
    /// `focus_content`. Adding a new setting without touching this list
    /// breaks the schema contract — pin it.
    #[test]
    fn key_descriptions_len_is_45() {
        assert_eq!(
            KEY_DESCRIPTIONS.len(),
            45,
            "KEY_DESCRIPTIONS must have exactly 45 rows; got {}",
            KEY_DESCRIPTIONS.len()
        );
    }

    /// F-02: descriptions are user-facing prose. No `crates/` references,
    /// no milestone IDs (M\d+), no lesson codes (L\d), no internal
    /// doc-pointer paths. The lint at `make consumer-surface-lint` does
    /// the same check repo-wide — this unit test pins the schema surface.
    #[test]
    fn descriptions_are_consumer_surface_clean() {
        for (key, _ty, _default, _allowed, description) in KEY_DESCRIPTIONS {
            assert!(
                !description.contains("crates/"),
                "description for {key} leaks crates/ path: {description}"
            );
            assert!(
                !contains_milestone_id(description),
                "description for {key} contains milestone ID: {description}"
            );
            assert!(
                !contains_lesson_code(description),
                "description for {key} contains lesson code: {description}"
            );
            assert!(
                !description.contains("docs/code-review-lessons"),
                "description for {key} points at internal doc: {description}"
            );
        }
    }

    fn contains_milestone_id(s: &str) -> bool {
        // M\d+ but not part of a longer word (e.g. "MSG").
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'M' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                return true;
            }
            i += 1;
        }
        false
    }

    fn contains_lesson_code(s: &str) -> bool {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'L' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                return true;
            }
            i += 1;
        }
        false
    }

    /// F-03: every description is non-empty. An empty description renders
    /// as an empty card body in the Settings lane, which is a regression
    /// we want caught at unit-test time.
    #[test]
    fn descriptions_are_non_empty() {
        for (key, _ty, _default, _allowed, description) in KEY_DESCRIPTIONS {
            assert!(
                !description.trim().is_empty(),
                "description for {key} must be non-empty"
            );
        }
    }

    /// F-04: `keybinds.focus_content` is no longer in the schema — M200
    /// removed it from the user-rebindable set. Pin the negative.
    #[test]
    fn focus_content_is_not_in_schema() {
        for (key, _ty, _default, _allowed, _description) in KEY_DESCRIPTIONS {
            assert_ne!(
                *key, "keybinds.focus_content",
                "focus_content must not appear in KEY_DESCRIPTIONS after M200"
            );
        }
    }

    /// F-05: build_schema_report produces exactly 45 sorted entries with
    /// `$schema_version: "1.0"` at the top. The golden fixture relies on
    /// this shape.
    #[test]
    fn build_schema_report_emits_45_sorted_keys() {
        let report = build_schema_report();
        assert_eq!(report.schema_version, "1.0");
        assert_eq!(report.keys.len(), 45);
        let keys: Vec<&str> = report.keys.iter().map(|e| e.key.as_str()).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "schema keys must be sorted by key");
    }

    /// F-06: choice rows carry `allowed`; non-choice rows do not. The
    /// `#[serde(skip_serializing_if = "Option::is_none")]` gate depends
    /// on `allowed == None` for non-choice rows.
    #[test]
    fn choice_rows_carry_allowed_non_choice_rows_do_not() {
        let report = build_schema_report();
        for entry in &report.keys {
            match entry.ty.as_str() {
                "choice" => assert!(
                    entry.allowed.is_some(),
                    "choice row {} must carry `allowed`",
                    entry.key
                ),
                other => assert!(
                    entry.allowed.is_none(),
                    "non-choice row {} ({other}) must not carry `allowed`",
                    entry.key
                ),
            }
        }
    }

    // M201 AC-05 regression guard (cycle 2 F-03): assert that the schema's
    // emitted `default` values come from the canonical tables, not from
    // hardcoded fallback strings. The `KEY_DESCRIPTIONS` row carries a
    // fallback (typed `default` column in the source); the emit path in
    // `build_schema_report` MUST override that fallback for bool rows
    // (from `ProjectConfig::default()`) and for keybind rows (from
    // `KEYBIND_DEFAULTS`). If a future change drops either override, this
    // test catches it at unit-test time.
    //
    // Without this guard, a future change that hardcodes `default = "true"`
    // in `KEY_DESCRIPTIONS` would silently drift from
    // `ProjectConfig::default()` (e.g. after the next flag flip in the
    // `UiConfig` defaults) and the schema would lie about what the
    // canonical default is.
    #[test]
    fn schema_bool_defaults_match_project_config_default() {
        use crate::config::ProjectConfig;
        let report = build_schema_report();
        let cfg = ProjectConfig::default();

        // Pin the bool defaults the schema claims come from ProjectConfig::default().
        // Each (key, expected_default_str) tuple must match ProjectConfig::default().
        let expected: &[(&str, &str)] = &[
            ("ui.color", &cfg.ui.color.unwrap_or(true).to_string()),
            ("ui.hide_done", &cfg.ui.hide_done.unwrap_or(false).to_string()),
            (
                "ui.show_watch_tab",
                &cfg.ui.show_watch_tab.unwrap_or(false).to_string(),
            ),
            (
                "git.auto_commit",
                &cfg.git.auto_commit.unwrap_or(false).to_string(),
            ),
            (
                "git.commit_on_milestone_complete",
                &cfg.git.commit_on_milestone_complete.unwrap_or(false).to_string(),
            ),
            (
                "git.auto_push",
                &cfg.git.auto_push.unwrap_or(false).to_string(),
            ),
            (
                "agent.automation.commit_after_execute",
                &cfg.agent.automation.commit_after_execute.unwrap_or(false).to_string(),
            ),
            (
                "agent.automation.push_after_review",
                &cfg.agent.automation.push_after_review.unwrap_or(false).to_string(),
            ),
            (
                "workflow.plan.in_repo",
                &cfg.workflow.plan.in_repo.unwrap_or(true).to_string(),
            ),
            (
                "workflow.steps.code_review",
                &cfg.workflow.steps.code_review.unwrap_or(false).to_string(),
            ),
        ];

        let by_key: std::collections::BTreeMap<&str, &str> = report
            .keys
            .iter()
            .map(|e| (e.key.as_str(), e.default.as_str()))
            .collect();
        for (key, want) in expected {
            let got = by_key
                .get(key)
                .unwrap_or_else(|| panic!("schema missing bool row for {key}"));
            assert_eq!(
                got, want,
                "schema default for bool key {key} drifted from ProjectConfig::default(); \
                 the build_schema_report bool override was lost"
            );
        }
    }

    #[test]
    fn schema_keybind_defaults_match_keybind_defaults_table() {
        use crate::config::KEYBIND_DEFAULTS;
        use std::collections::BTreeMap;

        let report = build_schema_report();
        let table: BTreeMap<&str, &str> = KEYBIND_DEFAULTS.iter().copied().collect();

        // Every keybind row in the schema must pull its default from the
        // canonical KEYBIND_DEFAULTS table. Build a map of the schema's
        // emitted defaults and assert equality for every keybind row.
        let schema_keybinds: BTreeMap<&str, &str> = report
            .keys
            .iter()
            .filter(|e| e.ty == "keybind")
            .map(|e| (e.key.as_str(), e.default.as_str()))
            .collect();

        // First sanity check: the schema must cover every KEYBIND_DEFAULTS row
        // (modulo the action-name prefix).
        let expected_actions: Vec<&str> = table.keys().copied().collect();
        for action in &expected_actions {
            let full_key = format!("keybinds.{action}");
            assert!(
                schema_keybinds.contains_key(full_key.as_str()),
                "schema missing keybind row for {full_key}"
            );
        }

        // Now the actual regression guard: each schema keybind default must
        // equal the canonical KEYBIND_DEFAULTS chord (no hardcoded fallback
        // slipped through).
        for (key, got) in &schema_keybinds {
            let action = key
                .strip_prefix("keybinds.")
                .unwrap_or_else(|| panic!("keybind row {key} missing `keybinds.` prefix"));
            let want = table.get(action).unwrap_or_else(|| {
                panic!("KEYBIND_DEFAULTS table missing action `{action}` (schema row {key})")
            });
            assert_eq!(
                got, want,
                "schema default for {key} drifted from KEYBIND_DEFAULTS; \
                 the build_schema_report keybind override was lost"
            );
        }
    }
}
