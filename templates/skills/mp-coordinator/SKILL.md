---
name: mp-coordinator
description: Coordinator role workflow for Master Plan milestones. Use when planning or grooming specs, running spec-grill, performing external review, or re-reviewing remediation in the mp-flow lifecycle.
---

# mp-coordinator — planning + reviewing (coordinator role)

The coordinator is one of two agent roles in the two-agent / four-role architecture
(`mp-flow`). The coordinator owns the spec-authoring domain (stages 1-4) and
the review domain (stages 8, 10) plus the closing stages (11-12). When an agent
session loads `mp-flow` + `mp-coordinator`, it is in the coordinator role.

## Stage ownership

| Stage | Name | Owner |
|-------|------|-------|
| 1 | Draft | coordinator |
| 2 | Groom | coordinator |
| 3 | Specify | coordinator |
| 4 | Approve | coordinator |
| 8 | External review | coordinator |
| 10 | Re-review | coordinator |
| 11 | Document | coordinator |
| 12 | Hand off | coordinator |

Stages 1-4 are the spec-authoring domain: Draft (interview), Groom (challenge +
decompose), Specify (write AC + scope + design_decisions), Approve (gate check,
transition to `spec_status: ready`). Stages 8 + 10 form the review domain:
External review (file findings) and Re-review (verify remediation). Stages 11-12
close out the cycle.

The runner owns stages 5-7 and 9 (execution, self-review, complete, remediate).
See `mp-runner` for the runner's domain.

## Sub-mode map

Three sub-modes match the coordinator's stage ownership:

| Sub-mode | Stages | Deep-dive |
|----------|--------|-----------|
| Planning | 1, 2, 3, 4 | [planning.md](planning.md) |
| Spec co-design | 1, 2, 3 | [spec-co-design.md](spec-co-design.md) |
| Reviewing | 8, 10 | [reviewing.md](reviewing.md) |

The planning sub-mode covers stages 1-4 and is self-contained (CLI contract,
fragment discipline, state updates, per-profile checklists, error codes, and
gotchas all live in `planning.md`). Spec co-design is an adversarial sub-mode
for weak specs, most useful at stages 1-3 before the Approve gate. The
reviewing sub-mode covers stages 8 + 10; the lesson-pattern
pre-screen is inlined in [reviewing.md](reviewing.md).

Each sub-mode file is a procedural wrapper that references the primary tools;
content is not duplicated (single-source invariant).

## Two-round review

Round 1 is the runner's self-review at stage 6 (file findings with `--phase self`).
Round 2 is the coordinator's external review at stage 8.

The two rounds are separated by a session boundary. The runner session that produces
code (stages 5-7) is not the same session that does the stage-8 external review.

The author should not be the only reviewer — self-review in the same
session is a useful first pass, not the final one.

The review loop (stages 8 → 9 → 10) is:
- Stage 8: coordinator files findings against the runner's work.
- Stage 9: runner remediates each finding (see `mp-runner`).
- Stage 10: coordinator re-reviews; **clean requires a lifecycle-closing command**:
  1. `mp milestone verify <id>` (ACs + findings check)
  2. `mp reviews pass <id> --verdict ok --reviewer <who>` (records the review;
     auto-promotes `lifecycle=done` → `complete` when the legacy triple is
     present; already-`complete` milestones stay terminal)
  New findings = loop back to stage 9. Do not end stage 10 after verify alone.

## Session-boundary discipline

Same-session self-review is unreliable — the author should not be the
only reviewer. The implementation: sessions that cross the
coordinator/runner boundary are never the same session. Each hand-off
point names which side's session id closes and the evidence the
producing side leaves. The four-point protocol (data, session-boundary,
evidence) lives in `mp-flow`'s Hand-off protocol section.

- **Stages 1-4 (planning):** one coordinator session. Before the stage 4 → 5
  hand-off, run `mp plan verify-ac <id>` (the AC verification integrity pre-flight,
  see [reviewing.md](reviewing.md)). Any unresolvable verification symbol blocks
  the Approve gate.
- **Stages 8 + 10 (reviewing):** fresh coordinator session, never the same session
  that did planning (stages 1-4) nor the runner's execution (stages 5-7, 9).
- **Stage 10 re-review:** fresh coordinator session, never the same session that
  did stage 8 review.

## Hand-off contract to the runner

The coordinator hands work to the runner at four points in the 12-stage timeline.
The deep-dive protocol (what data passes, what session-boundary, what evidence)
lives in `mp-flow`'s Hand-off protocol section. At a high level:

| Hand-off | From stage | To stage | Description |
|----------|-----------|----------|-------------|
| (a) | 4 (Approve) | 5 (Claim & execute) | Approved spec + planning notes + gate result + AC verification integrity report |
| (b) | 7 (Complete) | 8 (External review) | Runner's self-review at stage 6 → coordinator picks up at stage 8 |
| (c) | 8 (External review) | 9 (Remediate) | Coordinator's external findings → runner fixes |
| (d) | 9 (Remediate) | 10 (Re-review) | Runner's fixes + finding resolutions → coordinator re-verifies |

Before the (a) hand-off (Approve gate at stage 4), the coordinator runs
`mp plan verify-ac <id>` and `mp milestone approve --dry-run <id>`. The
integrity report and gate result are part of the hand-off payload so the
runner can validate the spec is executable before claiming.

## Automation handoffs

The coordinator reads project-level workflow policy from `config.toml`
`[agent.automation]` at the review boundary instead of deciding push /
remediation per session. The knobs are project-level — apply them, do
not invent session-local policy.

### Stage 8 — External review (auto_remediate)

When filing findings at stage 8, the coordinator consults
`mp config get agent.automation.auto_remediate` once per review session
(default `"none"` = record only). The threshold semantics — the value
names the **minimum** severity to remediate, ordering
`none < low < medium < high`, with `all` aliasing `low` — are codified
in `SeverityRank`. The parser (`SeverityRank::from_config_value`)
accepts only the documented set; any other value (including the legacy
four-level severity labels) is treated as `None` and silently
defeats AC-06.

| Threshold | Stage 8 action |
|-----------|----------------|
| `none` (default) | File every finding via `mp reviews finding add --phase external`; the runner decides per finding at stage 9 |
| `low` / `all` | File all findings; flag the at-or-above list in the (c) hand-off payload so the runner knows which to remediate immediately |
| `medium` | File all findings; flag `medium` and `high` as auto-remediate (skip `low`) |
| `high` | File all findings; flag only `high` as auto-remediate |

`mp reviews finding add` records every severity unconditionally (the
audit trail must be complete). The threshold is the **decision to act**
at the (c) hand-off — applying it is policy, not gating. Recording
without acting is the safe default; acting without recording would erase
the audit trail and is never acceptable.

### Stage 8 → Stage 9 hand-off (push_after_review)

Before the (c) hand-off, the coordinator consults
`mp config get agent.automation.push_after_review`:

| Value | Coordinator action at stage 8 → 9 |
|-------|------------------------------------|
| `true` | `git push -u origin <branch>` after the (c) hand-off payload is sent to stage 9 |
| `false` (default) | Skip the push; the operator decides per session |

The push target is the runner's branch as recorded in the (a) hand-off
payload (set at stage 5 from `agent.automation.branch_strategy`). If the
push fails (no remote, auth, divergence), file a stage-8 finding so the
runner can handle it at stage 9.

## See also

- `mp-flow` — canonical 12-stage timeline, stage-binding, and the four-point
  hand-off protocol (data, session-boundary, evidence).
- `mp-runner` — runner role skill (stages 5-7, 9).
- `spec-grill` — adversarial sub-mode for weak specs at stages 1-3.
- [reviewing.md](reviewing.md) — external review + inlined lesson-pattern pre-screen.
