# Spec co-design sub-mode — adversarial spec grilling

The spec co-design sub-mode is the adversarial sub-mode for milestones with weak or
vague specs. The primary tool is `spec-grill`. This file is the procedural wrapper;
`spec-grill` SKILL.md is the canonical reference for the three-round adversarial
loop. No content is duplicated (single-source invariant).

## When to use

Invoke the spec co-design sub-mode at stages 1-3 (Draft → Groom → Specify) when:
- The intent/outcome is vague ("I want X" without a concrete problem).
- ACs are missing or non-verifiable.
- Scope is unbounded (no out-of-scope items).
- The problem isn't articulated in one sentence.
- The agent detects weak spec quality during the planning sub-mode.

The sub-mode is most useful before the Approve gate (stage 4): a spec that
passes the adversarial grilling is more likely to survive the runner's
execution without clarification loops.

## Workflow

1. Load `spec-grill` in the coordinator session alongside `mp-coordinator`.
2. Invoke `spec-grill` via its activation triggers: user says "spec grill",
   the agent detects a weak idea, or the reviewer invokes it during grooming.
3. `spec-grill` runs the three-round adversarial loop (Intent & Problem →
   Scope & Out-of-scope → Acceptance Criteria & Verifiability), wrapping
   `mp interview gaps` after each round.
4. The output is a validated milestone spec ready for `mp milestone create`
   or `mp milestone update`.

## Integration with planning sub-mode

| Stage | Spec co-design role |
|-------|---------------------|
| 1 (Draft) | If the idea is vague, start with spec-grill before writing the spec. |
| 2 (Groom) | If the challenge exposes weaknesses, run spec-grill to tighten the spec. |
| 3 (Specify) | If the spec won't pass the G1-G4 gates, run spec-grill to fix the gaps. |

After spec-grill produces a validated spec, return to [planning.md](planning.md)
to continue the stage workflow.

## See also

- `spec-grill` — canonical three-round adversarial loop (source of truth).
- [planning.md](planning.md) — the planning sub-mode that invokes spec-grill for weak specs.
