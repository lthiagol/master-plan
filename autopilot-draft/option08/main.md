# Option 08 — Dual-mode (Idle / Active flip)

Single screen, but visually distinct based on state. Idle = setup panel.
Active = progress-first with minimal controls.

## Layout — Idle mode (default)

```
┌──────────────────────────────────────── Autopilot · IDLE ────────────────┐
│   ▶ Topology      [1][2][*3]                                              │
│                                                                            │
│   ▶ Harness       uniform · opencode / cursor / pi                        │
│                                                                            │
│   ▶ Milestones    M236 · M238 · M234   [ + ] [ ↻ ]                        │
│                                                                            │
│   ▶ Commit policy per-finding                                              │
│                                                                            │
│   ▶ Run mode       ( normal run )  ( detached )                            │
│                                                                            │
│   ▶ Recent runs                                                            │
│       ✓ yest  3-agent M200..  11m 24s                                    │
│       ✓ 2d    1-agent M178         2m 51s                                 │
│       ✕ 3d    3-agent M225        ⏹ aborted F-05                          │
│                                                                            │
│              ┌────────────────────────────────────────────────┐         │
│              │      ▶ Start run · will run 3 milestones · ~12m   │         │
│              └────────────────────────────────────────────────┘         │
└──────────────────────────────────────────────────────────────────────────┘
```

When the user clicks Start, the screen flips:

## Layout — Active mode

```
┌───────────────────────────────────── Autopilot · LIVE ─────────────┐
│                                                                    │
│  Topology     3-agent · uniform · opencode · per-finding commits    │
│  Run-mode     detached                                              │
│                                                                    │
│  ● M236 ⟶ herdr agent prompt opencode  ▰▰▰▱ 4/7     cycle 11 of 17 │
│  ● M238 ⟶ waiting on orchestrator       ▰▱▱▱ 1/3     cycle  1       │
│  ○ M234 (queued)                                                     │
│                                                                    │
│  activity tail                                                       │
│  11:14:23 orchestrator → verifier: ok                                │
│  11:14:24 verifier   → orchestrator: M236/AC-03 ok                  │
│  11:14:25 orchestrator → runner: M238 step S1                       │
│  11:14:26 runner     → orchestrator: S1 done                        │
│                                                                    │
│  ────────── telemetry ────────────────────────────────────────────│
│  lanes 2/3 · cycles 2 · queue 1 · last update 50ms ago · $0.42    │
│                                                                    │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
│  │ ⏸ Pause   │  │ ⏹ Stop   │  │ ↻ Resync │  │ ⤴ back to setup  │  │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘  │
└────────────────────────────────────────────────────────────────────┘
```

## Mode transitions

- **Idle → Active**: Start button → screen transition (fade or slide).
- **Active → Idle**: "back to setup" → no fade, instant flip. (Disabled while
  a run is mid-flight? See notes.)
- **Active → Active**: Pause / Stop / Resync → no transition; just state.
- **Active (done)**: After the run completes, auto-flip back to Idle,
  pre-filling "Recent runs" with the just-completed entry.

## Why this option

- During a run, the *only* thing that matters is progress. Why show setup?
- Mode flip is a strong visual signal — the screen *looks* different.
- Each mode is small (Idle = setup panel; Active = progress-first strip).
  Less code than the canonical option.

## Tradeoffs

- Mid-run edits are awkward; the user has to "back to setup" which feels
  suspicious if the run is still going.
- Mode state needs to be persisted so the user can refresh / reopen.
- The "back to setup" button during a run needs to be wired to the runner's
  mid-run edit protocol (which doesn't exist today).
