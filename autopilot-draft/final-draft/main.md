# Main screen — autopilot tab

40/60 left-right split. Setup region on the left, tabbed sidebar on the right.

When a run is live, the screen flips to the **While Executing** view (see
`subscreens.md`) — the split is replaced with a dedicated full-tab run
dashboard. The State tab disappears during a run.

## Layout — 132 cols × 36 rows (idle / pre-run state)

```
┌──────────────────────────────────────┬─────────────────────────────────────────────────────────┐
│  Autopilot · setup                    │  [ Progress ]  ( Activity )  ( State )              │ │
│ ───────────────────────────────────── │ ────────────────────────────────────────────────────────── │
│                                      │                                                         │ │
│  Topology                             │  ●  M236  ⟶  herdr agent prompt opencode          │ │
│  ◯ 1-agent  ◯ 2-agent  ● 3-agent      │            ▰▰▰▱▱▱▱▱  cycle 11 of 17                  │ │
│                                      │  ●  M238  ⟶  waiting on orchestrator                │ │
│  Harness per role     ✓ uniform      │            ▰▱▱▱▱▱▱▱  cycle  1 of 3                   │ │
│     Orchestrator:  ( opencode )(cursor)(pi)                                    │ │
│     Runner:        ( opencode )(cursor)(pi)  ○  M234  ⟶  queued                  │ │
│     Verifier:      ( opencode )(cursor)(pi)                                    │ │
│     ⋯ advanced override               │                                                         │ │
│                                      │  activity tail (auto-sized):                          │ │
│  Milestones to run                    │  11:14:23  orchestrator → verifier: ok               │ │
│     ■ M236     [ high ]                │  11:14:24  verifier   → orchestrator: M236/AC-03 ok │ │
│     ■ M238     [ high ]                │  11:14:25  orchestrator → runner: M238 step S1     │ │
│     ☐ M234     [normal]                │  11:14:26  runner     → orchestrator: S1 done      │ │
│     [ + select ]  ↻ only-ready         │  11:14:27  orchestrator → verifier: ok               │ │
│                                      │  11:14:28  verifier   → orchestrator: M236/AC-04 ok │ │
│  Commit policy                        │  11:14:29  orchestrator → runner: M238 step S2     │ │
│  (per-step) (per-finding*) (per-cycle)│                                                         │ │
│  (batched)                            │  ─────── telemetry ─────────                          │ │
│                                      │  lanes 2/3 · cycles 2 · queue 1 · qps 0.4           │ │
│  Run mode                             │  cost est $0.42 · last update 50ms ago              │ │
│  ( normal run ) ● detached            │                                                         │ │
│                                      │                                                         │ │
│  ┌────────────────────────────────┐  │                                                         │ │
│  │  ▶ Start run                   │  │                                                         │ │
│  │  3 milestones · ~12 min est.   │  │                                                         │ │
│  └────────────────────────────────┘  │                                                         │ │
│                                      │                                                         │ │
│  ── control row ──                   │                                                         │ │
│  ⏸ Pause   ⏹ Stop   ↻ Resume   ⤴back   │                                                         │ │
│  (greyed out if no live run)         │                                                         │ │
└──────────────────────────────────────┴─────────────────────────────────────────────────────────┘
```

## Layout — While Executing (a run is live)

When the user clicks Start, the screen **flips** to the While Executing view
(see `subscreens.md` §1 for the full layout). The split disappears; the
entire tab becomes a focused run dashboard.

```
┌─ Autopilot · LIVE · 3-agent · uniform · opencode · per-finding ─────────────┐
│                                                                              │
│  ●  M236  ⟶  herdr agent prompt opencode                                    │
│            ▰▰▰▰▰▰▱▱  cycle 14 of 17 · est. 7m 22s left                        │
│     intent:  close the recurring false-positive at                           │
│              crates/mp-model/src/milestone.rs:155-158                        │
│                                                                              │
│  ●  M238  ⟶  waiting on orchestrator                                        │
│            ▰▱▱▱▱▱▱▱  cycle  1 of 3 · est. 6m 51s left                         │
│                                                                              │
│  ○  M234  ⟶  queued · starts when M238 cycles finish                        │
│                                                                              │
│  ────── activity ──────                                                    │
│  11:14:25  orchestrator → runner: M238 step S1                            │
│  11:14:26  runner     → orchestrator: S1 done                            │
│  11:14:27  orchestrator → verifier: ok                                     │
│  11:14:28  verifier   → orchestrator: M236/AC-04 ok                       │
│  11:14:29  orchestrator → runner: M238 step S2                            │
│  11:14:30  runner     → orchestrator: S2 done                            │
│  11:14:31  orchestrator → verifier: M236/AC-05 in_flight                  │
│                                                                              │
│  ────── telemetry ──────                                                   │
│  lanes 2/3 · cycles 14 · queue 1 · qps 0.4 · cost $0.42                    │
│  last update 50ms ago                                                       │
│                                                                              │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌───────────────────────┐  │
│  │ ⏸ Pause     │ │ ⏹ Stop      │ │ ↻ Resync    │ │ ⤴ Edit & re-run   │  │
│  └─────────────┘ └─────────────┘ └─────────────┘ └───────────────────────┘  │
│                                                                              │
│  ⏎ on highlighted row → peek view (reviews, logs, commits)                │
│  Esc → back to setup (with detach confirm if not attached)                  │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Tab visibility per state

The sidebar tabs in the idle / pre-run state:

- **Progress** — visible, default tab.
- **Activity** — visible.
- **State** — visible. *Hidden during a live run* (its content moves into the
  While Executing "peek view").

Once a run is live:
- The sidebar tabs disappear.
- The While Executing screen takes over the full tab.

## Click on a milestone in While Executing — opens the peek view

Same peek view as `subscreens.md` §4 — focused on the milestone's intent,
ACs, and per-cycle activity. Returns to While Executing on close.

## Click on "Edit & re-run" (or `s` while running)

When `Edit & re-run` is pressed mid-run, raul prompts:

```
│  ⚠ Going back to setup pauses the current run.                │
│    ┌──────────┐  ┌────────────┐                                │
│    │  Pause   │  │  Continue  │                                │
│    │  & edit  │  │  running   │                                │
│    └──────────┘  └────────────┘                                │
```

After Pause & edit, the idle split view returns with the previous
configuration pre-filled.

## Why this shape

- You see setup **and** progress at the same time while idle — the
  setup region never disappears until you start.
- Once a run is live, the While Executing screen gives the live state
  the full vertical space it deserves — no more cramming live progress
  under a 60% sidebar.
- After the run completes, the screen returns to the split view with
  the previous configuration pre-filled so you can immediately start the
  next run.

## Mouse model

We adopt **Model B** (free click + focus).
- Click on any chip / button activates it directly.
- Tab + arrow keys / Space / Enter for keyboard-only.
- Sidebar resize: drag the right-edge divider of the setup region
  (limits 25-75% per Q4).
- Floating picker lives above everything else and eats mouse events
  for its area only.
