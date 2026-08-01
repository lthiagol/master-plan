# Planning & authoring specs

You are in the **coordinator/planning** role: turning a vague idea into an
approved, gate-checked spec. Spec fields only — no steps or work packages yet
(those come after approval, in [`decomposing.md`](./decomposing.md)).

## The loop

```bash
# 1. Surface what's still unclear
mp interview checklist --checklist-type milestone
# … ask the user 2–4 questions per round; skip answered topics …

# 2. Check for gaps against an existing draft
mp interview gaps --id <id>           # what's still missing

# 3. Create the spec (spec fields ONLY)
mp milestone create --example         # preview the accepted JSON shape
mp milestone create --json @-         # from stdin / a scratch file
#   — or interactively —
mp milestone create --title "Add config validation"

# 4. Move toward approval
mp milestone set-spec-status <id> review      # → lifecycle: groomed
# … resolve every open question (Q-XX) …
mp milestone approve <id>                     # → lifecycle: approved
mp validate
```

## The spec fields

A spec is `{ title, intent, problem, scope, acceptance_criteria[],
design_decisions[], open_questions[] }`. Use `mp milestone create --example` to
see the exact template. The full anatomy is in
[`../milestone-details/`](../milestone-details/); the essentials:

- **`intent.outcome`** — one sentence: what users can do after this ships.
- **`problem.description`** — why it's needed.
- **`scope.in_scope` / `scope.out_of_scope`** — explicit boundaries.
- **`acceptance_criteria[]`** — observable, each with a `verification` command.
- **`open_questions[]`** — must all be resolved before approval.
- **`design_decisions[]`** — only where there were real trade-offs.

## Adding fragments during authoring

Use the fragment commands as the spec firms up — don't rebuild arrays:

```bash
mp milestone ac add <id> --description "…" --verification "cargo nextest run -p mp --test …"
mp milestone ac update <id> AC-02 --verification "…"
mp milestone ac remove <id> AC-04          # fails if a step covers it

mp milestone question add <id> --text "Do we need to support legacy fixtures?"
mp milestone question resolve <id> Q-01 --resolution "No — legacy is archived."

mp milestone design-decision add <id> --area storage --decision "JSON files" --rationale "diff-friendly"
mp milestone design-decision update <id> --area storage --decision "…"
```

## Gates you must clear to reach `review` / `approved`

`mp validate` enforces:

- **G3** — at least one acceptance criterion.
- **G4** — the configured minimum out-of-scope items (`full` = 2, `hybrid` = 1;
  configurable via `planning.require_min_out_of_scope`).
- **G14** — no pending `approval-request` annotations.

A spec that fails these cannot be promoted to `review`. And **all open
questions must be resolved** before approval.

## Mutating spec fields

For scalar spec fields, use `milestone update` (it accepts `--json @file` or
`--file`). It **rejects** the `acceptance_criteria` / `steps` arrays by default —
that is deliberate; use the fragment commands above instead.

```bash
mp milestone update <id> --json @payload.json
# payload.json: { "intent": { "outcome": "…" }, "scope": { "in_scope": ["…"] } }
```

`--accept-extra-fields` is an escape hatch for a raw `show --format raw` →
`update --json` round-trip; `--replace-arrays` is a migration-only escape hatch.
Neither is a normal authoring tool.

## Spec review projection

```bash
mp spec since-approval <id>     # what changed since the last approval
mp spec review <id>             # condensed review-oriented projection
```

Useful right before re-approving after a drift fix.

## Other intake surfaces (don't make everything a milestone)

| Change | Use |
|--------|-----|
| One-line bug / tweak + verification command | `mp track add bugfix\|tweak …` |
| Vague "someday" idea | `mp idea create …` |
| Concrete but not-now | `mp backlog add …` |

Tracks go `start → done` (no external review). If one grows, promote it:
`mp track promote bugfix BF-03 --to-milestone`. Routing guidance:
[`../mp/commands.md`](../mp/commands.md).

## When the spec is approved

Stop authoring. Hand off to the implementation-planning phase — see
[`decomposing.md`](./decomposing.md). Do not scaffold steps before approval.
