# Agent Quickstart

Fast session-start to plan-ready path. Copy-paste these commands.

> **M81: pick the right artifact for the size of work FIRST.** `mp` has
> five intake surfaces: **track**, **milestone**, **idea**, **backlog**,
> and **bugfix** (a kind of track item). Trying to file a one-line bug
> fix as a milestone — with its spec/interview/approve/decompose ritual —
> is the exact anti-pattern `mp` exists to prevent. Default to the
> smallest one that fits. See [SIZE-ROUTING.md](./SIZE-ROUTING.md) for the
> decision matrix; the rest of this doc points you at the matching flow.

## One-time setup

```bash
mp init --profile full          # creates master-plan/ in repo root
mp doctor                       # verify everything is in place
```

## Every session

```bash
mp status                       # where are we?
mp next                         # what to do now?
```

## Common flows (smallest → largest)

### Small fix (track) — 1-line bug or polish

> **Default this.** If the change fits in a single patch and has clear
> verification, a track is faster and cleaner than a milestone.

```text
1. mp interview checklist --checklist-type track-item --kind bugfix
2. mp track add bugfix --title "..." --problem "..." --verification "..."
3. mp track start bugfix BF-01
4. (implement)
5. mp track done bugfix BF-01 --evidence "..."
```

### Tweaks (track, kind=tweak)

Same shape as bugfix, but the change is non-urgent polish rather than a
defect. Lives in the **Tweaks** TUI lane, not Bugfixes.

```text
1. mp track add tweak --title "..." --problem "..." --verification "..."
2. mp track start tweak TW-01
3. mp track done tweak TW-01 --evidence "..."
```

### Defer / unclear (backlog or idea)

If the work is large or the shape isn't yet clear, don't open a
milestone. Park it.

```text
# Backlog — concrete but not now (priority + resolution tracked)
mp backlog add --title "..." --priority med

# Idea — vague / later (open ticket, no commitment)
mp idea create --title "..."
```

### New feature (milestone) — multi-step, requires planning

Reach for a milestone only when:

- The work spans **multiple steps** (≥ 2 days), **or**
- It needs **acceptance criteria** that multiple agents will share, **or**
- It crosses **scope boundaries** (intent/problem/in-scope/out-of-scope
  matter).

```text
1. mp interview checklist --checklist-type milestone --draft
2. (answer questions with user)
3. mp milestone create --json @-  <<'EOF'
{"title": "...", "intent": {"outcome": "..."}, "problem": {"description": "..."},
 "scope": {"in_scope": [...], "out_of_scope": [...]},
 "acceptance_criteria": [{"description": "...", "verification": "..."}]}
EOF
4. mp milestone approve <id>
5. mp milestone decompose <id>
6. mp path
```

### Planning only (no code)

```text
1. mp interview checklist --checklist-type milestone --draft
2. mp milestone create --json @-
3. mp milestone set-spec-status <id> review
4. (user approves)
5. mp milestone approve <id>
6. Stop. No code.
```

## Format cheat sheet

| Use case | Flag |
|----------|------|
| Agent reads | `mp <cmd>` (JSON default) |
| User display | `raul` or summarize JSON |
| Validate after writes | `mp validate` |

## Where to go next

- [SIZE-ROUTING.md](./SIZE-ROUTING.md) — the decision matrix this doc
  built on. Read first when in doubt.
- [AGENTS.md](../../../AGENTS.md) — complete session instructions (toolkit repo)
- [AGENT-READINESS.md](../01%20-%20Agent%20Integration/AGENT-READINESS.md) — command matrix
- [AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md) — state transitions
- [PLANNING-ONLY-MODE.md](./PLANNING-ONLY-MODE.md) — plan vs execute gate
- [BROWNFIELD-HANDOFF.md](../05 - Technical/BROWNFIELD-HANDOFF.md) — existing-code projects
