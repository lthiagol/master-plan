//! M147 AC-05 + AC-06: skill contract — `mp-runner` and `mp-coordinator`
//! read the four `[agent.automation]` knobs at the right handoff
//! boundaries and branch their behavior on the values.
//!
//! These tests are read-only against the skill files; they exist so a
//! future refactor that drops the consult-and-act discipline fails the
//! gate before the skill ships to a harness.

use std::path::PathBuf;

fn skill_path(skill: &str, file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../templates/skills")
        .join(skill)
        .join(file)
}

/// Read the skill file as a string. Tests below assert on substrings so
/// a refactor that removes the numbered consult step fails the gate
/// before the change merges.
fn read_skill(skill: &str, file: &str) -> String {
    let p = skill_path(skill, file);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// F-03 / F-04 / F-05 helper assertion: `first` appears in `body` at a
/// byte offset strictly earlier than `second`. Used to pin the
/// sequencing the skills document (e.g., "consult `branch_strategy`
/// BEFORE `mp milestone set-status`", or "`mp milestone complete`
/// BEFORE `git add -A`"). The helper fails fast with both substrings
/// quoted if either is missing, and the failure message embeds the full
/// body so the regression is unambiguous. The `context` label is shown
/// in every panic message — pass a stable identifier like the file
/// name plus the ordering pair so a regression in either file is
/// attributable from the failure alone.
fn assert_ordered(body: &str, first: &str, second: &str, context: &str) {
    let a = body
        .find(first)
        .unwrap_or_else(|| panic!("{context}: required substring not found: {first:?}"));
    let b = body
        .find(second)
        .unwrap_or_else(|| panic!("{context}: required substring not found: {second:?}"));
    assert!(
        a < b,
        "{context}: expected {first:?} before {second:?}, found at offsets {a} and {b} respectively; full body:\n{body}"
    );
}

/// AC-05: `mp-runner/SKILL.md` documents the `commit_after_execute` and
/// `branch_strategy` consult steps at the (b) and (a) hand-off points.
#[test]
fn skill_runner_skill_md_documents_automation_handoffs() {
    let s = read_skill("mp-runner", "SKILL.md");
    for required in [
        // Consumer surface: no internal milestone IDs in headings.
        "## Automation handoffs",
        // Both knobs are named in the runner's domain.
        "agent.automation.commit_after_execute",
        "agent.automation.branch_strategy",
        // The runner's claim-time consult is documented.
        "branch_strategy",
        // The runner explicitly does NOT push.
        "push is the coordinator's knob",
    ] {
        assert!(
            s.contains(required),
            "mp-runner SKILL.md must contain {required:?}; full body follows:\n{s}"
        );
    }
}

/// AC-05: `mp-runner/executing.md` (the stage 5/6/7 deep-dive) carries
/// the consult steps inline so a session deep-linking into the file
/// still sees them.
#[test]
fn skill_runner_executing_md_documents_branch_and_commit_consult() {
    let s = read_skill("mp-runner", "executing.md");
    for required in [
        "agent.automation.branch_strategy",
        "agent.automation.commit_after_execute",
        // Stage 5 consult and stage 7 consult are both present and
        // distinguishable — file uses "Stage 5" / "Stage 7" headings.
        "Stage 5:",
        "Stage 7:",
    ] {
        assert!(
            s.contains(required),
            "mp-runner/executing.md must contain {required:?}; full body:\n{s}"
        );
    }
}

/// AC-06: `mp-coordinator/SKILL.md` documents the `push_after_review` and
/// `auto_remediate` consult steps at the (c) and stage-8 hand-off points.
#[test]
fn skill_coordinator_skill_md_documents_automation_handoffs() {
    let s = read_skill("mp-coordinator", "SKILL.md");
    for required in [
        // Consumer surface: no internal milestone IDs in headings.
        "## Automation handoffs",
        "agent.automation.push_after_review",
        "agent.automation.auto_remediate",
        // Stage 8 review gets the threshold consult.
        "Stage 8 — External review (auto_remediate)",
        // (c) hand-off consult is named; the coordinator owns the push.
        "Stage 8 → Stage 9 hand-off (push_after_review)",
        // The four thresholds are mapped to actions.
        "`none`",
        "`low` / `all`",
        "`medium`",
        "`high`",
    ] {
        assert!(
            s.contains(required),
            "mp-coordinator SKILL.md must contain {required:?}; full body:\n{s}"
        );
    }
}

/// AC-06: `mp-coordinator/reviewing.md` (the stage 8/10 deep-dive)
/// surfaces the consult steps inline so a reviewer session deep-linking
/// into the file sees the threshold on first read.
#[test]
fn skill_coordinator_reviewing_md_documents_threshold_and_push_consult() {
    let s = read_skill("mp-coordinator", "reviewing.md");
    for required in [
        "agent.automation.auto_remediate",
        "agent.automation.push_after_review",
        // Recording every finding is the audit-trail invariant.
        "Every finding is recorded unconditionally",
        // Auto-remediate vs record-only is named in the checklist.
        "auto_remediate",
        "record_only",
    ] {
        assert!(
            s.contains(required),
            "mp-coordinator/reviewing.md must contain {required:?}; full body:\n{s}"
        );
    }
}

/// Defense in depth: the runner does NOT document the push knob —
/// pushing is the coordinator's responsibility. If a future refactor
/// accidentally moves the push into the runner's domain, this fails.
#[test]
fn skill_runner_skill_md_does_not_consult_push_after_review() {
    let s = read_skill("mp-runner", "SKILL.md");
    assert!(
        !s.contains("agent.automation.push_after_review"),
        "mp-runner must not consult push_after_review — that knob belongs to mp-coordinator"
    );
    let executing = read_skill("mp-runner", "executing.md");
    assert!(
        !executing.contains("agent.automation.push_after_review"),
        "mp-runner/executing.md must not consult push_after_review"
    );
}

/// Defense in depth: the coordinator does NOT document the
/// `commit_after_execute` knob — that knob belongs to the runner. If a
/// future refactor moves commit into the coordinator, this fails.
#[test]
fn skill_coordinator_skill_md_does_not_consult_commit_after_execute() {
    let s = read_skill("mp-coordinator", "SKILL.md");
    assert!(
        !s.contains("agent.automation.commit_after_execute"),
        "mp-coordinator must not consult commit_after_execute — that knob belongs to mp-runner"
    );
    let reviewing = read_skill("mp-coordinator", "reviewing.md");
    assert!(
        !reviewing.contains("agent.automation.commit_after_execute"),
        "mp-coordinator/reviewing.md must not consult commit_after_execute"
    );
}

/// Threshold semantics show up in both the SKILL.md and reviewing.md so
/// a reviewer who only reads the deep-dive file is reminded that the
/// ordering is `none < low < medium < high` (M147 AC-03 / AC-06
/// connection).
#[test]
fn skill_coordinator_threshold_ordering_is_documented() {
    let s = read_skill("mp-coordinator", "SKILL.md");
    for required in [
        "none < low < medium < high",
        "`all` aliasing `low`",
        "SeverityRank",
    ] {
        assert!(
            s.contains(required),
            "mp-coordinator SKILL.md must carry the threshold ordering contract {required:?}; body:\n{s}"
        );
    }
}

/// F-03: `mp-coordinator/reviewing.md` instructs reviewers to file
/// findings in the canonical `low|medium|high` severity vocabulary that
/// `SeverityRank::from_config_value` recognizes (config.rs accepts
/// `none|low|all|medium|high`, with `all` aliased to `low` for the
/// config-side `auto_remediate` value). The stale `blocker|major|minor|nit`
/// vocabulary would silently map to `SeverityRank::None` via the
/// catch-all in `from_config_value`, defeating the M147 AC-06
/// threshold. The forbidden-substring check pins the fix: any future
/// refactor that reverts to the stale vocabulary fails this gate
/// before merge.
#[test]
fn skill_coordinator_reviewing_md_uses_canonical_severity_vocabulary() {
    let s = read_skill("mp-coordinator", "reviewing.md");
    for required in [
        "`low`",
        "`medium`",
        "`high`",
        "SeverityRank",
        "auto_remediate",
    ] {
        assert!(
            s.contains(required),
            "mp-coordinator/reviewing.md must reference canonical severity vocabulary {required:?}; full body:\n{s}"
        );
    }
    for forbidden in [
        "`blocker`",
        "`major`",
        "`minor`",
        "`nit`",
        "blocker|major|minor|nit",
        "blocker, `major`, `minor`, `nit`",
    ] {
        assert!(
            !s.contains(forbidden),
            "mp-coordinator/reviewing.md must not use the stale severity label {forbidden:?} (SeverityRank::from_config_value only parses none|low|all|medium|high, anything else maps to None); full body:\n{s}"
        );
    }
}

/// F-03: `mp-flow/SKILL.md` describes the hand-off (c) data contract
/// (severity-ordered findings) using the canonical vocabulary, not
/// the stale `blocker/major first` wording the original M147 file
/// used. `mp-flow` is what every coordinator and runner session loads,
/// so the hand-off vocabulary has to be canonical here.
#[test]
fn skill_mp_flow_skill_md_uses_canonical_severity_vocabulary() {
    let s = read_skill("mp-flow", "SKILL.md");
    for required in ["`high`", "`medium`", "`low`", "SeverityRank"] {
        assert!(
            s.contains(required),
            "mp-flow SKILL.md must reference canonical severity {required:?} in the hand-off contract; full body:\n{s}"
        );
    }
    for forbidden in ["blocker/major first", "blocker|major", "`blocker`"] {
        assert!(
            !s.contains(forbidden),
            "mp-flow SKILL.md must not use stale severity wording {forbidden:?} (SeverityRank only knows low|medium|high + config alias all); full body:\n{s}"
        );
    }
}

/// F-04: in `mp-runner/executing.md` Stage 5 Execution order, the
/// `branch_strategy` consult MUST run before
/// `mp milestone set-status <id> in-progress`. The original M147 file
/// flipped this (claim at step 1, consult at step 2), which stranded
/// the first plan write of a `per-milestone` switch on the old
/// branch. `assert_ordered` pins the corrected sequence so any future
/// reorder fails this gate before merge. Scope is the Execution
/// order section only — the canonical commands table earlier in the
/// file names `set-status` as command #1 by definition, not by
/// ordering vs the consult.
#[test]
fn skill_runner_branch_strategy_consult_runs_before_claim() {
    let executing = read_skill("mp-runner", "executing.md");
    let exec_order_start = executing
        .find("### Execution order")
        .expect("mp-runner/executing.md must contain an 'Execution order' section");
    let exec_order = &executing[exec_order_start..];
    assert_ordered(
        exec_order,
        "agent.automation.branch_strategy",
        "set-status <id> in-progress",
        "mp-runner/executing.md Stage 5 Execution order (F-04: consult must precede claim)",
    );
}

/// F-04: portable base branch. The runner docs MUST NOT hard-code a
/// `from \`main\`` substring — repositories whose default branch is
/// `master` (like this one) or any other name would silently fail.
/// The runner must document the portable base: current `HEAD`, or the
/// discovered default branch (`git symbolic-ref refs/remotes/origin/HEAD`)
/// only when the discovery is unambiguous. The forbidden-substring
/// check pins the fix across BOTH runner docs; the positive primitive
/// check (HEAD / default branch / symbolic-ref) only applies to
/// `SKILL.md` because `executing.md` legitimately delegates base
/// selection to the SKILL.md via "see SKILL.md → Automation handoffs".
#[test]
fn skill_runner_branch_base_is_portable_no_hardcoded_main() {
    for (file, label) in [
        ("SKILL.md", "mp-runner SKILL.md"),
        ("executing.md", "mp-runner/executing.md"),
    ] {
        let s = read_skill("mp-runner", file);
        for forbidden in [
            "from `main`",
            "cut from main",
            "from `master`",
            "based on `main`",
            "from main before",
        ] {
            assert!(
                !s.contains(forbidden),
                "{label} must not hard-code a base branch with the substring {forbidden:?} (portability — repositories like this one default to master, not main); full body:\n{s}"
            );
        }
    }
    // Positive primitive check on the SKILL.md only — the canonical
    // base-policy table lives there. `executing.md` delegates via "see
    // SKILL.md → Automation handoffs" and does not need to repeat the
    // primitives.
    let skill = read_skill("mp-runner", "SKILL.md");
    assert!(
        skill.contains("HEAD") || skill.contains("default branch") || skill.contains("symbolic-ref"),
        "mp-runner SKILL.md must reference a portable base primitive (HEAD, default branch, or symbolic-ref) so the forbidden-string check cannot pass by accident after a wholesale rewrite that drops base guidance; full body:\n{skill}"
    );
}

/// F-04: the `mp-runner/SKILL.md` per-milestone row in the Automation
/// handoffs table states the cut runs `before claiming`. This pins the
/// ordering discipline the table embodies so a future refactor of the
/// row text cannot silently relax it.
#[test]
fn skill_runner_skill_md_per_milestone_row_keeps_before_claiming_discipline() {
    let skill = read_skill("mp-runner", "SKILL.md");
    let per_milestone_idx = skill
        .find("per-milestone")
        .expect("mp-runner SKILL.md must document the per-milestone branch_strategy value");
    // Look only at the row text from the per-milestone label onwards,
    // not the whole file — the discipline must be in the row.
    let around_per_milestone = &skill[per_milestone_idx..];
    assert!(
        around_per_milestone.contains("before claiming"),
        "mp-runner SKILL.md per-milestone row must state 'before claiming' so the consult-and-branch discipline is unambiguous (F-04 fix); body around the row:\n{around_per_milestone}"
    );
}

/// F-05: in the runner docs, `mp milestone complete` runs BEFORE
/// `git add -A && git commit` when `commit_after_execute=true`. This
/// guards against committing failed work (a `complete` that rejects on
/// red AC verification) and against leaving the lifecycle / evidence
/// mutations written by `complete` outside the automated commit.
/// `assert_ordered` pins the corrected sequence in both runner files.
#[test]
fn skill_runner_complete_precedes_git_commit_when_commit_after_execute() {
    let skill = read_skill("mp-runner", "SKILL.md");
    let executing = read_skill("mp-runner", "executing.md");
    assert_ordered(
        &skill,
        "mp milestone complete",
        "git add -A",
        "mp-runner SKILL.md ordering (F-05: complete must precede git commit)",
    );
    assert_ordered(
        &executing,
        "mp milestone complete",
        "git add -A",
        "mp-runner/executing.md ordering (F-05: complete must precede git commit)",
    );
}

/// F-05 (defense in depth): both runner docs must say the commit
/// happens *after* the confirmed `complete` lifecycle, not before.
/// Pinning the word "after" alongside the `commit_after_execute`
/// consult keeps the ordering discipline visible to a reviewer who
/// only skims the file.
#[test]
fn skill_runner_documents_commit_after_complete_not_before() {
    for (file, label) in [
        ("SKILL.md", "mp-runner SKILL.md"),
        ("executing.md", "mp-runner/executing.md"),
    ] {
        let s = read_skill("mp-runner", file);
        // The "before" wording is forbidden in the F-05 fix — the
        // runner docs no longer tell agents to commit *before*
        // calling `complete`.
        for forbidden in [
            "before calling `complete`",
            "then stage 7's `mp milestone complete`",
            "commit ... then stage 7's",
        ] {
            assert!(
                !s.contains(forbidden),
                "{label} must not describe commit-before-complete ordering ({forbidden:?}); full body:\n{s}"
            );
        }
    }
}

/// Fixture-driven end-to-end check: a project with the four knobs set
/// to non-default values drives the documented behavior across both
/// skill's consult steps. Runs the same `mp config get` surface a
/// runner / coordinator session would call, then asserts that the
/// values round-trip through the JSON contract.
#[test]
fn skill_contract_automation_fixture_drives_documented_behavior() {
    use std::process::Command;
    let cwd = tempfile::TempDir::new().expect("temp");
    let mp = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/mp");
    let init = Command::new(&mp)
        .args(["init", "--profile", "full", "--format", "json"])
        .current_dir(cwd.path())
        .output()
        .expect("mp init");
    assert!(init.status.success(), "mp init failed");

    for (key, value) in [
        ("agent.automation.commit_after_execute", "true"),
        ("agent.automation.push_after_review", "true"),
        ("agent.automation.branch_strategy", "per-milestone"),
        ("agent.automation.auto_remediate", "medium"),
    ] {
        let set = Command::new(&mp)
            .args(["config", "set", key, value, "--format", "json"])
            .current_dir(cwd.path())
            .output()
            .expect("mp config set");
        assert!(
            set.status.success(),
            "set {key}={value} failed: {}",
            String::from_utf8_lossy(&set.stderr)
        );
    }

    // Read back via the same surface the skills describe. Bools
    // surface as JSON booleans (not strings) — assert each value as
    // its expected JSON shape rather than coercing to a string.
    let expectations: &[(&str, serde_json::Value)] = &[
        (
            "agent.automation.commit_after_execute",
            serde_json::json!(true),
        ),
        (
            "agent.automation.push_after_review",
            serde_json::json!(true),
        ),
        (
            "agent.automation.branch_strategy",
            serde_json::json!("per-milestone"),
        ),
        (
            "agent.automation.auto_remediate",
            serde_json::json!("medium"),
        ),
    ];
    for (key, expected) in expectations {
        let get = Command::new(&mp)
            .args(["config", "get", key, "--format", "json"])
            .current_dir(cwd.path())
            .output()
            .expect("mp config get");
        assert!(get.status.success(), "get {key} failed");
        let v: serde_json::Value = serde_json::from_slice(&get.stdout).unwrap();
        assert_eq!(
            v["value"], *expected,
            "{key} must round-trip to {expected}; got {v:?}"
        );
    }
}
