# Planning sub-mode — stages 1-4

The planning sub-mode covers the coordinator's spec-authoring domain: Draft →
Groom → Specify → Approve. This file is self-contained: the CLI contract, the
fragment discipline, the state-update commands, and the per-profile checklists
all live here. There is no separate "primary tool" skill — the planning
sub-mode reads this file directly.

For deeper reference material that does not duplicate this content:

| Need | Go to |
|------|-------|
| Command reference | `docs/mp/commands.md` |
| Lifecycle / gates | `docs/milestone-lifecycle/` |
| Milestone fields | `docs/milestone-details/` |
| Agent workflows | `docs/agent-guide/` |
| Install / harness | `docs/mp/getting-started.md` |

## CLI contract

| Task | Command pattern |
|------|-----------------|
| Agent reads | `mp <cmd>` — JSON default; **omit** `--format json` |
| Read one AC | `mp milestone ac show <id> <AC-id>` (fragment-only — no full milestone load) |
| Read one step | `mp milestone step show <id> <step-id>` (fragment-only) |
| Field projection | `mp <cmd> --fields 'a.b,c[].d'` (all read commands; unknown path = hard error) (prefer over jq) |
| Projection by stable id | `mp show milestone <id> --fields 'acceptance_criteria[AC-03],steps[S4]'` |
| Health rollup | `mp show milestone <id> --summary` (prefer over jq counts) |
| Headline metrics | `mp status --summary` (no path block); `mp reviews finding list <id> --summary`; `mp reviews lifecycle --summary` |
| Note add markdown | `mp note add --title X --body @file.md` or `--body @-` (avoid shell-backtick mangling) |
| User display | Summarize JSON or defer to `raul` (see raul CLI). `mp` is agent-only — no `--format human`. |
| Agent writes | `mp <cmd> --json @-` (pipe JSON to stdin) or `--file path.json` |
| Edit one AC | `mp milestone ac add/update/remove <id> [<AC-id>]` (fragment-only — **not** `milestone update --json` array rebuild) |
| Edit one step | `mp milestone step add/update/remove <id> [<step-id>]` (fragment-only) |
| Edit one WP | `mp milestone wp add/update/remove <id> [<wp-id>]` (fragment-only) |
| After writes | `mp validate` |
| Toolkit health | `mp doctor` |
| **State updates** | See `docs/agent-guide/executing.md` — start/done/block/complete |
| **Findings** | `mp reviews finding list/resolve` — not `milestone update` |

**Fragment-first rule:** agents edit by id (`ac/step/wp show|add|update|remove`)
and read small JSON slices. `mp milestone update --json` rejects `acceptance_criteria`
and `steps` arrays by default; pass `--replace-arrays` only for migration scripts.
See `docs/agent-guide/` and `docs/mp/commands.md` for the full surface.

**Session start:** `mp doctor` → `mp config show` → `mp execution status` → `mp next` or `mp inbox`.

| `workflow.profile` | Prefer |
|--------------------|--------|
| `full` | brief → milestones |
| `hybrid` | track / idea / `session start` |
| `session` | `session start` → groom → spec |

Environment: `MP_HOME` (toolkit root), `MP_PROJECT` (project root override).

Step ids: outline notation (`S1`, `S3.1`) per milestone — see `docs/IDS.md`.

## Read-command quick table

| Task | Read command |
|------|-------------|
| Overall status | `mp status` |
| What's next? | `mp next` |
| All milestones | `mp list milestones` |
| One milestone | `mp show milestone <id>` |
| Steps for milestone | `mp list steps --milestone <id>` |
| Path preview | `mp path` |
| Inbox items | `mp inbox` |
| Validation | `mp validate` |
| Execution readiness | `mp execution check` |
| Review queue | `mp reviews pending` |
| Execution handoff report | `mp execution report <M>` |

## Work type routing

| User intent | Route |
|-------------|-------|
| New project, fuzzy direction | **Brief** (`mp brief *`) — first after `mp init` |
| New capability / epic | **Greenfield milestone** (`change_kind: greenfield`, default) |
| Change existing behavior | **Track** (small) or **delta milestone** (P4); see brownfield § |
| Small fix or tweak | Track item (`bugfix` / `tweak`) |
| "Handle later", parking lot | Idea (`ideas.json`, P1.6) |
| What's next? | `mp next` |

### Greenfield vs brownfield

| Kind | Use when | Doc |
|------|----------|-----|
| **Greenfield** | Net-new subsystem, no domain truth yet | Default milestone |
| **Brownfield (track)** | One-off fix, small change | `mp track add` |
| **Brownfield (delta)** | Changes documented domain (API, auth, …) | P4: `change_kind: delta` |

Run `mp doctor` — if `brownfield_likely`, prefer explicit before/after
in specs and `context.references` to source files. Full guide: `docs/BROWNFIELD.md`.

**Pre-P4 brownfield:** use greenfield milestone workflow + code zone search during
interview; document current vs desired behavior in spec fields.

**Spec before code.** Do not change application source until the milestone has
`spec_status: ready` (approved).

## Stage 1: Draft

Goal: turn a raw idea into a milestone with intent, problem, scope, and AC.

### Checklist

- [ ] Run `mp interview checklist` to populate the initial spec skeleton.
- [ ] Write the `intent.outcome` statement (one paragraph).
- [ ] Write the `problem.description` (what breaks today, who is affected).
- [ ] Write `scope.in_scope` and `scope.out_of_scope` (at least 2 out-of-scope items for G4).
- [ ] Write at least one `acceptance_criteria` entry (G3).
- [ ] Create the milestone: `mp milestone create --json @-` (pipe the spec JSON).
- [ ] Lifecycle: `planned`. Spec status: `draft`.

### Gotchas

- **Fragment discipline:** Add ACs one at a time via `mp milestone ac add`.
  Do NOT rebuild arrays via `mp milestone update --json`.
- **Dry-run honesty:** When previewing bulk operations, use `--dry-run` and
  confirm output matches expectations before the live run.
- **Search-first discovery:** Use `mp search <query>` to find content; never
  grep plan files directly.

## Stage 2: Groom

Goal: refine the spec and decompose into work packages and steps.

### Checklist

- [ ] Run `mp milestone groom <id>` to open the grooming workflow.
- [ ] Run `mp milestone challenge start <id>` for adversarial spec review.
- [ ] Run `mp milestone decompose <id>` to split into WPs and steps.
- [ ] Push `spec_status` to at least `review`.
- [ ] All steps have valid `covers_ac` references (G10 gate).
- [ ] Step IDs follow outline notation (`S1`, `S2`, ...).

### Fragment discipline for grooming

- Use `mp milestone wp add|update|remove` for WPs (fragment-only).
- Use `mp milestone step add|update|remove` for steps (fragment-only).
- After decomposition: `mp validate` to confirm step coverage.

### Weak spec

If the spec is vague — unclear intent, missing AC, no scope bounds — switch to the
spec co-design sub-mode with `spec-grill`. See [spec-co-design.md](spec-co-design.md).

## Stage 3: Specify

Goal: lock the spec for execution. Transition to `spec_status: ready`.

### Checklist

- [ ] All G1-G4 gates pass: `mp validate`.
- [ ] `mp milestone set-spec-status <id> ready` succeeds without `--force`.
- [ ] No open questions remain.
- [ ] AC descriptions are verifiable (each has a test command or manual check).

### G1-G4 gates

| Gate | When | Check |
|------|------|-------|
| G1 | set-spec-status `ready` | spec_status is NOT `ready` yet |
| G2 | set-spec-status `ready` | all open_questions resolved or closed |
| G3 | set-spec-status `ready` | at least one acceptance criterion |
| G4 | set-spec-status `ready` | at least two out-of-scope items |

## Stage 4: Approve

Goal: pass the gate checks, produce the approval payload, and hand off to the
runner at the (a) hand-off point. The full hand-off protocol (data, session
boundary, evidence) lives in `mp-flow`'s Hand-off protocol section.

### Checklist

- [ ] Run `mp plan verify-ac <id>` to verify every AC verification field resolves
  to a real test target or script. See [reviewing.md](reviewing.md) for the
  integrity pre-flight details.
- [ ] Run `mp milestone approve --dry-run <id>` to preview the gate result.
- [ ] If both pass: `mp milestone approve <id>` to set `lifecycle: approved`.
- [ ] The hand-off payload to the runner includes: approved spec + planning notes +
  gate result + AC verification integrity report. See the (a) hand-off contract
  in `mp-flow`'s Hand-off protocol section.

### Approve gate

Stage 4 is the planner-to-runner hand-off entry. The coordinator's job at this
stage is to produce a spec that the runner can pick up and execute without
clarification loops. If `mp milestone approve` fails any gate, return to the
relevant stage (2 or 3) to fix the deficiency.

## Minimal validating milestone JSON

Copy and adapt this schema-valid example for `mp milestone create --json @-`:

```json
{
  "title": "Your milestone title",
  "intent": {
    "outcome": "What users can do after this ships."
  },
  "problem": {
    "description": "Why this is needed — the gap it fills."
  },
  "scope": {
    "in_scope": ["Specific deliverable"],
    "out_of_scope": ["Explicit non-goal 1", "Explicit non-goal 2"]
  },
  "acceptance_criteria": [
    {
      "description": "Observable behavior that proves completion",
      "verification": "How to verify (test command, manual check)"
    }
  ],
  "design_decisions": [],
  "open_questions": []
}
```

**Lean 2.0:** do not include `behavior`, `requirements`, `success_criteria`,
`interface`, `context`, `technical_context`, `assumptions`, `risks`, or
`follow_ups` — dropped ceremony fields.

## Per-profile first-hour checklists

### Full profile (greenfield / brownfield)

```text
1. mp init --profile full --from-repo    (or --from-repo for brownfield)
2. mp doctor
3. mp brief todo
4. Fill brief topics with user
5. mp brief done
6. mp interview checklist --checklist-type charter --draft
7. Fill charter goals/non-goals with user
8. mp interview checklist --checklist-type milestone --draft
9. First milestone → mp milestone create → approve → decompose
```

### Hybrid profile

```text
1. mp init --profile hybrid
2. mp doctor
3. mp config show   (confirm .mp/ path is gitignored)
4. mp track add bugfix ...        (or tweak)
5. Park ideas: mp idea create ...
6. mp next
```

### Minimal / session profile

```text
1. mp init --profile session
2. mp session start "My session"
3. mp interview checklist --checklist-type milestone --draft
4. mp milestone create ...
5. mp session focus <id>
6. mp next
```

## Error codes

Common validation and schema errors:

| Code | Meaning | Action |
|------|---------|--------|
| `B1` | `brief.status != done` | Run `mp brief done` first |
| `B3` | Charter after brief without `brief done` | Complete brief before charter interview |
| `G1` | `in-progress` without `spec_status: ready` | Approve spec first |
| `G2` | Open question unresolved at `ready` | Resolve or close `Q-XX` items |
| `G3` | Missing acceptance criteria | Add at least one AC |
| `G4` | Fewer than 2 out-of-scope items | Add scope exclusions |
| `G5` | Implementation plan before spec ready | Remove WPs/steps until approved |
| `G6` | AC not passed at `verified` | Pass all criteria before `complete` |
| `G7` | `done` without `verified` | Use `milestone complete` |
| `G8` | Dependency not done | Complete dependent milestones first |
| `G10` | Uncovered AC (strictness=full) | Add `--covers-ac` to relevant steps |
| `G11` | Delta needs domain | Add `delta.domain` and `specs/{domain}.json` |
| `G12` | Invalid delta targets | Check MODIFIED/REMOVED targets exist in domain |
| `G13` | Delta merge conflict | Rebase milestone; update `delta.base_version` |
| `G14` | Open approval-request annotation blocks ready | Resolve the approval annotation |
| `R1` | Invalid annotation item | Fix target, kind, status, body, or author |
| `SCH-01` | Schema validation error | Check field path in error message |
| `T1` | Track item missing required field | Fill title, problem, verification |

## Gotchas — CLI ≠ docs

| # | The doc says... | Real CLI behavior |
|---|----------------|-------------------|
| 1 | `mp idea add` | Use `mp idea create` (add is not a subcommand) |
| 2 | `mp interview --type` | Use `--checklist-type` (old `--type` is deprecated) |
| 3 | `mp list tracks` | Use `mp list tracks` (plural) or `mp track list` |
| 4 | `mp plan show --all` | No `--all` flag; `mp plan show` returns everything |
| 5 | `mp milestone step done --evidence` | No `--evidence` flag on `milestone step done` (evidence stored in milestone) |
| 6 | `mp milestone criterion pass --evidence` | `--evidence` is a positional arg, not a flag (use quotes) |
| 7 | Add `--format json` on every read | JSON is default — omit the flag |
| 8 | `mp show milestone \| jq` for counts | Use `mp show milestone <id> --summary` (or `mp status --summary`, `mp reviews finding list <id> --summary`) |
| 9 | `milestone update --json` for findings | Use `mp reviews finding resolve` |
| 10 | `mp path \| jq` / `mp inbox \| jq` for one field | Use `mp path --fields '…'` / `mp inbox --fields '…'` |

## See also

- `mp-flow` — canonical 12-stage timeline and stage-binding.
- [spec-co-design.md](spec-co-design.md) — adversarial sub-mode for weak specs at stages 1-3.
- [reviewing.md](reviewing.md) — AC verification integrity pre-flight, session-boundary discipline, and inlined lesson-pattern pre-screen.
