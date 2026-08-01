# Size-Aware Routing — Decision Guide

> **M81:** the first thing a new user learns about `mp` should be its size
> model: **smallest artifact that fits**. The bug-fix-as-16-ACs anti-pattern
> is what `mp` was built to avoid.

## TL;DR

| Change looks like | Reach for | Why |
|--------------------|-----------|-----|
| 1-line bug, single commit, has a verification command | **track (bugfix)** | No interview, no approve — `mp track add bugfix …` → `mp track done bugfix BF-NN`. |
| 1-file tweak, polish, "while you're in there" | **track (tweak)** | Same shape as bugfix; kind=tweak routes to the Tweaks TUI lane. |
| Can't pull it off in a single commit | **milestone** | Acceptance criteria + steps block forces a plan. |
| "We should eventually…" — no shape yet | **idea** | Open ticket, no commitment. |
| Concrete but not-now | **backlog** | Title + priority + resolution; promote to track/milestone when ready. |
| Brownfield fix on existing code | **track (bugfix)** | The repo's routing table already exists in `AGENTS.md`; trust it. |

If in doubt between **track** and **milestone**, **pick track**. You can
always promote later with `mp track promote bugfix <BF-NN> --to-milestone`.

## The 2×2 — clarity × size

```text
                 │  Clear shape        │  Shape unclear
─────────────────┼─────────────────────┼──────────────────────────
Single commit     │  TRACK (bugfix /     │  IDEA   — open ticket,
                 │  tweak)              │  promote to track later
─────────────────┼─────────────────────┼──────────────────────────
Multi-step /      │  MILESTONE           │  BACKLOG — prioritized
crosses scope     │  (interview → spec →  │  candidate; promotes to
                 │  approve → decompose) │  milestone when shape firms up
```

Corner cases belong to the upper-right or lower-left cells. The tool
matches the work:

- **track** keeps ceremony low: title + problem + verification.
  Anyone can read it; `mp reviews pass --all --filter force-bypassed`
  keeps the queue in proportion to actual risk.
- **milestone** makes the work legible: intent, problem, scope in/out,
  acceptance criteria, steps, work packages. Use it when that
  scaffolding pays for itself.
- **idea / backlog** exist so you can capture a thought without the
  tools forcing a fake decision.

## Decision tree (textual)

```text
                   ┌─ can I do this in one commit, with a clear verification?
                   │     yes
                   ▼
              TRACK item (bugfix or tweak)
                   │     no
                   ▼
        ┌─ does this need acceptance criteria,
        │  multiple steps, or visible scope changes?
        │     yes                               no
        ▼                                       ▼
   MILESTONE                          ┌─ can I name ONE thing
   (interview → spec → approve)       │  to do, just not today?
                                       │     yes
                                       ▼
                                  BACKLOG (priority + resolution)
                                       │     no — I can't even name it
                                       ▼
                                IDEA (open ticket)
```

## Why this matters (the anti-pattern)

A 16-step, 8-criterion milestone for a one-line typo fails three ways at
once:

1. **It lies about the work.** Anyone reading the plan has to wade
   through AC scaffolding to find out "turn this flag on."
2. **It stalls on ceremony.** `mp milestone approve` is the gating
   step. A typo doesn't deserve a meeting.
3. **It buries real milestones.** If every bug is a milestone, the
   `mp list milestones` lane becomes noise and the project's actual
   roadmap becomes invisible.

`mp` separates intake by size so that small work stays small and
large work stays accountable.

## Cross-reference

| If you have… | Read | Use |
|--------------|------|-----|
| a one-line fix idea | this doc → "1-line bug" row | `mp track add bugfix …` |
| a vague "later" idea | this doc → "Shape unclear" cell | `mp idea create …` |
| a multi-step feature | this doc → "MILESTONE" cell | `mp interview checklist --type milestone` |
| an existing-code repo | [BROWNFIELD-HANDOFF.md](../05 - Technical/BROWNFIELD-HANDOFF.md) | trust the routing table in `AGENTS.md` |
| emergency hotfix | [EMERGENCY.md](../05 - Technical/EMERGENCY.md) | `mp track add bugfix …`; never bypass gate |

## Internal anchors

This guide is referenced from:

- [AGENT-QUICKSTART.md](./AGENT-QUICKSTART.md) — the entry point for
  every new agent session.
- [README.md](../../../README.md) — repo front-page.
- [00-Concepts.md](../../00-Concepts.md) — overall taxonomy.
