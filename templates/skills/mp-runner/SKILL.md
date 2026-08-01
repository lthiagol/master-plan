---
name: mp-runner
description: Runner role workflow for Master Plan milestones. Use when executing approved steps, self-reviewing, completing milestones, or remediating external review findings in the mp-flow lifecycle.
---

# mp-runner — executing + fixing (runner role)

The runner is one of two agent roles in the two-agent / four-role architecture
(`mp-flow`). The runner owns the execution domain (stages 5-7) and the
remediation domain (stage 9). When an agent session loads `mp-flow` + `mp-runner`,
it is in the runner role.

## Stage ownership

| Stage | Name | Owner |
|-------|------|-------|
| 5 | Claim & execute | runner |
| 6 | Self-review | runner |
| 7 | Complete | runner |
| 9 | Remediate | runner |

Stages 5-7 are the execution domain: Claim & execute (set in-progress, run steps),
Self-review (round-1 self-review, file self-findings), Complete (mark done, hand
off to coordinator for round-2 external review). Stage 9 is the remediation domain:
read the coordinator's findings, patch the code, mark resolved, re-verify.

The coordinator owns stages 1-4, 8, 10, 11, 12 (spec authoring, external review,
re-review, closing). See `mp-coordinator` for the coordinator's domain.

## Sub-mode map

Two sub-modes match the runner's stage ownership:

| Sub-mode | Stages | Deep-dive |
|----------|--------|-----------|
| Executing | 5, 6, 7 | [executing.md](executing.md) |
| Fixing | 9 | [fixing.md](fixing.md) |

Plus a cross-cutting contract that applies to both sub-modes:
[atomic-writes.md](atomic-writes.md) (advisory flock lock + per-AC killpg timeout).

The executing sub-mode covers the four canonical execute commands: claim (set-status
in-progress), step done, criterion pass, complete — plus self-review at stage 6
(`mp reviews finding add --phase self`). The fixing
sub-mode covers the fix cycle after the coordinator's round-2 review.

## Two-round review (runner's side)

Round 1 is the runner's self-review at stage 6 (file findings with `--phase self`).
Round 2 is the coordinator's external review at stage 8.

The runner's self-review at stage 6 is a useful first pass — not the final one.
The same session that wrote the code cannot be the sole reviewer — that
discipline forces a session reset at every hand-off. The runner flags
self-detected issues via `mp reviews finding add <id> --phase self`.

The runner session that produced the code (stages 5-7) is NOT the same session
that fixes the coordinator's findings at stage 9. A fresh runner session picks
up the findings and remediates. The session-boundary protocol is documented
inline in `mp-flow`'s Hand-off protocol section.

## Evidence hygiene

Evidence is a per-run audit record. The `evidence` field on each AC is
written once during `mp milestone complete` and is never silently overwritten.

- `mp milestone ac criterion pass <id> <ac-id> --evidence "..."` stamps per-AC
  run evidence.
- `mp milestone complete <id> --evidence "..."` stamps the milestone-level
  verification block and sets AC statuses to `passed`. If an AC already has
  evidence and you need to overwrite it, pass `--force`.
- After complete, the lifecycle transitions to **`complete`** (terminal). The
  session ends only after `mp show milestone <id> --fields 'milestone.lifecycle'`
  reads `complete`.

## Hand-off contract to the coordinator

The runner hands work to the coordinator at four points in the 12-stage timeline.
The deep-dive protocol (what data passes, what session-boundary, what evidence)
lives in `mp-flow`'s Hand-off protocol section. At a high level:

| Hand-off | From stage | To stage | Direction | Description |
|----------|-----------|----------|-----------|-------------|
| (a) | 4 (Approve) | 5 (Claim & execute) | coordinator → runner | Approved spec with AC verification integrity report |
| (b) | 7 (Complete) | 8 (External review) | runner → coordinator | `lifecycle=complete` + self-findings (`mp reviews finding list`) |
| (c) | 8 (External review) | 9 (Remediate) | coordinator → runner | External findings for the runner to fix |
| (d) | 9 (Remediate) | 10 (Re-review) | runner → coordinator | Remediated code + finding resolutions for re-verification |

Before the (b) hand-off (Complete at stage 7), the runner must:
1. All steps are marked `done`.
2. All ACs have evidence stamped via `criterion pass`.
3. Self-review findings are filed via `mp reviews finding add --phase self`.
4. `mp milestone complete <id> --evidence "..."` transitions lifecycle to
   **`complete`**. This MUST happen **before** any `git add` / `git commit`
   driven by `[agent.automation].commit_after_execute` so a `complete` that
   rejects on red AC verification never produces a false commit, and so the
   lifecycle / evidence mutations written by `complete` are inside the
   automated commit. Do not leave the milestone at `in-progress` after all
   steps are done — that stuck state is flagged by validate as
   `W-LC-STUCK-EXEC`.
5. **After** `mp milestone complete` succeeds and `lifecycle` is confirmed
   `complete`, consult `[agent.automation].commit_after_execute` (see
   "Automation handoffs" below). If `true`, stage and commit the
   resulting code + plan evidence via `git add -A && git commit -m
   "<id>: complete"`. If `false`, leave the worktree dirty for the next
   stage / operator.

## Automation handoffs

The runner reads project-level workflow policy from `config.toml`
`[agent.automation]` at every handoff boundary instead of treating each
session as ad-hoc. The knobs are project-level — apply them, do not
invent session-local policy.

### Stage 5 — Claim & execute (branch_strategy)

Before claiming the milestone, run `mp config get
agent.automation.branch_strategy` (defaults to `"current"`). The runner
honors the value at claim time:

| Value | Runner action at stage 5 |
|-------|--------------------------|
| `per-milestone` | `git checkout -b <id>-<slug>` from a portable base before claiming. Portable base, in order of preference: (a) the current `HEAD` (always safe), (b) the repository's discovered default branch — `git symbolic-ref refs/remotes/origin/HEAD` resolved to its short name — only when the discovery is unambiguous, (c) never a hard-coded `main` or `master`. The runner records the base it actually used so the (a) hand-off payload and the (c) hand-off know where to push |
| `current` (default) | Stay on the current branch; `git status` to confirm clean |
| `none` | Do not cut, do not switch; document the branch the session is on |

A non-default `branch_strategy` is recorded in the (a) hand-off payload
so the coordinator knows which branch to push at stage 9. If the
branch creation fails (orphan HEAD, no remote, dirty worktree), the
runner stages the failure as a self-finding before continuing.

### Stage 7 — Complete → Stage 8 hand-off (commit_after_execute)

After `criterion pass` and **after `mp milestone complete` succeeds**
(lifecycle = `complete`), the runner consults
`agent.automation.commit_after_execute` — the consult feeds the commit
that lands AFTER complete, not before it:

| Value | Runner action at stage 7 |
|-------|--------------------------|
| `true` | Run stage 7's `mp milestone complete <id> --evidence "..."` **first** and confirm terminal success (`lifecycle = complete` via `mp show milestone <id> --fields 'milestone.lifecycle'`); only then stage and commit the resulting code + plan evidence via `git add -A && git commit -m "<id>: complete"` |
| `false` (default) | Skip the commit; the operator decides per session |

The runner does NOT push (push is the coordinator's knob — see below).
If `commit_after_execute` is `true` and the commit hook fails, file a
self-finding and continue — the milestone can still complete, and the
operator handles the commit manually.

## See also

- `mp-flow` — canonical 12-stage timeline, stage-binding, and the four-point
  hand-off protocol (data, session-boundary, evidence).
- `mp-coordinator` — coordinator role skill (stages 1-4, 8, 10, 11, 12).
- [executing.md](executing.md) — stage 6 self-review + inlined lesson-pattern pre-screen.
