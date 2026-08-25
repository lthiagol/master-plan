# Agent instructions — master-plan toolkit

*Your master plan — structured for agents, readable for humans.*

This project uses the **Master Plan** toolkit (`mp` CLI) for spec-driven development.
The plan lives in `master-plan/` and is the source of truth for what to build and in
what order.

## Session start

```bash
mp doctor
mp status
mp next
```

## Key rules

1. **Never edit files under `master-plan/` directly.** Use `mp` for all reads and writes.
2. **Spec before code.** No application changes until `spec_status: ready`.
3. **Reads use JSON** (default stdout). For human display, use `raul` or summarize JSON.
4. **After every write, validate.** `mp validate`
5. **Plan-only mode.** When asked to plan without implementing, stop after `mp` writes.
6. **Never complete on red tests.** Tests gate transitions; `--force` is debt, not a shortcut.
7. **Evidence is test output, not prose.** Record what ran + exit code, never *"test X verifies Y"*.

## Lifecycle

```
Track:    start → done                              (no external review needed)
Milestone: executing → executed → review-ready
                            → in-review → done      (independent review required)
```

`done` for a milestone is reachable **only** via `mp reviews pass` (an independent
pass). `mp milestone complete` enters the review queue — it does not ship. See
`master-plan/AGENTS.md` §3.3b–§3.3d for the full flow, the execution contract, and
the remediation loop.

## Quick reference

> **Pick the smallest artifact first.** One-line bug → `mp track add bugfix …`,
> not a milestone. Prefer track / idea / backlog over a full milestone when the
> work is small; see the toolkit
> [`docs/agent-guide/core-principles.md`](~/.agents/master-plan/docs/agent-guide/core-principles.md).

| Goal | Approach |
|------|----------|
| What to do? | `mp next` |
| Plan a feature | `mp interview checklist --checklist-type milestone --draft` |
| Small fix | `mp track add bugfix --title "..." --problem "..." --verification "..."` |
| Defer / unclear | `mp backlog add ...` or `mp idea create ...` (see SIZE-ROUTING.md) |
| See all work | `mp list milestones` |
| See status | `mp status` |

Full instructions: [master-plan/AGENTS.md](master-plan/AGENTS.md).
Command reference: toolkit
[`docs/mp/commands.md`](~/.agents/master-plan/docs/mp/commands.md).
Agent workflows: toolkit
[`docs/agent-guide/`](~/.agents/master-plan/docs/agent-guide/).
