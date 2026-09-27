# Option 01 — notes

## Strong points

- **Lowest cognitive load**: everything visible at once, no hidden drawers.
- **Mouse-driven end-to-end** is achievable without changing the existing focus
  model — every chip is a hit-test rect.
- **Keyboard-driven end-to-end** is achievable without changing the existing
  keybinds registry — every chip is reachable via Tab + arrow keys.

## Best when

- The audience knows what autopilot is doing and wants a daily-driver screen.
- Terminal width is generous (108+ cols).

## Worst when

- Terminal width is < 90 cols (the harness chip grid starts to wrap ugly).
- The user mostly wants to read telemetry, not run things (status-first users
  will feel the setup is in the way).

## Mouse vs. keyboard model — pick one

Two options, both viable, you have to choose:

- **Model A — Focus + click.** Moving focus with Tab highlights a chip;
  clicking anywhere hits the chip *whose focus state matches*. Single-source
  truth in `view_state.focus_target`. Easier to test.
- **Model B — Free click + focus.** Clicking always activates the chip under
  the cursor regardless of focus. Tab only changes keyboard focus. Closer to
  a browser / IDE model.

This option works with either; both are equally implementable. The choice is
purely editorial.

## Engineering cost (rough)

- 1 new state field for currently-selected chip group.
- 1 new state field for "advanced override expanded".
- `lane_lists.rs::render_autopilot_tab` rewritten: ~400 LOC → ~250 LOC of new
  geometry + ~150 LOC of unchanged telemetry.
- Mouse hit-test table populated (already populated for the lanes).
- No backend changes; the button labels are read from the existing
  `AutopilotLaneState` and `OverridePanel`.

## Risk

The biggest risk is the **layout under the wireframe**: a real implementation
might need to trim the harness chip grid to fit narrow screens. The fallback
is to drop the "role labels" and render the harness chips in a 1-row strip.
