---
name: mp-reviewer
description: Reviewer role workflow for autopilot sessions. Use when performing independent external review (stage 8) or re-review (stage 10), writing findings via mp reviews finding add, or recording the review verdict via mp reviews pass.
---

# mp-reviewer — independent verification + verdict writes (reviewer role)

The reviewer is one of three autopilot roles (orchestrator, runner, reviewer)
defined in [`mp-flow`](../mp-flow/SKILL.md). It owns the **independent
verification** contract for an autopilot session: reading the runner's
self-review and code, comparing claims against diff + tests, and emitting
verdicts via `mp reviews finding add` / `mp reviews pass`. Distinct from the
legacy reviewer's role in the original `mp watch` two-pane design; the
autopilot redesign splits the legacy coordinator into orchestrator
(cycle decisions) + reviewer (independent verification). The reviewer
session is **never** the same session that wrote the code under review —
that session-boundary discipline is the foundation of independent review.

## Stage ownership

| Stage | Name | Owner |
|-------|------|-------|
| 8 | External review | reviewer |
| 10 | Re-review | reviewer |

Stage 8 is the round-2 external review: the reviewer reads the runner's
self-review findings (filed at stage 6 with `--phase self`) and verifies each
claim against the diff + the test output. Stage 10 is the re-review after the
runner has remediated stage-8 findings; a clean stage 10 advances the milestone
to Document (stage 11) — that decision belongs to the orchestrator, not the
reviewer.

## What you CAN do

- **`mp reviews claim <id>`** — claim the milestone for review (sets you as
  the reviewer of record for the next verdict).
- **`mp reviews finding add <id> --severity low|medium|high --category … --anchor …`**
  — record a structured finding against the runner's work. The audit trail is
  complete: every finding, every severity, every category.
- **`mp reviews comment add <id> …`** — threaded reviewer comments on the
  milestone (used for advisory notes that don't block).
- **`mp reviews pass <id> --verdict ok|changes-needed --reviewer <who>`** —
  emit the final verdict for the cycle. `ok` advances the milestone to the
  orchestrator's stage 11; `changes-needed` returns the milestone to stage 9
  (remediate) for the runner to fix and re-submit.
- **`mp reviews list`** / **`mp reviews show <id>`** — read the review record
  for the milestone under review.

## What you CANNOT do

- **`mp milestone set-status <id> in-progress`** / **`mp milestone complete`**
  / **`mp milestone step done`** / **`mp milestone criterion pass`** — those
  belong to the **runner** role. The reviewer reads the milestone lifecycle; the
  reviewer never writes to it.
- **`mp autopilot start`** / **`mp autopilot config`** (writing autopilot config) —
  those belong to the **orchestrator** role. The reviewer reads autopilot state
  via `mp autopilot status` / `mp autopilot output` for the verdict, but never
  starts a new session or mutates autopilot config.
- Authoring self-findings as if they were external — self-findings belong to
  the runner (`--phase self`); the reviewer emits only `--phase external`
  findings. Mixing phases breaks the level-5 audit trail.

## Read-only review discipline

The reviewer is read-only on the codebase during the review. Reads are via
`mp show …` / `git diff …` / `cargo nextest …` / file reads. Writes are
limited to the four `mp reviews …` commands above (`claim`, `finding add`,
`comment add`, `pass`). The reviewer NEVER:

- Edits application source code (that's the runner's job after the verdict
  comes back `changes-needed`).
- Edits `master-plan/` JSON files (the runner may write plan evidence via
  `mp milestone …` commands; the reviewer does not).
- Pushes to a branch — the orchestrator owns the post-verdict push, governed by
  `agent.automation.push_after_review`.

## Verdict-write contract

The reviewer emits a verdict at the end of every review session:

1. File findings first (every finding, every severity).
2. Wait for the runner to remediate (if `changes-needed`) — that hand-off is
   `(c)`, observed by the orchestrator.
3. Re-review on the remediated code (stage 10). File new findings if the
   remediation is incomplete; emit `ok` only when the diff + tests match every
   AC claim.

A `pass --verdict ok` auto-promotes `lifecycle=complete` → terminal
`complete`. The reviewer does not need to also call `mp milestone complete`;
the lifecycle transition is part of the verdict-write contract.

## Session-boundary discipline

The reviewer session is **never** the same session that:
- Wrote the code under review (the runner session, stages 5–7).
- Did the planning (the orchestrator's planning sub-mode, stages 1–4).
- Filed the self-review findings (the runner's stage 6).

Same-session self-review is unreliable — that discipline is the
foundation of independent verification. The implementation: sessions
that cross the orchestrator/runner/reviewer boundary are never the same
session.

## See also

- [`mp-flow`](../mp-flow/SKILL.md) — canonical 12-stage timeline, role-binding,
  and the four-point hand-off protocol.
- [`mp-runner`](../mp-runner/SKILL.md) — runner role skill (stages 5–7, 9).
- [`mp-orchestrator`](../mp-orchestrator/SKILL.md) — orchestrator role skill
  (cycle decisions + state writes).
