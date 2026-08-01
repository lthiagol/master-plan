---
name: mp-flow
description: Cross-role Master Plan lifecycle orchestration. Use when coordinating the 12-stage mp-flow timeline across planning, execution, self-review, external review, remediation, documentation, and hand-off.
---

# mp-flow — cross-role orchestration (12-stage timeline)

The cross-role orchestration reference for the full milestone lifecycle — from
writing the spec to closing the review rounds. Any agent (coordinator or runner)
loads this skill as the meta-skill that lays out the 12 stages and binds each
stage to the role that owns it.

## Role-binding table

| Stage | Name | Owner |
|-------|------|-------|
| 1 | Draft | coordinator |
| 2 | Groom | coordinator |
| 3 | Specify | coordinator |
| 4 | Approve | coordinator |
| 5 | Claim & execute | runner |
| 6 | Self-review | runner |
| 7 | Complete | runner |
| 8 | External review | coordinator |
| 9 | Remediate | runner |
| 10 | Re-review | coordinator |
| 11 | Document | coordinator |
| 12 | Hand off | coordinator |

Stages 1–4 are the coordinator's spec-authoring domain. Stages 5–7 are the
runner's execution domain. Stages 8–10 form the review loop: if stage 10
(Re-review) finds new findings, the loop returns to stage 9 (Remediate); a
clean stage 10 advances to stage 11. Stages 11–12 wrap the cycle.

## Two-round review

Round 1 is the runner's self-review at stage 6 (file findings with
`--phase self`). Round 2 is the coordinator's external review at stage 8.
Stage 7 `mp milestone complete` writes lifecycle **`complete`** (terminal).

The author should not be the only reviewer — that discipline is the
rationale for this split: the runner session that produces code is not
the same session that does stage-8 external review. Self-review (stage 6) is a
useful first pass, not the final one. The full hand-off protocol — what data
passes between rounds, what session-boundary each side enforces, what
evidence each side leaves — is documented inline below in the Hand-off
protocol section.

The review loop (stages 8 → 9 → 10) is:
- Stage 8: coordinator files findings against the runner's work.
- Stage 9: runner remediates each finding.
- Stage 10: coordinator re-reviews with `mp milestone verify` **and**
  `mp reviews pass --verdict ok`; clean = advance to Document (stage 11),
  new findings = loop back to stage 9.

## Stage ownership

The coordinator owns stages 1, 2, 3, 4, 8, 10, 11, 12. The runner owns
stages 5, 6, 7, 9. Stages 8 → 9 → 10 form a review loop (see
`stages.toml`).

### Coordinator → runner hand-offs

Four session-boundary crossings follow the author-not-only-reviewer discipline (see Hand-off
protocol below):

| Hand-off | From stage | To stage | Owner | Description |
|----------|-----------|----------|-------|-------------|
| Planner → runner | 4 (Approve) | 5 (Claim & execute) | coordinator → runner | Hand-off (a) |
| Runner → coordinator self-review | 7 (Complete) | 8 (External review) | runner → coordinator | Hand-off (b) |
| Coordinator → runner with findings | 8 (External review) | 9 (Remediate) | coordinator → runner | Hand-off (c) |
| Runner → coordinator re-review | 9 (Remediate) | 10 (Re-review) | runner → coordinator | Hand-off (d) |

## Hand-off protocol

The cross-cutting protocol that defines what data passes, what session-boundary
discipline applies, and what evidence the runner leaves for the coordinator
(and vice versa) at each stage transition in the 12-stage timeline. The
author should not be the only reviewer is the rationale: the runner
session that produces code (stages 5-7) is not the same session that
fixes the coordinator's findings (stage 9); the coordinator's review
session (stages 8 + 10) is not the same session that did the planning
(stages 1-4).

### Session-boundary discipline

Applied to the mp-flow timeline:

- A runner session that completes execution (stage 7) closes; the
  coordinator's external review (stage 8) opens in a **fresh session**.
- The coordinator's review session (stage 8) closes; the runner's
  remediation session (stage 9) opens in a **fresh session**.
- The coordinator's planning session (stages 1-4) closes; the runner's
  execution session (stage 5) opens in a **fresh session**.

Same-session self-review (a runner reviewing their own work in the same
session that produced it) is unreliable; the author-shouldn't-be-only-reviewer
discipline forces a session reset at every hand-off.

### Hand-off point (a): Approve → Claim & execute

Stage transition: 4 → 5.

| Field | Value |
|-------|-------|
| **Direction** | coordinator → runner |
| **Data** | approved spec (intact `intent`/`scope`/`acceptance_criteria`/`design_decisions`) + per-AC verification integrity report (output of `mp plan verify-ac <id>`) + gate result (G1–G4, G14 all green) + planning notes (any design_decisions the coordinator wants to surface) |
| **Session-boundary** | coordinator's planning session (stages 1-4) closes; runner's execution session (stage 5) opens in a fresh session |
| **Evidence** | the milestone file on disk (the source of truth) + the integrity report surfaced as part of the hand-off payload |

#### AC verification integrity report

The integrity report is the output of `mp plan verify-ac <id>`: a per-AC
table where each row resolves the AC's verification command (`cargo test`
target, `make` target, `bash`/`python` script) or surfaces an
unresolvable symbol. The report travels with the hand-off payload at stage
4 → 5 so the runner doesn't have to re-discover verification integrity on
its own.

**Runner-side rejection rule:** if any row in the integrity report is
`UNRESOLVABLE` (or `empty` or `unknown`), the runner rejects the
hand-off. The rejection surfaces the report and routes the milestone
back to the coordinator's reviewing session for re-approval. The runner
does NOT silently fix bad verifications — that is a spec defect, not a
runner defect.

### Hand-off point (b): Complete → External review

Stage transition: 7 → 8.

| Field | Value |
|-------|-------|
| **Direction** | runner → coordinator |
| **Data** | `lifecycle=complete` + self-findings (the round-1 review at stage 6) via `mp reviews finding list <id> --phase self` + per-step evidence (`mp milestone step show <id> <step-id>`) + per-AC evidence (`mp milestone ac criterion show <id> <ac-id>`) |
| **Session-boundary** | runner's execution session (stages 5-7) closes; coordinator's external review session (stage 8) opens in a fresh session |
| **Evidence** | the milestone file with `lifecycle: complete`, the per-AC `evidence` fields, the `mp reviews finding` registry entries |

### Hand-off point (c): External review with findings → Remediate

Stage transition: 8 → 9.

| Field | Value |
|-------|-------|
| **Direction** | coordinator → runner |
| **Data** | external findings via `mp reviews finding list <id> --phase external` (the round-2 review at stage 8, severity-ordered with `high` first, then `medium`, then `low` per the `SeverityRank` contract — the canonical `low\|medium\|high` vocabulary that the install source's `SeverityRank::from_config_value` parses) + the milestone file with `lifecycle: in-progress` (re-opened for remediation) |
| **Session-boundary** | coordinator's external review session (stage 8) closes; runner's remediation session (stage 9) opens in a fresh session |
| **Evidence** | the `mp reviews finding` registry entries (each finding has `fixed_in: ""` until the runner resolves it; `mp reviews finding resolve <id> <finding-id>` updates `fixed_in` to the resolution commit) |

### Hand-off point (d): Remediate → Re-review

Stage transition: 9 → 10.

| Field | Value |
|-------|-------|
| **Direction** | runner → coordinator |
| **Data** | remediated code (the patches for each finding) + finding resolutions (each `mp reviews finding` row's `status: fixed`, `fixed_in: <sha>`, `resolved: <date>`) + the milestone file with `lifecycle: in-progress` (re-opened for re-review) |
| **Session-boundary** | runner's remediation session (stage 9) closes; coordinator's re-review session (stage 10) opens in a fresh session |
| **Evidence** | the patch series (commits, in order of finding resolution) + the updated `mp reviews finding` registry |

If the re-review at stage 10 finds new findings, the cycle loops back to
hand-off (c). A clean re-review advances the lifecycle past the review
loop (stage 10 → stage 11 Document).

### Stage-transition contract

For every hand-off point above, three subsections are documented:

1. **Data** — what passes (findings, evidence, blockers, integrity report, etc.)
2. **Session-boundary** — which side's session id closes; the next side's session id opens
3. **Evidence** — the audit trail the producing side leaves (registry entries, milestone file state, commit chain)

The contract is verified by `cargo test -p mp --test
handoff_protocol_session_boundary_enforced` (content-completeness check,
not runtime harness enforcement) against the Hand-off protocol section
above.

## Single-source invariant

The execute → review → remediate procedure in `mp-flow` is the canonical
12-stage timeline; the role-specific deep-dives live in `mp-coordinator`
(stages 1-4, 8, 10, 11, 12) and `mp-runner` (stages 5-7, 9). The 8-9-10
review loop cross-links to `mp-coordinator/reviewing.md` for the
lesson-pattern pre-screen. `mp-flow` is the canonical timeline; the
role-specific skills own the deep-dives for each role's domain.

## G1–G4 spec-authoring gates (coordinator domain)

Before a milestone reaches `spec_status: ready`, it must pass four gates:

| Gate | What it checks |
|------|---------------|
| G1 | `in-progress` without `spec_status: ready` is rejected |
| G2 | Open questions must be resolved or closed before `ready` |
| G3 | At least one acceptance criterion must be present |
| G4 | At least two out-of-scope items must be declared |

These gates are the coordinator's responsibility in stages 1–4. The
test-evidence discipline: no force-bypass without `--force` and recorded
debt. See `flow-stages.md` for the per-stage checklist covering G1–G4.

## Draft

Stage 1 — coordinator. Write the milestone spec (intent, problem, scope, AC,
design_decisions).

```
mp interview checklist
mp milestone create --json @-
```

All spec fields filled before advancing. Use `mp validate` after every write.
If the milestone idea is vague, load `spec-grill` for adversarial co-design
before creating the milestone.

## Groom

Stage 2 — coordinator. Refine the spec, challenge assumptions, decompose into
work packages and steps.

```
mp milestone groom
mp milestone challenge start
mp milestone decompose
```

Push `spec_status` to at least `review` before advancing. Use
`mp milestone set-spec-status <id> review` and summarize for user approval.

## Specify

Stage 3 — coordinator. Lock the spec for execution — transition to
`spec_status: ready`.

```
mp milestone set-spec-status <id> ready
```

All G1–G4 gates must be green. Use `mp validate` to confirm. The
`spec_status: ready` state is non-negotiable — no implementation begins
without it.

## Approve

Stage 4 — coordinator. Pass the G1–G4 gates and the G14 approval gate;
transition to `lifecycle: approved`.

```
mp milestone approve <id>
```

After approval, the milestone is ready for the runner. This is the first
hand-off point (a). If in planning mode, stop here — do NOT set
`in-progress`.

## Claim & execute

Stage 5 — runner. Claim the milestone and execute the steps. Use
`mp execution check` to confirm readiness, then `mp execution handoff`
to enter autonomous mode.

```
mp milestone set-status <id> in-progress
mp milestone step done <id> <step-id>
```

Follow the gotchas in `flow-stages.md`: gate parity (G6 → G7),
dry-run honesty, op-arg up-front validation, no-op semantics. Step status
must be set to `in-progress` before code work begins.

## Self-review

Stage 6 — runner. Round-1 self-review (per the author-not-only-reviewer discipline). File findings with
`--phase self`; lifecycle stays `in-progress` until stage 7 complete.

```
mp reviews finding add <id> --phase self --description "..."
```

Record findings the runner catches before handing to the coordinator.
Self-review is a useful first pass, not the final one — the coordinator's
external review (stage 8) catches what the implementer's frame misses.

## Complete

Stage 7 — runner. Mark the milestone complete; lifecycle transitions to
**`complete`** (terminal). This is the second hand-off point (b). Do not leave
all steps done with lifecycle still `in-progress` — validate flags that as
`W-LC-STUCK-EXEC`.

```
mp milestone complete <id> --evidence "..."
```

Verify each AC is passed before completing (G6 gate). The verification
cache is run-once per `mp milestone complete` invocation — re-run
`mp validate` if evidence changes. See `flow-stages.md` for verification
cache and per-AC killpg timeout details.

## External review

Stage 8 — coordinator. Round-2 external review. File findings for the runner
to remediate. Lifecycle: `reviewed`. This is the third hand-off point
(c).

```
mp reviews finding list <id>
mp reviews finding add <id> --phase external --description "..."
```

Use `mp execution report <M>` to read the per-step/per-AC evidence summary
first. Verify report claims against diff and tests — do not trust the
runner's self-attestations.

## Remediate

Stage 9 — runner. Patch the code per the coordinator's findings. Lifecycle
transitions to `in-progress` (remediation). This is the fourth hand-off point
(d).

```
mp reviews finding resolve <id> <finding-id>
```

Resolve each finding individually. After all findings are resolved,
re-run `mp milestone complete` and hand back to the coordinator.

## Re-review

Stage 10 — coordinator. Verify the remediation. If clean, **close the review
and ensure terminal lifecycle**, then advance to stage 11 (Document). If new
findings exist, loop back to stage 9 (Remediate).

```
mp milestone verify <id>
mp reviews pass <id> --verdict ok --reviewer <who>
```

`verify` checks findings + ACs. `reviews pass --verdict ok` records the review
and auto-promotes `lifecycle=done` → `complete` when the legacy triple is present.
Do not end stage 10 after verify alone.

## Document

Stage 11 — coordinator. Capture the lesson (if any) and write the closing
note.

```
mp note add --title "..." --body @-
```

Record execution narrative. Tags to include: milestone `id`, lessons learned,
review findings closed. Use `--body @file.md` or `--body @-` for markdown
content.

## Hand off

Stage 12 — coordinator. Close out the milestone and hand off to the next
work.

```
git commit -m "..."
```

The milestone is verified, documented, and ready to archive. Use `mp next`
to find the next work item.

## See also

- `mp-coordinator` — coordinator role skill: planning + reviewing
  + spec-co-design (stages 1–4, 8, 10, 11, 12).
- `mp-runner` — runner role skill: executing + fixing (stages 5–7, 9).
- The session-boundary discipline (the author should not be the only
  reviewer) — rationale for the role split.
- `stages.toml` — the canonical stage manifest (12 stages + role-binding).
- `flow-stages.md` — per-stage deep-dive checklist with the gotchas.