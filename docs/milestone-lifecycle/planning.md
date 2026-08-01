# Lifecycle: planning (`draft → groomed → approved`)

The planning stage turns a vague idea into an approved, gate-checked spec. Until
a milestone reaches `approved`, no implementation work should start.

## States

### `draft`
The spec exists but is incomplete. A freshly created milestone starts here.
What's typically missing: a crisp outcome, scope boundaries, and acceptance
criteria.

### `groomed`
The spec has been through an interview and is gap-free: it has an outcome, a
problem, scope (in and out), at least one acceptance criterion, and no open
questions. This corresponds to the legacy `spec_status` of `review`.

### `approved`
A human (or an approver) has accepted the spec. The milestone is now eligible
for execution. This corresponds to the legacy `spec_status` of `ready`.

## The loop

```bash
# 1. Interview — surface what's still unclear
mp interview checklist --checklist-type milestone
# … ask 2–4 questions per round, skip answered topics …
mp interview gaps --id <id>            # what's still missing

# 2. Create the spec (spec fields only — no steps/WPs yet)
mp milestone create --json @-          # or --file spec.json, or --title then --json
mp milestone set-spec-status <id> review   # → lifecycle: groomed

# 3. Resolve open questions (must be resolved before approval)
mp milestone question add <id> --text "…"
mp milestone question resolve <id> Q-01 --resolution "…"

# 4. Approve
mp milestone approve <id>              # → lifecycle: approved
mp validate
```

## Gates you must clear to reach `groomed`/`review`

`mp validate` enforces these on promotion:

- **G3** — at least one acceptance criterion.
- **G4** — the configured minimum out-of-scope items (default `2` for `full`,
  `1` for `hybrid`; configurable via `planning.require_min_out_of_scope`).

A spec that fails these cannot be promoted to `review`.

## What makes a good spec

- **`intent.outcome`** — one sentence: what can users do after this ships?
- **`problem.description`** — why is this needed? What gap does it fill?
- **`scope.in_scope` / `scope.out_of_scope`** — explicit boundaries. Two
  out-of-scope items is the default minimum because naming what you *won't* do
  is where most scope creep dies.
- **`acceptance_criteria`** — observable, verifiable behavior. Each AC carries a
  `verification` field that names a command or check. Avoid prose like "works
  correctly"; prefer `cargo nextest run -p mp --test config_set`.
- **`open_questions`** — anything unresolved. They must all be resolved before
  approval.
- **`design_decisions`** — when there were real trade-offs, record the area, the
  choice, and the rationale.

The anatomy of every field is in
[`../milestone-details/`](../milestone-details/).

## Spec-only creation

`mp milestone create` writes **spec fields only**. Do not include `steps` or
`work_packages` at creation — implementation planning is a separate, later phase.
Attempting to write those arrays through `milestone update` is rejected by
default (`--replace-arrays` is a migration escape hatch, not a normal tool).

## Drift after approval

If a previously-approved spec changes materially, validation flips
`needs_regrooming`. Re-run `mp milestone groom <id>`, address the findings, and
re-approve before resuming execution.
