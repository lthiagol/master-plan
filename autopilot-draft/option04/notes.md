# Option 04 — notes

## Strong points

- The most user-onboarding-friendly shape — five big cards instead of a
  routing problem.
- Marketing-friendly. Easy to demo in screencasts.
- Pre-named presets (1-agent = "lightweight", 3-agent = "full") match the
  mental model of CI pipelines.

## Best when

- The audience includes "first-time" users who haven't internalized the
  orchestrator↔runner↔verifier mental model.
- The tab is used as a daily-driver by both new and experienced users.

## Worst when

- The user wants to set up an unusual configuration; the 5-card grid forces
  them into the modal anyway.
- The user runs autopilot continuously and needs an always-on status view.

## Variants worth considering

- Replace one of the cards with a **Recent runs** card showing the last 5
  runs (click → re-launch that exact configuration).
- Add a **Custom** card (6th) that opens directly to the configure modal
  with no preset locked in.

## Mouse model

Model B (free click + focus) is mandatory. The hub treats each card as a
discrete click target; modal popups are clickable inside.

## Engineering cost (rough)

Same as option 01 plus:
- Hub widget ~150 LOC.
- 5 modal "configure" variants ~120 LOC each (most of which is reused).
- Hub animations (subtle, optional) for live/disabled states.

## What you give up vs. option 01

- A "fill everything on one screen" experience — the user has to click a card
  AND fill in a modal for most actions.
- The setup widget is modal, not in-place — there's no way to see setup and
  progress at the same time during a run.
