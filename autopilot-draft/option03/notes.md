# Option 03 — notes

## Strong points

- The user can *watch progress fill* on the right while clicking setup on the
  left; that's the core joy of this shape.
- Start button is in a stable place, never moves.
- Symmetry with IDEs (file tree on left, editor on right).

## Best when

- Terminal width ≥ 100 cols.
- The user runs autopilot a lot and likes seeing it work in real time.

## Worst when

- Terminal < 90 cols — the setup region goes vertical-scroll-only.
- The user is on a tiny terminal and prefers to hide one or the other.

## Mouse model

Model B (free click + focus) is mandatory here — focus can't jump across the
split dynamically; clicking the right-pane progress row shouldn't yank focus
away from a setup form the user is mid-filling.

## What you give up vs. option 01

- Less vertical real estate for setup on the same size screen.
- The progress region is fixed at 60% — the user might want 80% progress.

## Risk

Hit-test rectangles are easy in `view_state.rs` but the layout split has to be
recomputed on every terminal resize. The existing `compute_*_rects` family has
to be extended with a `compute_split_2_rects(left_pct, right_pct)`.

## Engineering cost (rough)

- New split renderer ~250 LOC; existing render functions largely reused.
- ~10 lines added to `view_state.rs` for the split-rect math.
- Same mouse registrations as option 01 plus ~20 more for the right pane rows.
