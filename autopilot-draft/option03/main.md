# Option 03 — 40/60 left-right split

Setup + controls on the left (40%), progress on the right (60%). One screen.

## Layout

```
┌───────────────────┬────────────────────────────────────────────────────────┐
│ Autopilot · setup │  Progress                                              │
│                   │                                                        │
│ Topology          │  ● M236 ⟶ herdr agent prompt opencode  ▰▰▰▱ 4/7        │
│ [1][2][3*]        │     cycle 11 of 17                                      │
│                   │  ● M238 ⟶ waiting on orchestrator       ▰▱▱▱ 1/3        │
│ Harness  ✓uniform │     cycle 1 of 3                                        │
│ Orch (oc|cu|pi)   │  ○ M234 (queued)                                        │
│ Runn  (oc|cu|pi)  │                                                        │
│ Verif (oc|cu|pi)  │  activity                                              │
│                   │  11:14:23 orchestrator → verifier: ok                   │
│ Milestones        │  11:14:24 verifier   → orchestrator: M236/AC-03 ok     │
│ M236 M238 M234    │  11:14:25 orchestrator → runner: M238 step S1          │
│ [ + add ] [ ↻ ]   │  11:14:26 runner     → orchestrator: S1 done            │
│                   │                                                        │
│ Commit policy     │  ────────── telemetry ───────────────────────────────  │
│ (step|find|c|b)   │  lanes 2/3 · cycles 2 · queue 1 · drag↕ to resize═╡    │
│                   │                                                        │
│ ┌───────────────┐ │                                                        │
│ │ ▶ Start (3 ms)│ │                                                        │
│ │ ~12 min est. │ │                                                        │
│ └───────────────┘ │                                                        │
└───────────────────┴────────────────────────────────────────────────────────┘
```

## Interactions

- Mouse: setup buttons on the left, progress rows on the right; click anywhere.
- Keyboard: left half uses arrow keys + Tab; right half can scroll with `j/k`.
- The Start button is anchored to the bottom of the left pane, never moves.
- The right pane grows taller than the left on wide terminals, but it always
  respects a minimum of 18 rows.

## Why this option

- The user sees the progress column fill as a run progresses — visual feedback
  is part of the joy of using the screen.
- A long-running run won't displace setup; setup stays clickable.

## Tradeoffs

- Narrow terminals (< 90 cols) force the left pane to scroll vertically — ugly.
- Mouse hit-testing has to handle ~30 buttons on the left and ~10 rows on the
  right; the registry grows.
- The visual hierarchy puts controls on the smaller side, which feels wrong for
  a control-first design.

## Mitigations

- Layout breakpoint: below 100 cols, drop the per-role harness chips to a
  single row "uniform: (opencode/cursor/pi)".
- Allow collapsing the right pane entirely with `[`, leaving only the left.

## Engineering cost (rough)

- 1 new layout function `render_setup_split(...)` ~200 LOC.
- 1 `progress_section::render(...)` ~180 LOC extracted from the existing
  status view.
- Mouse hit-test table registered for both panes.
- Keybinds unchanged (the keymap fits either pane).
