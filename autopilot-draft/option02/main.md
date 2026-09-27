# Option 02 — Three-tab single screen

`Setup | Controls | Progress` as a top-tab strip. One screen at a time.

## Layout — Setup tab (default)

```
┌──────────────────────────────────────── Autopilot ─────────────────────────┐
│ [ Setup ]  ( Controls )  ( Progress )    gate ✓ herdr   •  fresh •  idle  │
│ ─────────────────────────────────────────────────────────────────────────  │
│  Topology    [ 1-agent ]  [ 2-agent ]  [ 3-agent ]                          │
│                                                                            │
│  Harness per role          ✓ uniform                                     │
│    Orchestrator:  ( opencode ) ( cursor )  ( pi )                         │
│    Runner:       ( opencode ) ( cursor )  ( pi )                          │
│    Verifier:     ( opencode ) ( cursor )  ( pi )                          │
│    [ ⋯ advanced override ]                                                 │
│                                                                            │
│  Milestones      selected: M236 · M238 · M234                             │
│                  [ + add ] [ ↻ pick ] [ ✕ clear ]                          │
│                                                                            │
│  Commit policy  ( per-step ) ( per-finding ) ( per-cycle ) ( batched )   │
│                                                                            │
│                  ┌────────────────────────────────────┐                  │
│                  │  ▶ Start   3 milestones · ~12 min   │                  │
│                  └────────────────────────────────────┘                  │
│ ─────────────────────────────────────────────────────────────────────────  │
│  Tab switches screens · s starts · esc returns · ? help                   │
└────────────────────────────────────────────────────────────────────────────┘
```

## Layout — Controls tab

```
┌──────────────────────────────────────── Autopilot ─────────────────────────┐
│ ( Setup )  [ Controls ]  ( Progress )    gate ✓ herdr   •  live           │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                            │
│       ┌──────────┐  ┌──────────┐  ┌────────────┐  ┌──────────┐             │
│       │ ▶ Start  │  │ ⏸ Pause  │  │  ▶ Resume  │  │ ⏹ Stop   │             │
│       └──────────┘  └──────────┘  └────────────┘  └──────────┘             │
│       ┌──────────┐  ┌──────────────────────┐                             │
│       │ ✕ Abort  │  │  ▶ Start  detached   │                             │
│       └──────────┘  └──────────────────────┘                             │
│                                                                            │
│   Sync actions                                                            │
│       ┌──────────────────────────────┐                                    │
│       │  ↻  resync from on-disk JSON │                                    │
│       └──────────────────────────────┘                                    │
│       ┌──────────────────────────────────────┐                             │
│       │  ✉  re-send orchestrator notification│                             │
│       └──────────────────────────────────────┘                             │
│ ─────────────────────────────────────────────────────────────────────────  │
│  Tab switches · click to activate · ⌫ back to setup                        │
└────────────────────────────────────────────────────────────────────────────┘
```

## Layout — Progress tab

```
┌──────────────────────────────────────── Autopilot ─────────────────────────┐
│ ( Setup )  ( Controls )  [ Progress ]    gate ✓ herdr   •  live           │
│ ─────────────────────────────────────────────────────────────────────────  │
│  ● M236 ⟶ herdr agent prompt opencode  ▰▰▰▱ 4/7    cycle 11 of 17       │
│  ● M238 ⟶ waiting on orchestrator       ▰▱▱▱ 1/3    cycle 1 of 3         │
│  ○ M234 (queued)                                                          │
│ ─────────────────────────────────────────────────────────────────────────  │
│  activity tail                                                            │
│  11:14:23 orchestrator → verifier: ok                                     │
│  11:14:24 verifier   → orchestrator: M236/AC-03 ok                       │
│  11:14:25 orchestrator → runner: M238 step S1                             │
│  11:14:26 runner     → orchestrator: S1 done                              │
│                                                                            │
│  ──────────────────────────────────────────────────────────────────────   │
│  telemetry: lanes 2/3 · cycles 2 · queue 1 · qps 0.4 · drag↕ resize═╡     │
└────────────────────────────────────────────────────────────────────────────┘
```

## Interactions

- Tab strip clickable; `Tab` cycles Setup → Controls → Progress → Setup.
- Each tab inherits the same gate/header line so the screen identity is stable.
- "Progress" is read-only; clicking a row opens a per-milestone drill-down.

## Why this option

- Audience segmentation: status-seekers live on Progress, tuners live on Setup.
- Each tab can be developed and shipped independently (M243 = tab strips, etc.).
- Familiar UI model — chat clients (Slack, Discord) and IDEs use this exact
  pattern.

## Tradeoffs

- Tab switching eats scroll-key muscle memory.
- Status-seekers lose the always-on telemetry strip (it's only in Progress).
- Three full tab implementations → more code, more tests.
