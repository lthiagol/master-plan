---
name: mp-orchestrator
description: Orchestrator role workflow for autopilot sessions. Use when driving the mp autopilot cycle (claim → execute → review → remediate → complete), making cycle decisions, writing autopilot state, or handing off between panes in the 12-stage mp-flow timeline.
---

# mp-orchestrator — cycle decisions + state writes (orchestrator role)

The orchestrator is one of three autopilot roles (orchestrator, runner, reviewer) defined in
[`mp-flow`](../mp-flow/SKILL.md). It owns the cycle-decision contract for an autopilot
session: deciding what the runner executes next, advancing the milestone through
`complete` / `remediate` / `re-review`, and recording the cycle state in
`master-plan/autopilot/<id>/session.json`. Distinct from the legacy `coordinator`
role from the original `mp watch` two-pane design; the autopilot redesign splits
the legacy coordinator into orchestrator (cycle decisions) + reviewer
(independent verification).

## Stage ownership

| Stage | Name | Owner |
|-------|------|-------|
| 5 | Claim & execute | runner (orchestrator decides what to claim) |
| 6 | Self-review | runner |
| 7 | Complete | runner (orchestrator observes the transition) |
| 8 | External review | reviewer (orchestrator receives the verdict) |
| 9 | Remediate | runner (orchestrator forwards reviewer findings) |
| 10 | Re-review | reviewer (orchestrator advances on a clean verdict) |
| 11 | Document | orchestrator (closing stages) |
| 12 | Hand off | orchestrator |

Stages 5–9 form the per-cycle loop the orchestrator owns the **boundaries** of.
Stage 8 belongs to the reviewer (independent verification); the orchestrator never
plays the reviewer role in the same session. Stages 11–12 are the orchestrator's
closing domain (Document + Hand-off).

## What you CAN do

- `mp autopilot start <id> [<id>…] [--dry-run|--resume|--force]` — start a new
  autopilot session, spawning the orchestrator pane plus its sibling roles per the
  session's topology (`one-agent` / `two-agent` / `three-agent`).
- `mp autopilot status` / `mp autopilot stop` / `mp autopilot output` /
  `mp autopilot result` — control-plane reads against the live session.
- `mp autopilot config <get|set> autopilot.roles.<role>` — read or update
  per-role config (`orchestrator` / `runner` / `reviewer`).
- `mp milestone set-status <id> in-progress` — advance a milestone into the
  execution lane (the runner will pick it up; the orchestrator decides *when*).
- `mp milestone complete <id> --evidence "..."` — observe the runner's
  self-completion and let the review lane pick the milestone up. The orchestrator
  does NOT itself call `complete`; that is the runner's terminal write.
- `mp reviews handoff <id> --from-session <s> --to-session <r> ...` — record the
  hand-off at every stage boundary (data + session-boundary + evidence).
- `git push -u origin <branch>` after a clean reviewer verdict, IF
  `agent.automation.push_after_review` is `true` (project policy; consult, do
  not invent session-local push policy).

## What you CANNOT do

- **`mp reviews claim`** / **`mp reviews finding add`** / **`mp reviews pass`**
  / **`mp reviews finding resolve`** — those belong to the **reviewer** role.
  The orchestrator may receive findings (the reviewer emits them) but never
  authors them. Same-session self-review is unreliable; the orchestrator
  who drove the cycle must NOT also be the one to clear findings.
- `mp milestone criterion pass` — that is the **runner** role's per-AC stamp.
  The orchestrator may read AC evidence (`mp show milestone <id> --fields
  'acceptance_criteria[].evidence'`) but must not write it.
- `mp milestone complete <id>` as the author — same as above; that write belongs
  to the runner. The orchestrator reads the lifecycle transition.
- Reviewer-specific role binding (`mp autopilot config autopilot.roles.reviewer`)
  is off-limits; reviewer config is project policy and the orchestrator only
  consults it.

## Cycle-decision contract

The orchestrator drives the cycle by reading state and emitting hand-offs. The
deep-dive protocol — what data passes, what session-boundary each side enforces,
what evidence each side leaves — lives in `mp-flow`'s Hand-off protocol section.
At a high level, the orchestrator's per-cycle loop is:

1. **Read state**: `mp show milestone <id> --summary` and `mp autopilot status`.
2. **Decide**: claim → execute → self-review (runner owns) → external review
   (reviewer owns) → remediate (runner owns) → re-review (reviewer owns) →
   advance (this is the orchestrator's `advance_to` decision).
3. **Emit hand-off**: `mp reviews handoff <id> --from-session <s> --to-session
   <r> --data <json>` to record the boundary crossing.
4. **Observe + repeat** until stage 10 lands clean, then advance to stage 11
   (Document) and stage 12 (Hand off).

## State-write directives

The orchestrator writes to:
- `master-plan/autopilot/<id>/session.json` (the autopilot session state,
  through `mp autopilot` commands — never hand-edit).
- `master-plan/milestones/<id>.json` via `mp milestone set-status`,
  `mp milestone add-note`, and the hand-off record.

The orchestrator does NOT write to:
- The reviewer-only fields on the milestone (findings, verdicts) — those
  belong to the reviewer role and are written via `mp reviews ...` commands.
- The runner-only per-AC evidence (`mp milestone criterion pass`).

## Hand-off contract

The orchestrator hands work to the runner and the reviewer at the four points
in the 12-stage timeline. The four-point protocol (data, session-boundary,
evidence) lives in `mp-flow`'s Hand-off protocol section.

| Hand-off | From stage | To stage | Direction | Description |
|----------|-----------|----------|-----------|-------------|
| (a) | 4 (Approve) | 5 (Claim & execute) | orchestrator → runner | Approved spec + cycle policy |
| (b) | 7 (Complete) | 8 (External review) | runner → orchestrator → reviewer | Self-review + AC evidence |
| (c) | 8 (External review) | 9 (Remediate) | reviewer → orchestrator → runner | Reviewer findings for the runner to fix |
| (d) | 9 (Remediate) | 10 (Re-review) | runner → orchestrator → reviewer | Remediated code for re-verification |

## See also

- [`mp-flow`](../mp-flow/SKILL.md) — canonical 12-stage timeline, role-binding,
  and the four-point hand-off protocol.
- [`mp-runner`](../mp-runner/SKILL.md) — runner role skill (stages 5–7, 9).
- [`mp-reviewer`](../mp-reviewer/SKILL.md) — reviewer role skill (stages 8, 10).
