# Executing sub-mode — stages 5, 6, 7

The executing sub-mode covers the runner's execution domain: Claim & execute →
Self-review → Complete. The five canonical mp commands walk through each
stage. This file is the procedural deep-dive; `mp-flow` stages.toml is the
canonical stage manifest.

## Stage 5: Claim & execute

Goal: claim the milestone, set it in-progress, and execute each step.

### The five canonical execution commands

| # | Command | Stage | What it does |
|---|---------|-------|-------------|
| 1 | `mp milestone set-status <id> in-progress` | 5 (Claim & execute) | Claim the milestone; transitions execution_status to `in-progress` |
| 2 | `mp milestone step done <id> <step-id>` | 5 (Claim & execute) | Mark each step as done (run once per step) |
| 3 | `mp milestone ac criterion pass <id> <ac-id> --evidence "..."` | 5 (Claim & execute) | Stamp per-AC run evidence (run once per passed AC) |
| 4 | `mp reviews finding add <id> --phase self --severity <sev> --category <cat> --desc "..."` | 6 (Self-review) | Round-1 self-review: file self-detected issues (the runner's frame misses some defects; the coordinator's external review at stage 8 catches the rest) |
| 5 | `mp milestone complete <id> --evidence "..."` | 7 (Complete) | Finalize the milestone; transitions lifecycle to **`complete`** (terminal) |

### Execution order

1. **Automation consult FIRST:** `mp config get agent.automation.branch_strategy`.
   The consult runs **before** any plan write — a `per-milestone` switch
   must land on the new branch before `mp milestone set-status` writes
   the claim mutation, otherwise the first plan write is stranded on
   the old branch. Default `"current"` — stay on the current branch;
   `per-milestone` cuts `git checkout -b <id>-<slug>` against the
   portable base documented in SKILL.md → "Automation handoffs"; `none`
   does not cut or switch.
2. `set-status <id> in-progress` — claim the work (now on the branch
   `branch_strategy` selected — `per-milestone` already switched via
   step 1; `current` / `none` left the runner where it was).
3. For each step (in dependency order): work the step, then `step done <id> <step-id>`.
4. After all steps: for each passing AC, `criterion pass <id> <ac-id> --evidence "..."`.
5. If any AC fails: do NOT complete. Return to the relevant step.

### Evidence hygiene

Evidence is a per-run audit record — write once, never silently overwritten:

- Each `criterion pass` stamps evidence on one AC. The `evidence` field is the
  audit trail for that run (what test passed, what output confirms it).
- `mp milestone complete --evidence "..."` stamps the `verification.evidence`
  block and sets all ACs to `passed`. If an AC already has evidence from a prior
  run, `--force` is required to overwrite.
- After complete, the lifecycle is **`complete`**. The runner's session ends only after that field is confirmed.

### Commit hygiene

Commit policy is declarative — the `[agent.automation]` `commit_after_execute`
knob owns it. The runner does NOT ad-hoc commit; it consults `mp config get`
at stage 7 and either commits or not. When committing, the milestone's
evidence trail lives in the plan (via mp), not in the commit message. Keep
commits focused on the code changes.

When `commit_after_execute = false` (the default), the runner leaves the
worktree dirty so the human operator decides. When `true`, the runner
commits as part of the (b) hand-off; the push is then the coordinator's
responsibility at the (c) hand-off.

## Stage 6: Self-review

Goal: round-1 self-review before the hand-off to the coordinator.

### Self-review checklist

- [ ] Walk the lesson-pattern pre-screen.
  Key patterns:
  - Green tests do not imply correct behavior.
  - Reproducers catch what test suites miss.
  - Gate parity — new paths don't bypass existing gates.
  - The author should not be the only reviewer.
  - New bulk paths that bypass single-path validation.
  - Dry-run paths that don't actually preview what would happen.
- [ ] For each self-detected issue: file a finding.
  `mp reviews finding add <id> --phase self --severity <severity> --category <cat> --desc "..."`.
- [ ] If findings are filed: they are visible at stage 8 to the coordinator
  as part of the (b) hand-off payload. The coordinator's external review
  assesses them alongside their own findings.

### Self-review limits

Self-review is round 1 — a useful first pass, not the final one. The
author's self-review in the same session that produced the code is
unreliable — the discipline forces a session reset at every hand-off.
The runner's job at stage 6 is to catch the obvious defects and flag
them transparently. The coordinator's external review at stage 8 is
the definitive review.

## Stage 7: Complete

Goal: finalize the milestone and hand off to the coordinator.

### Complete checklist

- [ ] All steps are `done` (`mp milestone step show <id> <step-id>`).
- [ ] All ACs have evidence (`mp milestone ac criterion pass` for each).
- [ ] Self-review findings filed (`mp reviews finding list <id> --phase self`).
- [ ] **Automation consult FIRST:** `mp config get agent.automation.commit_after_execute`.
  Note the value but DO NOT commit yet — `mp milestone complete` runs
  before any `git add` / `git commit`. Committing the plan evidence and
  code after a successful complete is the only safe order; if `complete`
  rejects on red AC verification, the worktree stays dirty (no false-commit)
  and the failure is the runner's signal to block or escalate.
- [ ] `mp milestone complete <id> --evidence "..."` — transitions lifecycle to
  **`complete`**, sets AC statuses to `passed`. Confirm with
  `mp show milestone <id> --fields 'milestone.lifecycle'` (`complete`
  is the terminal state; if `complete` rejects, stop here and do not
  commit). If the milestone's AC verifications contain runnable commands
  (not `manual:`), `mp milestone complete` runs them — any failure rejects
  the complete. Use `--skip-verify` when the ACs were already verified
  manually (e.g., manual review steps).
- [ ] **Commit AFTER confirmed complete (when `commit_after_execute=true`):**
  `git add -A && git commit -m "<id>: complete"`. The commit
  includes the runner's code changes AND the plan evidence the
  complete step wrote. The push is the coordinator's knob, NOT the
  runner's — see `mp-coordinator` and the (c) hand-off.

### Post-complete

After complete, the runner's session ends. The coordinator picks up at stage 8
(External review) with a fresh session per the session-boundary discipline.
The (b) hand-off includes `lifecycle=complete` + self-findings from stage 6.

## See also

- `mp-flow` — per-stage commands, stages.toml manifest, and the Hand-off
  protocol section (point (b): runner→coordinator after complete).
- `mp-coordinator` — coordinator's external review at stage 8.
- The lesson-pattern pre-screen checklist (inlined in stage 6 above).
- [fixing.md](fixing.md) — fixing sub-mode for stage 9.
- [atomic-writes.md](atomic-writes.md) — advisory flock lock + per-AC killpg contracts.
