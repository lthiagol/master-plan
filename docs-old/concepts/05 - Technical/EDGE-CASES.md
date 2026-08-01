# Edge Cases & Uncovered Situations

How to handle situations outside the happy path. Complements [AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md).

**Runtime matrix:** [AGENT-READINESS.md](./AGENT-READINESS.md) — check before calling commands.

---

## Execution

### Step tests fail (red)

Do **not** call `step done`.

```text
1. Fix code or fix plan if step was wrong
2. If step definition wrong → mp step update <m> <s> …
3. If need PM → mp milestone block <m> --reason "S2 tests fail: …"
4. Re-run tests; then mp step done
```

Or record a failed attempt without marking done:

```bash
mp step fail <m> <s> --evidence "…"
```

### Skip a step

User approves skip:

```bash
mp step set-status <m> <s> skipped
mp validate
```

Ensure AC coverage still satisfied (`plan gaps`) or adjust ACs via milestone update.

### Reopen completed milestone

If archived: `mp restore archived <type> <id>`.

Otherwise:

```bash
mp milestone reopen <id> --reason "…"   # spec_status: ready, execution_status: planned
```

History is preserved in `verification`.

### Partial ship / milestone split

Use [GROOMING.md](./GROOMING.md) `mp milestone split` — parent keeps id, children `03.1`, `03.2`.

### Wrong next-step taken

Verify with `mp show milestone` and `mp path` before coding.

---

## Planning

### Duplicate idea or milestone id

**Shipped (M09):** `mp idea create` warns on similar titles (normalized compare).  
Duplicate milestone `id` across files → validate error.

### Orphan milestone file

File in `milestones/` not referenced — **warning W01**. Run `mp sync` to rebuild the index.

### Track outgrew track lane

```bash
mp track promote bugfix BF-03 --to-milestone   # spawns draft milestone
```

### Brief reopen after done

```bash
mp brief reopen   # sets brief.status in_progress, planning_phase brief
```

### Charter drift

Goals in `plan.json` no longer match milestones — **weekly:** `mp interview gaps --type charter`,
`mp plan show`, grooming review.

---

## Brownfield (P4)

### Delta stale after domain bump

G13 blocks `milestone complete`. Run `mp delta rebase <milestone>` after
`mp specs show <domain>`.

### One milestone, two domains

Split into two delta milestones or one milestone with multiple `delta.domain` entries
(**deferred P4.1** — today: one domain per delta).

---

## Multi-agent & concurrency

**ADR-007:** One writer per project per session.

If two agents might run:

- Partition by track vs milestone
- Human PM serializes planning sessions
- Use `mp milestone update --if-updated <date>` — stale writes fail with conflict error

---

## Ops

### Secrets in evidence

Never put API keys/tokens in `evidence` fields. Use: “stored in vault”, CI run URL,
commit SHA.

### Toolkit version mismatch

**Deferred:** doctor compares `mp` version to `MP_HOME/schemas` bundle version.

### Monorepo

**Open decision** — default: one `master-plan/` at repo root. Subprojects use
`--plan-dir` override (document in project AGENTS.md).

---

## References

- [DECISIONS.md](./DECISIONS.md)
- [EMERGENCY.md](./EMERGENCY.md)
- [DESIGN-REVIEW.md](./DESIGN-REVIEW.md)
