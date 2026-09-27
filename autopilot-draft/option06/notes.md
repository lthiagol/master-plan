# Option 06 — notes

## Strong points

- Most ticker-friendly. Always-on status is the default.
- Drawer hides under 80 cols; the rest is fine.
- Pause/Stop prominence keeps dangerous actions *available* (cf. option 01
  which hides them).

## Best when

- The user keeps the tab open all day.
- Terminal width is constrained.
- Run-state changes (pause/stop) are common operations.

## Worst when

- The user runs autopilot and then walks away — they only saw the drawer
  open twice a day, so they don't remember the keymap.
- Status-seekers want an in-place "edit config during run" path — possible
  here, but the drawer is the *only* path; users have to know that.

## Pause-and-edit: yes or no?

This option enables the most aggressive feature: editing config *mid-run*.
The trade-off is real:

- **Yes**: allows real iteration. Engineers change harness when one keeps
  flaking, change commit policy mid-cycle, etc.
- **No**: keeps semantics simple. Config is fixed at run start; changes
  apply on next run.

Recommendation: ship without pause-and-edit at first; close the door after
the option is stable. The "Apply edits & continue" button (option 06
subscreen 4) is a follow-up.

## Mouse model

Model B (free click + focus) because Pause/Stop must always be reachable
and the drawer toggle is a free-position element.

## What you give up

- Every option you change requires opening the drawer. No "see everything."
- Pause/Stop proximity to Run encourages fat-finger accidents. Mitigate with
  a confirmation popover for Stop.

## Engineering cost (rough)

- 1 collapsible drawer widget ~120 LOC.
- Existing keybinds extended: `e` / `c`.
- 1 confirmation popover for Stop ~50 LOC.
- `.mp/autopilot-state.json` field for drawer-open boolean.
- Status always-on strip — extracted from the existing view, ~100 LOC.
