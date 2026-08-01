# Planning-Only Mode

How to plan without implementing — and where the gate is.

## The gate

There is a hard boundary between **planning** and **execution**:

| Phase | What you can do | Files touched |
|-------|----------------|---------------|
| Planning | Interview, spec, approve, decompose | `master-plan/` only |
| Execution | Implement, test, verify | Application code (`src/`, `tests/`) |

**Never touch application code before `spec_status: ready`.**

## What "plan-only" means

When someone says "just plan it" or "spec this out":

1. **Interview** — `mp interview checklist --checklist-type milestone --draft`
2. **Spec** — `mp milestone create --json @-`
3. **Review** — `mp milestone set-spec-status <id> review`
4. **Approve** — `mp milestone approve <id>`
5. **Decompose** — `mp milestone decompose <id>`
6. **Stop.** Do not set `in-progress`. Do not touch source files.

## How to stay plan-only

- After `mp milestone approve`, exit the workflow
- Do NOT run `mp milestone set-status <id> in-progress`
- Do NOT follow `mp next` into implementation steps
- If asked "implement it", first confirm with user

## Switching modes

```text
Planning → Execution:
  mp milestone set-status <id> in-progress
  mp next 

Execution → Planning:
  mp execution pause
```

## References

- [AGENTS.md](../AGENTS.md) — rule 7 (plan-only mode)
- [AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md) — state transitions
- [EXECUTION-MODES.md](./EXECUTION-MODES.md) — autonomous vs planning
