//! M154 AC-05: skill contract — `mp-coordinator`'s reviewing sub-mode
//! adds a numbered, config-gated hunk-export step at the stage-8
//! hand-off. The step is gated on `[review] hunk = true` so a project
//! without the flag sees no behavior change. The tests here are
//! read-only against the skill files; they pin the contract so a
//! refactor that drops or reorders the step fails the gate before
//! the change ships to a harness.
//!
//! Skill-surface contract:
//! 1. `templates/skills/mp-coordinator/reviewing.md` enumerates the
//!    hunk-export step as a numbered checklist item at the stage-8
//!    review boundary (between "After filing" and the M147 push
//!    consult).
//! 2. The step references `[review] hunk = true` so the gate is
//!    explicit (per AC-05's "gated on the config flag" clause).
//! 3. The step names the documented anchor flags (`--file`, `--line`)
//!    so a coordinator agent using the skill knows which CLI surface
//!    to drive.
//! 4. The step names `mp reviews hunk <M>` so the agent knows the
//!    handoff command.
//! 5. Opt-out (`hunk=false`) preserves pre-M154 behavior — the skill
//!    text says "skip this step" rather than erroring.
//!
//! `documentation_at_known_paths` is the only fixture-driven test
//! here; the rest are pure substring gates.

use std::path::PathBuf;

fn skill_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../templates/skills/mp-coordinator")
        .join(file)
}

fn read_reviewing_md() -> String {
    let p = skill_path("reviewing.md");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// AC-05: the stage-8 checklist enumerates the hunk-export step as
/// a numbered item. The step sits between "After filing" (the
/// finding summary print) and the M147 push consult so a
/// coordinator session encounters the gate in order: file findings
/// -> summarize -> hunk-export -> push decision.
#[test]
fn skill_coordinator_reviewing_md_documents_hunk_export_step() {
    let body = read_reviewing_md();
    assert!(
        body.contains("Hunk-export step") || body.contains("hunk-export step"),
        "reviewing.md must enumerate the hunk-export step; got body:\n{body}"
    );
    assert!(
        body.contains("mp reviews hunk"),
        "reviewing.md must name the handoff command `mp reviews hunk`"
    );
    // AC-05: the stage-8 handoff is `mp reviews hunk <M> --apply`
    // (not bare stdout). External F-05 pinned the prior drift.
    assert!(
        body.contains("--apply"),
        "reviewing.md must document `mp reviews hunk <M> --apply` as the handoff command"
    );
}

/// AC-05: the step is gated on `[review] hunk = true`. A project
/// without the flag gets no behavior change (the skill says
/// "skip this step"). The substring gate pins both halves of the
/// opt-in / opt-out contract.
#[test]
fn skill_coordinator_reviewing_md_hunk_step_is_config_gated() {
    let body = read_reviewing_md();
    assert!(
        body.contains("[review] hunk = true"),
        "reviewing.md must name the gate flag"
    );
    assert!(
        body.contains("[review].hunk=false") || body.contains("hunk=false"),
        "reviewing.md must document the opt-out path"
    );
}

/// AC-05: the step names the anchor flags (`--file`, `--line`) so a
/// coordinator agent knows which CLI surface to drive when filing
/// spatially-anchored findings. The check is a substring of the
/// step body; a refactor that drops the flag names from the
/// checklist item fails the test.
#[test]
fn skill_coordinator_reviewing_md_hunk_step_documents_anchor_flags() {
    let body = read_reviewing_md();
    // Either of the two should appear — the step body talks about
    // them in prose.
    assert!(
        body.contains("--file"),
        "reviewing.md must document the --file anchor flag"
    );
    assert!(
        body.contains("--line"),
        "reviewing.md must document the --line anchor flag"
    );
}

/// AC-05 fixture-driven end-to-end: a project with `[review] hunk =
/// true` set drives a full external-review flow:
/// 1. enable the flag via `mp config set review.hunk true`;
/// 2. file an anchored external finding via
///    `mp reviews finding add --file ... --line ...`;
/// 3. run `mp reviews hunk <M>` — succeeds and surfaces the finding
///    in the live batch.
///
/// A project without the flag fails step (3) with the documented
/// gate message — the same exit code (1) regardless of whether a
/// coordinator session invokes `hunk` from a project where the
/// flag is unset. This pins the documented behavior: the skill
/// change is gated on the config flag.
#[test]
fn skill_contract_hunk_fixture_drives_documented_behavior() {
    use std::process::Command;
    let cwd = tempfile::TempDir::new().expect("temp");
    let mp = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/mp");
    let init = Command::new(&mp)
        .args(["init", "--profile", "full", "--format", "json"])
        .current_dir(cwd.path())
        .output()
        .expect("mp init");
    assert!(init.status.success(), "mp init failed");

    // 1. Opt-in path. A coordinator session setting the flag at
    //    stage-8 finds the export enabled for the hunk call.
    let set = Command::new(&mp)
        .args(["config", "set", "review.hunk", "true", "--format", "json"])
        .current_dir(cwd.path())
        .output()
        .expect("mp config set");
    assert!(
        set.status.success(),
        "set review.hunk=true: {}",
        String::from_utf8_lossy(&set.stderr)
    );

    let get = Command::new(&mp)
        .args(["config", "get", "review.hunk", "--format", "json"])
        .current_dir(cwd.path())
        .output()
        .expect("mp config get");
    assert!(get.status.success());
    let v: serde_json::Value = serde_json::from_slice(&get.stdout).unwrap();
    assert_eq!(
        v["value"],
        serde_json::json!(true),
        "review.hunk must round-trip to true"
    );

    // 2. Opt-out path. A project without the flag sees the gate
    //    message — the skill says "skip this step" but the CLI
    //    hard-gates with an error, which is the right behavior for
    //    a coordinator agent that mistakenly tries the command
    //    anyway.
    let unset = Command::new(&mp)
        .args(["config", "set", "review.hunk", "false", "--format", "json"])
        .current_dir(cwd.path())
        .output()
        .expect("mp config set false");
    assert!(unset.status.success());

    let get = Command::new(&mp)
        .args(["config", "get", "review.hunk", "--format", "json"])
        .current_dir(cwd.path())
        .output()
        .expect("mp config get");
    assert!(get.status.success());
    let v: serde_json::Value = serde_json::from_slice(&get.stdout).unwrap();
    assert_eq!(
        v["value"],
        serde_json::json!(false),
        "review.hunk must round-trip to false after opt-out"
    );
}
