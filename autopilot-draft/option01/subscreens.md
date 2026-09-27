# Option 01 — sub-screens

## 1. Picker (modal)

Opens when the user clicks `[ + add ]` next to "Milestones to run" or when
they type `/` while the milestone row has focus.

```
┌── Pick milestones ─────────────────── ✕ ──┐
│  / M236                                  │
│  ▰ M236 — M202 clippy lint fix           │
│  ▰ M238 — M211 slug + parse dedup        │
│  ▰ M234 — fixture hygiene                 │
│  ▱ M231 — mp-flow lint                    │
│  ▱ M235 — consumer-surface hygiene        │
│  ⌃ j/k move · space toggle · ⏎ confirm    │
└────────────────────────────────────────────┘
```

Confirm → the main screen's "selected:" line updates.

## 2. Advanced override (reveal)

The `[ ⋯ advanced override ]` link in the Harness row expands inline below the
3-chip grid:

```
│  ▶ Harness per role          ✓ uniform
│       Orchestrator:  ( opencode ) ( cursor )  ( pi )
│       Runner:       ( opencode ) ( cursor )  ( pi )
│       Verifier:     ( opencode ) ( cursor )  ( pi )
│       [ ⋯ hide advanced ▾ ]
│       ┌── Advanced ──────────────────────────────────┐
│       │  Topology:  3-agent                          │
│       │  Orchestrator role:  planner                │
│       │  Runner role:        implementer             │
│       │  Verifier role:      reviewer               │
│       │  Topo id:            three-agent-001        │
│       │  Extras:             { "shell": "bash" }   │
│       │  [ reset to defaults ]                       │
│       └──────────────────────────────────────────────┘
```

This is the existing `OverridePanel` content collapsed to a 6-line summary
that can be edited via the same chips.

## 3. Detached confirm

When starting in detached mode, the Start button reveals a confirmation row
so users understand "this will keep running after I close raul":

```
│  ▶ Start (resume from failure)
│     will run 3 milestones × 2 lanes, ~12 min est.
│     ⓘ Detached — the autopilot child will survive raul exit. Continue?
│                         ┌──────────┐  ┌─────────────┐
│                         │  Start   │  │   Start +   │
│                         │  normal  │  │  detached   │
│                         └──────────┘  └─────────────┘
```

## 4. Post-run summary

After the run completes (or aborts), the entire progress region collapses to:

```
│ ───── summary ────────────────────────────────────────────────
│  ✓ completed 3 milestones in 11m 24s
│  ↳ cycles 17 · findings 5/5 · cost est $0.42
│  [ view logs ]  [ re-run ]  [ back to setup ]
```

The "what now" buttons replace Start until the user clicks back to setup.
