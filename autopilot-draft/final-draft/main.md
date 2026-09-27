# Main screen — autopilot tab

40/60 left-right split. Setup region on the left, tabbed sidebar on the right.

## Layout — 132 cols × 36 rows (realistic desktop terminal)

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
│                                      │  activity tail (last 8 lines):                         │ │
│  Milestones to run                    │  11:14:23  orchestrator → verifier: ok               │ │
│     ■ M236     [ high ]                │  11:14:24  verifier   → orchestrator: M236/AC-03 ok │ │
│     ■ M238     [ high ]                │  11:14:25  orchestrator → runner: M238 step S1     │ │
│     ☐ M234     [normal]                │  11:14:26  runner     → orchestrator: S1 done      │ │
│     [ + select ]  ↻ only-ready         │                                                         │ │
│                                      │  ─────── telemetry ─────────                          │ │
│  Commit policy                        │  lanes 2/3 · cycles 2 · queue 1 · qps 0.4           │ │
│  (per-step) (per-finding*) (per-cycle)│  cost est $0.42 · last update 50ms ago              │ │
│  (batched)                            │                                                         │ │
│                                      │                                                         │ │
│  Run mode                             │                                                         │ │
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

## Regions

### Setup region (left, 40% width, ~52 cols)

Six rows of configuration stacked vertically:

1. **Topology** — radio chips, one of {1-agent, 2-agent, 3-agent}.
2. **Harness per role** — three rows (Orchestrator / Runner / Verifier) each
   with three chips (opencode / cursor / pi); a `✓ uniform` toggle on the
   right; `⋯ advanced override` link below.
3. **Milestones to run** — selected items as compact chips with priority
   tags; `[ + select ]` opens the floating picker; `↻ only-ready` filter.
4. **Commit policy** — four chips, one selected.
5. **Run mode** — two chips (normal / detached).
6. **Start button** — large accent button with a single-line summary.

Below the start button, a **control row** always visible: Pause / Stop /
Resume / ⤴back. Idle-state buttons are dimmed; live-state buttons are active.

### Sidebar region (right, 60% width, ~80 cols)

Vertical stack with **three tabs** at the top: `[ Progress ]` `( Activity )`
`( State )`. The active tab fills the area; the other two are reachable via
Tab key cycling, sidebar collapse arrow, or click.

- **Progress tab** (default) — three milestone rows + activity tail
  (8 lines) + telemetry strip (3 lines).
- **Activity tab** — full activity log (20 lines) + a filter bar at the
  top (orchestrator-only / verifier-only / all).
- **State tab** — read-only dump of `session.json`, `override-panel.json`,
  `.mp/autopilot-state.json` with a `copy` button on each.

## When the run starts

The Setup region doesn't change. The sidebar Progress tab becomes live:
- Status dots pulse (or change color) at every state change.
- Activity tail updates in place, scrolling up.
- Telemetry strip refreshes every 250 ms.

The Start button stays visible but becomes "▶ resume" or "▶ detached" depending
on Run mode. The control row lights up (Pause / Stop / Resume active).

When the run completes, the Start button turns into:

```
┌────────────────────────────────┐
│  ✓  Run completed               │
│  3 milestones · 11m 24s         │
│  ↻ re-run   ✏ edit & re-run    │
└────────────────────────────────┘
```

## Why this shape

- You see setup **and** progress at the same time, every keystroke — the
  setup region never disappears.
- The right sidebar's tab flip lets you *show* the activity tail when you
  want it (default Progress = the top 8 lines; flip to Activity for the
  full feed) without losing setup context.
- The control row below the Start button is *always* visible. Daily-driver
  muscle memory for Pause/Stop doesn't have to learn new keybinds.
- The floating picker (separate document) handles the multi-select milestone
  workflow which doesn't fit in a single row.

## Mouse model

We adopt **Model B** from options 01/02 — free click + focus.
- Click on any chip / button activates it directly.
- Tab + arrow keys / Space / Enter for keyboard-only use.
- Sidebar tabs are clickable; right-edge drag handle resizes the split.
- Floating picker (see `floating-picker.md`) lives above everything else and
  eats mouse events for its area only.
