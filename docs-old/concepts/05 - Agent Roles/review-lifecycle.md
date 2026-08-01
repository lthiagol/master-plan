# Review Lifecycle — Two-Round Review Policy

**Foundation:** M119 (Agent role dispatch + skill registry foundation)

## Role → State Binding

The two-round review policy maps agent roles to M100 lifecycle states:

| Round | Agent Role | Lifecycle State | Description |
|-------|-----------|-----------------|-------------|
| 1 | **Runner** | `self-reviewed` | The executing agent reviews their own work. This is a useful first pass — catches obvious defects, verifies evidence, confirms the AC-step contract. |
| 2 | **Coordinator** | `reviewed` | A separate session (coordinator role) performs the final review. The coordinator verifies the runner's self-review wasn't blind to its own assumptions. |

## Session-Boundary Discipline

**Source:** `docs/code-review-lessons.md` L5 ("The author should not be the only reviewer").

The same session that wrote the change should never be the final reviewer. In practice this means:

1. A runner session that completes execution marks the milestone as `self-reviewed`.
2. The coordinator picks up `self-reviewed` milestones and either passes or fails the human-facing review.
3. If a single human plays both roles, the coordinator review MUST happen in a **separate session** from the runner session.
4. The session boundary forces a context reset: the coordinator reads the milestone spec fresh, without the runner's execution tunnel-vision.

## Protocol

```
Runner session (round 1):
  1. Execute milestone steps
  2. Verify AC evidence
  3. mp milestone complete → state flips to self-reviewed (or mp milestone ac pass → per-AC)
  4. Handoff to review queue

Coordinator session (round 2):
  1. mp reviews pending → discover self-reviewed milestones
  2. Read the milestone spec + diff (fresh eyes)
  3. Run findings pass (challenge audit or manual review)
  4. mp reviews pass <id> --verdict approved → state flips to reviewed
  5. mp reviews pass <id> --verdict changes-requested → back to in-progress
```

## Cross-References

- **M120** (mp-flow cross-role orchestration): the 12-stage timeline that embeds round-1 and round-2 as distinct phases.
- **M121** (mp-coordinator role skill): the coordinator's reviewing sub-mode and lesson-pattern reference.
- **M122** (mp-runner role skill): the runner's execution, self-review, and handoff procedures.
- **M123** (mp-handoff protocol): the hand-off ceremony between runner and coordinator sessions.
- **M100** (Unified milestone lifecycle): the state machine that `self-reviewed` and `reviewed` belong to.
- **M101** (Findings model): the `phase` field (`self` vs `external`) that maps to these two rounds.

## Related Docs

- `docs/code-review-lessons.md` — complete lesson catalog (L1–L59). L5 is the session-boundary anchor.
- `docs/concepts/05 - Agent Roles/` — role concept docs directory (this file).
- `AGENTS.md` (plan root) — session-start checklist for role-aware agents.
