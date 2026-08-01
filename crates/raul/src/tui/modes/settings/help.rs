//! M168 BF-03: hard-coded per-key documentation prose for the Settings
//! pane help box. Each entry is the dotted key (e.g. `ui.color`) mapped
//! to a 2-3 line prose block shown above the flat list when the cursor
//! rests on that key. The key list is the single source of truth in
//! `super::SETTINGS_KEYS`; this table is documentation only.

/// (key, prose) — prose is two lines. `&'static` so the table lives
/// in `.rodata` and the renderer can borrow without lifetime juggling.
pub const HELP: &[(&str, &[&str])] = &[
    // ui
    (
        "ui.color",
        &[
            "ANSI color for status, decisions, and read paths.",
            "Drives `paint_for_role` in `crates/raul/src/config.rs`.",
        ],
    ),
    (
        "ui.icons",
        &[
            "Glyph set: `none`, `ascii`, or `unicode`.",
            "Picked up at startup; reload to apply.",
        ],
    ),
    (
        "ui.theme",
        &[
            "Theme palette name. `mocha | latte | frappe | macchiato`.",
            "Picked up at startup.",
        ],
    ),
    (
        "ui.hide_done",
        &[
            "Hide done milestones in lists.",
            "Toggle the bottom-right `h` key in any list lane to flip at runtime.",
        ],
    ),
    // workflow
    (
        "workflow.profile",
        &[
            "Profile selects which gates / steps apply at plan time.",
            "Full profile requires every gate; tweak per project.",
        ],
    ),
    (
        "workflow.plan.location",
        &[
            "Plan directory. `in_repo` true means master-plan/ next to Cargo.toml.",
            "Changing this requires re-running `mp init`.",
        ],
    ),
    (
        "workflow.plan.in_repo",
        &[
            "Boolean: store plan under the project root vs `$MP_HOME`.",
            "When false, the same plan is shared across all projects.",
        ],
    ),
    (
        "workflow.gates.strictness",
        &[
            "Gate enforcement level. `full` is the default.",
            "Looser strictness skips selected gates; not for production use.",
        ],
    ),
    (
        "workflow.steps.code_review",
        &[
            "Optional review step between execution and verification.",
            "Off by default; flip on for milestones that warrant a second pass.",
        ],
    ),
    // git
    (
        "git.auto_commit",
        &[
            "Auto-commit on milestone close.",
            "Otherwise `mp` stages changes but leaves the commit to you.",
        ],
    ),
    (
        "git.commit_on_milestone_complete",
        &[
            "Auto-commit when a milestone reaches `complete`.",
            "Subsumed by `auto_commit` when both are on; off by default.",
        ],
    ),
    (
        "git.auto_push",
        &[
            "Auto-push after auto-commit. Requires `git.auto_commit` to be on.",
            "Off by default; flip on for solo projects.",
        ],
    ),
    // next
    (
        "next.prefer",
        &[
            "Forward lane when both are eligible: `milestone` or `track`.",
            "Trivial preference; choose whichever matches your workflow.",
        ],
    ),
    // agent
    (
        "agent.automation.commit_after_execute",
        &[
            "Commit after an agent executes a milestone.",
            "On by default; agents rebase their own branches before commit.",
        ],
    ),
    (
        "agent.automation.push_after_review",
        &[
            "Push after `mp reviews pass`.",
            "On by default; off if you only run reviews locally.",
        ],
    ),
    (
        "agent.automation.branch_strategy",
        &[
            "Branch naming for executor branches.",
            "Sensible default; only change if your CI has a stricter policy.",
        ],
    ),
    (
        "agent.automation.auto_remediate",
        &[
            "Auto-fix review findings (where the tool can).",
            "Off by default; requires explicit opt-in.",
        ],
    ),
    // keybinds (subset; full list lives in `crates/raul/src/tui/keybinds.rs`)
    (
        "keybinds.quit",
        &[
            "Key combo for `Action::Quit`. Default `q, Q`.",
            "Conflicts with the lane navigation prefix? Add a binding here.",
        ],
    ),
    (
        "keybinds.up",
        &[
            "Key combo for `Up` (one row back). Default `Up, k`.",
            "Tied to the `List` / `Table` / `Paragraph` selection. Change with care.",
        ],
    ),
    (
        "keybinds.down",
        &[
            "Key combo for `Down` (one row forward). Default `Down, j`.",
            "Tied to the `List` / `Table` / `Paragraph` selection. Change with care.",
        ],
    ),
    (
        "keybinds.enter",
        &[
            "Drill-in / confirm. Default `Enter`.",
            "In Settings, opens the field-edit popup for the focused key.",
        ],
    ),
    (
        "keybinds.escape",
        &[
            "Back / cancel. Default `Esc`.",
            "In Settings, closes the field-edit popup if open, else closes the modal.",
        ],
    ),
    (
        "keybinds.help",
        &[
            "Open the help overlay. Default `?`.",
            "Pop-up help on top of any lane.",
        ],
    ),
];

/// Default prose for keys not in the `HELP` table. Two lines so the
/// help box layout stays stable (no variable line count).
pub const DEFAULT_PROSE: &[&str] = &[
    "Configuration key handled by `mp config set` / `mp config get`.",
    "Edit the value above and press Enter to commit (or Esc to cancel).",
];

/// Look up the prose for a key, falling back to `DEFAULT_PROSE`.
pub fn help_for(key: &str) -> &'static [&'static str] {
    HELP.iter()
        .find(|(k, _)| *k == key)
        .map(|(_, prose)| *prose)
        .unwrap_or(DEFAULT_PROSE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_for_returns_prose_for_known_key() {
        let prose = help_for("ui.color");
        assert_eq!(prose.len(), 2);
        assert!(prose[0].contains("ANSI"));
    }

    #[test]
    fn help_for_returns_default_for_unknown_key() {
        let prose = help_for("nonexistent.key");
        assert_eq!(prose, DEFAULT_PROSE);
    }
}
