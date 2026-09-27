# Option 05 — notes

## Strong points

- Lowest mistake-rate by construction (one decision per screen).
- Most testable: each step is an isolated widget + reducer.
- Easy to onboard: "I see 5 boxes to fill in order."
- Each step can ship independently (M243a = step 1, M243b = step 2, etc.).

## Best when

- New users running autopilot for the first time.
- Auditors who want every choice explicitly recorded (the wizard produces an
  audit trail naturally).

## Worst when

- Daily-driver users — 5 transitions per run is friction.
- Ad-hoc runs where the user knows what they want and just wants to fill it
  in fast.

## Mouse model

Model A (focus + click) is fine here. The wizard is sequential; focus jumps
naturally between Next/Back buttons.

## Step skipping

A common variant: the user can `/` jump to a step by typing the step number
or clicking the stepper header. Add `g step-number` for keyboard users
(like vim's goto-line).

## Engineering cost (rough)

- 5 step screens, ~80-150 LOC each.
- 1 wizard reducer with state machine ~100 LOC.
- Step persistence file `.mp/autopilot-wizard-state.json` ~50 LOC.
- Step indicator / breadcrumb ~50 LOC.

## What you give up

- A "configure everything at once" experience.
- The ability to see setup and progress side by side.
- Speed for known users.

## Sweet-spot variant

Use the wizard **only on the first launch per project**. After the user
completes it once and the file persists, the main tab becomes option 01
(setup-first vertical). The wizard is the "first time" UX; the canonical
shape is the daily-driver UX.
