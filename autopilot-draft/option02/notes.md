# Option 02 — notes

## Strong points

- Mixed audience (status-seekers + tuners + operators) are happy — each gets a
  screen they live in.
- Each tab is testable in isolation — smaller renders, smaller state subsets.
- The `Setup / Controls / Progress` ordering matches the mental model of most
  CI / pipeline UIs.

## Best when

- Different roles touch autopilot (operator / tuner / spectator).
- The tab is open in a long-lived terminal session.

## Worst when

- The user has to switch tabs constantly (every micro-action).
- Terminal width < 90 cols (tab strip eats space).

## Engineering cost (rough)

- 3 tab renderers, ~250 LOC each.
- 1 `active_tab: AutopilotTab` field on `AutopilotLaneState`.
- 1 `persisted_state.json` writer/loader.
- Existing keybind registry extended — 6 new bindings (Tab, s, d, p, r, x).
- No backend changes.

## What you give up

- The "always-on" telemetry strip that the canonical option has. Status-seekers
  must switch tabs to see lanes 2/3 / cycles 2.
- Mouse-only quick actions like "abort" require going to Controls.

## Mouse model

Model B from option 01 (free click + focus) works best here, because the tab
strip + each button row both need to be click targets.
