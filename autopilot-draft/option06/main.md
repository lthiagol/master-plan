# Option 06 — Compact control bar + drawer

Status always visible at the top. Setup panel slides down as a drawer.

## Layout — collapsed (default)

```
┌──────────────────────────────────────── Autopilot ────────────────────────────┐
│ ● M236 ⟶ herdr agent prompt opencode  ▰▰▰▱ 4/7      cycle 11  ⏱ 4m12s        │
│ ● M238 ⟶ waiting on orchestrator       ▰▱▱▱ 1/3      cycle  1              │
│ ○ M234 (queued)                                                               │
│ ──────────────────────────────────────────────────────────────────────────  │
│ ┌─────────────────────────┐  ┌────────────┐  ┌────────────┐                │
│ │  ⏸ Pause  ⏹ Stop  ▶Run │  │ ▾  Setup  │  │    ⏎ /click│ ← drawer toggle   │
│ └─────────────────────────┘  └────────────┘  └────────────┘                │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Layout — drawer expanded

```
┌──────────────────────────────────────── Autopilot ────────────────────────────┐
│ ● M236 ⟶ herdr agent prompt opencode  ▰▰▰▱ 4/7      cycle 11  ⏱ 4m12s        │
│ ● M238 ⟶ waiting on orchestrator       ▰▱▱▱ 1/3      cycle  1              │
│ ○ M234 (queued)                                                               │
│ ────────────────────────────────────────────────────────────────────────── │
│ ▾ setup (single-line at the top, click ▸ to expand into a drawer)            │
│ ┌─────────────────────────────────────────────────────────────────────┐    │
│ │  Topology      [ 1-agent ]  [ 2-agent ]  [ *3-agent ]                │    │
│ │  Harness       uniform  ( opencode ) ( cursor ) ( pi )                │    │
│ │  Milestones    M236 · M238 · M234   [ + add ]                          │    │
│ │  Commit        ( per-finding )    [edit]                              │    │
│ │  Run mode      ( normal run )  ( detached )                            │    │
│ │                                                                      │    │
│ │  est. time     ~12 min                                               │    │
│ │  est. cycles   17                                                      │    │
│ │                                                                      │    │
│ │            ┌──────────────────────────────────┐                       │    │
│ │            │  Start detached · will run 3 ms  │                       │    │
│ │            └──────────────────────────────────┘                       │    │
│ └─────────────────────────────────────────────────────────────────────────┘    │
│ activity tail                                                                 │
│ 11:14:23 orchestrator → verifier: M236/AC-03 ok                              │
│ 11:14:25 orchestrator → runner: M238 step S1                                │
│ 11:14:26 runner     → orchestrator: S1 done                                  │
└────────────────────────────────────────────────────────────────────────────┘
```

## Interactions

- Mouse: clicking `▸ Setup` expands the drawer; clicking `▾ Setup` collapses.
- Keyboard: `e` expands, `c` collapses; arrow keys + Tab inside the drawer.
- The control bar (Pause / Stop / Run / Setup) is always visible.
- Drawer state is persisted per `.mp/autopilot-state.json`.

## Why this option

- Maximum terseness for users who keep the tab open all day.
- Always-on telemetry (the top strip) is the headline; setup is rare.
- Fits a 80-col terminal better than the canonical option.

## Tradeoffs

- Drawer opens in the middle of the screen; the activity tail is forced to
  grow upward.
- A user mid-run who wants to change topology has to know about the drawer.
- "Pause/Stop" buttons in the always-on bar are dangerous if you fat-finger.

## Engineering cost (rough)

- 1 collapsible drawer widget ~120 LOC.
- Existing keybind registry extended by `e` / `c`.
- 1 `.mp/autopilot-state.json` field for drawer-open boolean.
- Pause/Stop buttons reused from existing actions.
