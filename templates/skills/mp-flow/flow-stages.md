# mp-flow per-stage checklist

Deep-dive companion to `SKILL.md`. Each section maps to a stage in the
12-stage timeline and covers the gotchas (fragment discipline, dry-run
honesty, search-first discovery), plus the spec-authoring checklist for
the new stages 1–4.

## Stage 1: Draft — spec-authoring checklist

### G1–G4 gate checklist

- [ ] G1: `spec_status` is NOT `ready` yet (in-progress without ready is rejected)
- [ ] G2: All open questions (`open_questions`) are resolved or closed
- [ ] G3: At least one acceptance criterion present in `acceptance_criteria`
- [ ] G4: At least two out-of-scope items declared in `scope.out_of_scope`

### Lifecycle state checklist

- `lifecycle`: `planned` (default after `mp milestone create`)
- `spec_status`: `draft` or `review` (not `ready` until stage 3)

### Gotchas for spec authoring

- **Fragment discipline:** Add ACs and steps one at a time via
  `mp milestone ac add` / `mp milestone step add`. Do NOT rebuild the
  acceptance_criteria or steps arrays via `mp milestone update --json`.
- **Dry-run honesty:** If previewing a bulk operation (e.g., bulk
  set-spec-status), use `--dry-run` and confirm the output matches
  expectations before the live run.
- **Op-arg up-front validation:** When passing `--ids` or `--where`
  to bulk commands, validate the selector resolves to the expected set
  before the write. A dry-run that reports "ok" for rows that the live
  run would reject is a liar.
- **Search-first discovery:** Use `mp search <query>` to find content
  in the plan before referencing it. Do NOT grep plan files directly.

## Stage 2: Groom — refinement checklist

- [ ] Run `mp milestone groom <id>` to open the grooming workflow
- [ ] Run `mp milestone challenge start <id>` for adversarial spec review
- [ ] Run `mp milestone decompose <id>` to split into WPs and steps
- [ ] Push `spec_status` to at least `review`
- [ ] All steps have valid `covers_ac` references (G10 gate)
- [ ] Step IDs follow outline notation (`S1`, `S2`, …)

### Fragment discipline for grooming

- Use `mp milestone wp add|update|remove` for WPs (fragment-only)
- Use `mp milestone step add|update|remove` for steps (fragment-only)
- After decomposition: `mp validate` to confirm step coverage

## Stage 3: Specify — spec lock checklist

- [ ] All G1–G4 gates pass (`mp validate`)
- [ ] `mp milestone set-spec-status <id> ready` succeeds without `--force`
- [ ] No open questions remain
- [ ] AC descriptions are verifiable (have a test command or manual check)

### Gate parity (G6 → G7)

When the runner later reaches stage 7, G6 checks that all ACs are passed
before `milestone complete` is accepted. G7 checks that the lifecycle
transitions through `verified`. These two gates must stay in sync — if
a manual AC pass skips G6 validation, G7 will detect the gap.

## Stage 4: Approve — approval checklist

- [ ] All G1–G4 gates are green
- [ ] G14 gate: no open approval-request annotations block `ready`
- [ ] `mp milestone approve <id>` exits 0
- [ ] Lifecycle: `approved`
- [ ] Session-boundary discipline: this is the planner → runner hand-off
  (the (a) hand-off). If in planning mode, stop here.

### Spec-authoring test-evidence discipline

- No force-bypass of gates without `--force` and recorded debt
- Every AC has a `verification` field (test command or manual check)
- `spec_status: ready` is non-negotiable before stage 5

## Stage 5: Claim & execute — execution checklist

- [ ] `mp execution check` confirms readiness
- [ ] `mp execution handoff` to enter autonomous mode
- [ ] `mp milestone set-status <id> in-progress` before any code
- [ ] Step status set to `in-progress` before work begins on each step
- [ ] Step evidence recorded via `mp milestone step done <id> <step-id>`

### Gotchas for execution

- **Fragment discipline:** Update step status one at a time. Do not
  rebuild steps arrays.
- **No-op semantics:** `mp milestone step done` with a step already
  done is a no-op (returns success, does not overwrite evidence).
- **Dry-run honesty:** `--dry-run` on bulk status transitions must
  preview the same gates that the live run enforces.
- **Search-first discovery:** Use `mp search --type step` to find
  specific steps before updating them.

## Stage 6: Self-review — runner round-1 checklist

- [ ] Run tests locally before self-review declaration
- [ ] File findings via `mp reviews finding add <id> --phase self`
- [ ] Each finding has a concrete description (not "looks ok")
- [ ] Lifecycle: self-review runs before `milestone complete`

### Session-boundary discipline

Self-review in the same session is a useful first pass, not the final one.
The runner's frame is the implementer's frame — defects obvious to a fresh
pair of eyes are invisible to the author. The coordinator's external review
(stage 8) must run in a different session.

## Stage 7: Complete — complete checklist

- [ ] All ACs passed (G6 gate: `mp validate` confirms)
- [ ] All steps marked done with evidence
- [ ] `mp milestone complete <id> --evidence "..."` exits 0
- [ ] Lifecycle: `complete` (terminal; confirm via `mp show --fields milestone.lifecycle`)

### Verification cache (run-once per `mp milestone complete`)

The AC verification snapshot is computed once per `mp milestone complete`
invocation. If evidence is updated after the snapshot is taken, re-run
`mp validate` to refresh the cache before completing.

### Per-AC killpg timeout contract

Per-AC verification timeouts must clean up child processes via `killpg`.
If a verification command spawns children and the timeout fires, the
process group is terminated. Do NOT rely on `SIGTERM` alone — use
`killpg` to ensure no orphans survive the timeout.

## Stage 8: External review — coordinator round-2 checklist

- [ ] Different session than the runner's execute session (session-boundary discipline)
- [ ] Read `mp execution report <M>` first — verify, don't trust
- [ ] Check diff against the report claims
- [ ] File findings via `mp reviews finding add <id> --phase external`
- [ ] Lifecycle: `reviewed`

### Gotchas for review

- **Dry-run honesty:** If using bulk operations during review, dry-run
  first. A dry-run that reports "ok" for a row the live run would reject
  is a liar.
- **No-op semantics:** Filing a finding that already exists is a no-op
  (returns success, does not create a duplicate).
- **Search-first discovery:** Use `mp search --type ac` to find ACs
  to compare against the diff.

## Stage 9: Remediate — runner fix checklist

- [ ] Resolve each finding individually: `mp reviews finding resolve <id> <finding-id>`
- [ ] Re-run tests after each fix
- [ ] After all findings resolved: `mp milestone complete <id> --evidence "..."`
- [ ] Hand back to coordinator (the (d) hand-off)

### No-op semantics

Resolving an already-resolved finding is a no-op (returns success, does
not change state). If a finding appears resolved but is not, check that
the finding ID matches.

## Stage 10: Re-review — coordinator re-review checklist

- [ ] Run `mp milestone verify <id>` — checks all findings resolved + all ACs pass
- [ ] **Mandatory:** `mp reviews pass <id> --verdict ok --reviewer <who>` —
  records the review; promotes `done` → `complete` when applicable
- [ ] Confirm `lifecycle=complete` via `mp show milestone <id> --fields 'milestone.lifecycle'`
- [ ] If clean: advance to stage 11 (Document)
- [ ] If new findings: file them, loop back to stage 9 (Remediate)

### Review loop exit condition

The loop (stages 8 → 9 → 10) exits only when verify is clean **and**
`mp reviews pass --verdict ok` has run so lifecycle is terminal `complete`.
Do NOT end stage 10 after verify alone.

## Stage 11: Document — documentation checklist

- [ ] Capture lessons learned via `mp note add`
- [ ] Tag notes with milestone `id`
- [ ] Include review findings resolution summary
- [ ] Use `--body @file.md` or `--body @-` for markdown content

## Stage 12: Hand off — close-out checklist

- [ ] All ACs passed
- [ ] All findings resolved
- [ ] All notes recorded
- [ ] `mp validate` exits 0
- [ ] `git commit` with descriptive message
- [ ] `mp next` to find next work item
