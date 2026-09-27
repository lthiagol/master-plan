# Option 08 — notes

## Strong points

- The single best signal-to-noise ratio during a run. Active mode shows
  exactly the things that change.
- "Recent runs" row is a daily-driver power feature — re-running a known-good
  config is 1 click.
- Mode flip dramatically reduces "wait, where's the Start button?" confusion.

## Best when

- The audience splits between "tuners" (live mostly in Idle) and "operators"
  (live mostly in Active).
- The user re-runs similar configs often.

## Worst when

- The user wants to monitor + occasionally tweak simultaneously. The mode
  flip forces them to choose.

## Mouse model

Model B (free click + focus) works for Idle. Active mode has only ~5 click
targets and a simpler focus model.

## "Back to setup" during a run: yes / no?

- **Yes (with pause-and-confirm)**: enables real iteration. The "Pause &
  edit" button in the confirmation modal handles state.
- **No (disable the button while running)**: keeps semantics simple.

Recommendation: ship disabled, then enable later. The mode-flip gets a
shipping precedence over the mid-run edit protocol.

## Recent-runs privacy

`Recent runs` should not be persisted across plans (a different project's
runs shouldn't show up). The persistence path uses a plan-scoped key, not
a global user-dir key.

## Engineering cost (rough)

- 2 mode widgets (Idle + Active) ~150 LOC each.
- Mode transition ~50 LOC (or skip animation).
- Recent-runs persistence file ~80 LOC.
- Active-mode focus model ~30 LOC.

## What you give up

- Simultaneous setup + progress viewing.
- The "advanced override" drill-down needs to fit inside Idle mode (currently
  it's a separate reveal).
- Mid-run edits feel risky (mitigated by pause).
