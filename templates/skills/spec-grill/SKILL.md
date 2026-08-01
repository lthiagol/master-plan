---
name: spec-grill
description: Interactive spec co-design via adversarial multi-round questioning. Use when a milestone idea is vague, when the agent detects weak intent/scope/ACs, or when the user types /spec-grill. Wraps mp interview checklist + mp interview gaps in a structured grilling loop. Output is a validated milestone spec ready for mp milestone create.
compatibility: >-
  Requires the mp binary on PATH or at ~/.agents/master-plan/bin/mp.
  v1 harnesses: OpenCode (discovers ~/.agents/skills/spec-grill) and
  Cursor (install mirror to ~/.cursor/skills/spec-grill). Deployed by mp install.
---

# Spec Grill

**Spec Grill** — *Grill vague ideas into review-ready milestone specs.*

This skill runs a structured adversarial questioning loop when a milestone idea
lacks clarity. It does NOT replace `mp interview` — it wraps it, running
`mp interview gaps` after each round to surface remaining weak spots.

## Activation

| Trigger | How |
|---------|-----|
| **Slash command** | User types `/spec-grill` |
| **Contextual** | Agent detects a vague idea (no clear intent, missing ACs, no scope) |
| **Manual** | Reviewer invokes it during grooming |

## The 3-round adversarial loop

Each round asks 2–4 pointed questions targeting one spec dimension, then runs
`mp interview gaps` to surface remaining weaknesses. The next round attacks
whatever the gap report shows is weakest.

### Round 1 — Intent & Problem

Goal: turn "I want X" into a concrete problem statement.

```
1. Ask 2–4 adversarial questions:
   - "What specifically breaks today that this would fix?"
   - "Who is affected and how do they know it's broken?"
   - "What happens if we don't do this?"
   - "Is this a symptom of a deeper problem?"
2. User answers → update intent/context/problem in mp milestone update
3. mp interview gaps --checklist-type milestone
4. Feed gap report into Round 2
```

Defensibility check: If the user cannot articulate the problem in one sentence,
stay in Round 1.

### Round 2 — Scope & Out-of-scope

Goal: bound the work so the milestone is achievable and focused.

```
1. Ask 2–4 adversarial questions:
   - "What is the simplest version that delivers value?"
   - "What are you explicitly NOT building?"
   - "Does this touch other systems? Which ones are in-scope vs out?"
   - "What would make this 'done' vs 'over-engineered'?"
2. User answers → update scope.in_scope / scope.out_of_scope
3. mp interview gaps --checklist-type milestone
4. Feed gap report into Round 3
```

Defensibility check: Fewer than 2 out-of-scope items → stay in Round 2.

### Round 3 — Acceptance criteria & Risks

Goal: make the spec testable and honest about risk.

```
1. Ask 2–4 adversarial questions:
   - "How will we know this works? What specific test or observation proves it?"
   - "What could go wrong that isn't obvious?"
   - "Is there a scenario where this change is harmful?"
   - "What dependencies could break or block this?"
2. User answers → update acceptance_criteria / open_questions / design_decisions
3. mp interview gaps --checklist-type milestone
4. If gaps remain → loop back to the weakest dimension
5. If clean → proceed to output
```

Defensibility check: No AC with a concrete verification → stay in Round 3.

## Output

After the grilling loop produces a clean gap report:

```
1. mp milestone create --json @-          # write the refined spec
2. mp milestone set-spec-status <id> review
3. Summarize the spec for human review
4. On approval: mp milestone approve <id>
5. mp validate
```

The milestone is now ready for decomposition (Phase 2).

## References

- `mp interview` docs: `docs/concepts/06 - Reference/MP-COMMANDS.md`
- Gap analysis: `mp interview gaps --help`
- Spec model: `docs/SPEC.md`
- Planning sub-mode: `templates/skills/mp-coordinator/planning.md` (stages 1-4 deep-dive, including the Stage 1 milestone interview)
