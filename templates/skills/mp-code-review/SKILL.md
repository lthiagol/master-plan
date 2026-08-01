---
name: mp-code-review
description: Lesson-pattern code review — pre-screen changes against the runnable Pattern: blocks for L6/L8/L13/L14/L15 (and the broader L1–L63 catalog), file findings via `mp reviews finding add`, and pin regressions with grep/ripgrep fixtures. Use when reviewing a milestone's diff before stage-8 sign-off.
---

# mp-code-review — lesson-pattern review (repository-internal)

> **Repository-internal skill.** This skill is NOT part of the consumer
> catalog. Its runnable patterns are coupled to master-plan's own
> fixtures (`crates/mp/tests/code_review_patterns.rs`) and the
> lessons catalog (`crates/mp/tests/fixtures/code-review-lessons.md`); it
> does not deploy to adopters via `mp install`. The references to
> `M\d+` and `L\d+` tokens below are intentional and are part of the
> master-plan repo's internal vocabulary — they are NOT
> consumer-surface provenance.

This skill is the per-change pre-screen. It walks the lessons catalog
in `crates/mp/tests/fixtures/code-review-lessons.md` (L1–L63), runs the
runnable patterns for the lessons that have them (L6, L8, L13, L14, L15
per M173 S1), and files findings for any matches or suspected violations.
The canonical lessons live in that fixture file; the runnable patterns
live in `crates/mp/tests/code_review_patterns.rs`.

The skill is **read-only** — it never modifies code. It writes to the
plan via `mp reviews finding add` (audit trail) and surfaces matches
on stdout for the runner / coordinator to act on.

## When to load this skill

- Before signing off on a milestone at stage 8 (external review).
- Before any dogfood pass on the repo's own code (mp-dogfood-log).
- When a `mp reviews finding add --phase self` came back empty and you
  want a second pass with a different mental frame.

Do NOT load this skill for:

- Spec review (use `mp-coordinator`'s reviewing.md sub-mode).
- Execution review (that's the runner's stage-6 self-review with
  `mp reviews finding add --phase self`).
- Plan-shape or doc review (no runnable fixtures; not this skill's
  job).

## Lessons with runnable Pattern: blocks (M173 S1)

| Lesson | Pattern | Run command |
|--------|---------|-------------|
| L6 | Bulk paths MUST call the same gate as single-id paths | `cargo nextest run -p mp --test ac_update_bulk -E 'test(/bulk_update_unknown_ac_does_not_partial_apply/)' --no-fail-fast` |
| L8 | Validate operation-level args ONCE at dispatch | `cargo nextest run -p mp --test suite_milestone -E 'test(/bulk_validates_operation_level_args_up_front/)' --no-fail-fast` |
| L13 | `#[allow(dead_code)]` is a "look here next" flag | `rg '#\[allow\(dead_code\)\]' crates/mp/src/migrate.rs` (must return nothing) |
| L14 | No empty `if` branches | `rg -n 'if [^{]+\{[[:space:]]*(\}|/[[:space:]]*\*[^}]*\})' crates/mp/src/` (every match is a finding) |
| L15 | Spec clauses map to integration tests | `rg -A2 'AC-' master-plan/milestones/*.json \| rg -v 'manual:' \| rg 'verification'` then cross-reference each `verification` line against the `tests` block |

Each pattern block in the lessons file carries a Pattern / Positive
fixture / Negative fixture triple. The run command in the table above
is the Positive-fixture check; the Negative fixture is what to grep for
to surface violations.

## Pre-screen workflow

1. **Pull the milestone's diff.**
   `git diff --stat <base>..HEAD -- crates/ mp-dogfood-log.md docs/`
   Scope the review to changed files; the lesson patterns apply
   selectively.

2. **Walk the lessons catalog.** Open
   `crates/mp/tests/fixtures/code-review-lessons.md` and read the
   L6/L8/L13/L14/L15 sections. For each lesson, run the Positive-fixture
   command; if it fails, the lesson's contract has regressed and you
   have a finding.

3. **Grep for the Negative fixtures.** The Negative-fixture text in
   each Pattern block names the grep. Run it; matches are findings.

4. **Walk the wider L1–L63 catalog for non-runnable lessons.** Even
   without a fixture, L5 (the author should not be the only reviewer)
   and L15 (tests written against the implementation, not the spec)
   are must-checks on every diff. Read the lesson, decide if the
   diff matches the smell; if yes, file a finding.

5. **File findings.** Use
   `mp reviews finding add <id> --phase external --severity <sev>
    --category <lesson-id> --desc "..." [--file <path> --line <n>]`.
   The `--category` should be the lesson id (e.g. `L6`, `L8`) so the
   finding is searchable by lesson.

6. **Hand off.** Findings are the (c) hand-off payload (stage 8 → 9).
   The runner picks them up at stage 9 and remediates. See the
   `mp-runner` skill, "Remediate review findings" workflow.

## Severity rubric

| Severity | When |
|----------|------|
| high | The lesson's Positive-fixture command fails (the contract is broken in this diff) |
| medium | The Negative-fixture grep matches AND the match is in changed code (not pre-existing) |
| low | The Negative-fixture grep matches in pre-existing code (flag for backlog, not the current milestone) |
| info | The lesson's smell pattern is plausibly present but the match is borderline; record for context |

## What this skill is NOT

- Not a replacement for `mp-coordinator`'s reviewing.md sub-mode.
  Coordinator review is the broader stage-8 pass; this skill is the
  focused lesson-pattern pre-screen that runs *inside* the review
  session.
- Not a spec-co-design tool. Spec review uses `mp-coordinator`'s
  spec-co-design.md (with the optional `spec-grill` add-on).
- Not a runner-side self-review. The runner files `--phase self`
  findings at stage 6 with `mp-runner`'s executing.md sub-mode.
- Not a green-build gate. The Positive-fixture commands catch
  contract regressions, not behavioral bugs. The runner's own
  `cargo nextest run` is the behavioral gate.

## Embed / link map

This skill embeds (and references) the lessons catalog. The canonical
home is `crates/mp/tests/fixtures/code-review-lessons.md` (relative to
the repo root); the runnable fixtures are pinned in
`crates/mp/tests/code_review_patterns.rs`. Since this skill is
repository-internal and does not deploy to adopters, all paths here
are repo-relative. The lesson-contract check is:
`cargo nextest run -p mp --test code_review_patterns --no-fail-fast`.

## Install

This skill is **not** installable via `mp install` (manifest
`category: internal`). Load it from the repo tree
(`templates/skills/mp-code-review/`) when reviewing master-plan itself.

## See also

- `crates/mp/tests/fixtures/code-review-lessons.md` — lesson-pattern
  library (L1–L63; L6/L8/L13/L14/L15 have runnable Pattern: blocks).
- `crates/mp/tests/code_review_patterns.rs` — runnable fixtures for the
  M173 S1 lessons.
- `mp-coordinator` (M121) — coordinator role skill (stages 1-4, 8, 10-12).
- `mp-runner` (M122) — runner role skill (stages 5-7, 9).
- `mp-dogfood-log.md` — dogfood-log triage notes.
