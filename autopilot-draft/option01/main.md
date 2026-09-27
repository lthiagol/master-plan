# Option 01 — Setup-first vertical (the canonical proposal)

Single screen, no sub-tabs. Setup panel on top, progress on the bottom.

## Layout (108 cols × 32 rows)

```
┌──────────────────────────────────────────────────────── Autopilot ──────────┐
│  ▶ Topology       [ 1-agent ]  [ 2-agent ]  [ 3-agent ]     gate ✓ herdr   │
│                                                                            │
│  ▶ Harness per role          ✓ uniform                                    │
│       Orchestrator:  ( opencode ) ( cursor )  ( pi )                       │
│       Runner:       ( opencode ) ( cursor )  ( pi )                       │
│       Verifier:     ( opencode ) ( cursor )  ( pi )                       │
│       [ ⋯ advanced override ]                                              │
│                                                                            │
│  ▶ Milestones to run                                                        │
│       selected: M236 · M238 · M234 · M231                                 │
│       [ + add ]   [ ↻ pick ]   [ ✕ clear ]                                │
│                                                                            │
│  ▶ Commit policy                                                           │
│       ( per-step )  ( per-finding )  ( per-cycle )  ( batched )            │
│                                                                            │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │  ▶ Start (resume from failure)                                     │    │
│  │     will run 3 milestones × 2 lanes, ~12 min est.                 │    │
│  │                          ┌──────────────────┐                      │    │
│  │                          │  Start detached  │                      │    │
│  │                          └──────────────────┘                      │    │
│  └────────────────────────────────────────────────────────────────────┘    │
│ ───────────── progress ────────────────────────────────────────────────── │
│  ● M236 ⟶ herdr agent prompt opencode  ▰▰▱▱ 4/7                           │
│  ● M238 ⟶ waiting on orchestrator       ▰▱▱▱ 1/3                         │
│  ○ M234 (queued)                                                            │
│ ──────────────────────────────────────────────────────────────────────────  │
│  11:14:23 started · 12ms queued · lanes 2/3 · cycles 2 · drag↕ resize═╡   │
└────────────────────────────────────────────────────────────────────────────┘
```

## Interactions

- Mouse: any chip/button is clickable; the start button (`▶ Start detached`) is
  a separate accent button.
- Keyboard: arrow keys move focus among button groups; Tab cycles between rows;
  Space / Enter activates the focused chip; `s` starts, `d` detaches, `r` resumes.
- The "⋯ advanced override" link reveals the existing OverridePanel drill-down
  collapsed to its essentials (it's hidden by default).
- The progress region auto-collapses when the run completes; press `[`/`]` to
  fold/unfold manually.

## Why this option

- The canonical shape. No mode flips, no drawers, no wizards.
- Sets up everything on one screen — most clicks stay in the upper half.
- Progress is *read*, not *acted on*; placing it at the bottom keeps the active
  action region at the top.

## Tradeoffs

- The progress region is fixed-height; on narrow terminals the Setup panel
  feels cramped.
- A long activity tail needs a scroll-pane or collapse-to-default-collapsed.
- Mouse-driven clicks vs. keyboard focus model — pick one (see notes.md).
