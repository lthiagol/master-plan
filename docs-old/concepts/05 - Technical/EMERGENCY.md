# Emergency & Hotfix Policy

Production down, security incident, or “fix it now” — how Master Plan handles urgency
**without** bypassing spec gates on milestones.

**Decision:** [DECISIONS.md ADR-005](./DECISIONS.md#adr-005-emergency--hotfix-policy)

---

## Policy

| Severity | Route | Spec gate |
|----------|-------|-----------|
| **Small fix** (hours) | `mp track add bugfix` → start → done | Track gates T1–T2 only |
| **Medium** (needs test plan) | Track or expedited milestone | Milestone still needs `ready` before code |
| **Large / behavior change** | Normal milestone + interview | Full gates |

**There is no `mp waiver` or `spec_status: emergency`.** Milestones always require
`ready` before application code changes (G1).

---

## Emergency workflow (agent)

```text
1. mp track add bugfix --title "…" --problem "…" --verification "…"
2. mp track start bugfix BF-XX
3. Fix in code zone
4. mp track done bugfix BF-XX --evidence "…"
5. mp validate
6. After incident: optional mp idea create or milestone for proper hardening
```

If the fix **changes product behavior** beyond a bug restore, promote to milestone
(`mp track promote` P1) or schedule follow-up milestone.

---

## Human PM override

The human may explicitly say “implement without milestone” in conversation. The agent
should:

1. Warn that this violates project rules in `master-plan/AGENTS.md`
2. Prefer track or expedited **review → approve** if any time exists
3. If user insists: proceed in code zone only, then **capture debt** via
   `mp idea create` or `mp backlog add` before ending session

---

## References

- [AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md)
- [EDGE-CASES.md](./EDGE-CASES.md)
