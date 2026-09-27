# Option 06 — sub-screens

## 1. Picker modal (slide up from bottom)

When user clicks `[ + add ]`:

```
                                                                              ▲
┌── Pick milestones ───────────────────────────────────────── esc ──┐        │
│  / M236                                                          │        │
│  ▶ M236   clippy lint fix                  selected              │        │
│    M238   M211 slug + parse dedup          selected              │        │
│    M234   fixture hygiene                  selected              │        │
│    M231   mp-flow lint                                                   │        │
│    M235   consumer-surface hygiene                                    │        │
│                                                                       │        │
│       ┌────────────┐  ┌─────────────┐                                │        │
│       │  Confirm   │  │   Cancel    │                                │        │
│       └────────────┘  └─────────────┘                                │        │
└──────────────────────────────────────────────────────────────────────┘        │
```

The picker is half-height and pinned to the bottom of the screen, with the
status strip behind it. Closure releases the strip back to its full size.

## 2. Run summary (post-completion)

When the run finishes (or aborts), the entire drawer collapses to a sticky
banner that *replaces* the toolbar:

```
┌────────────────────────────────── Autopilot ─────────────────────────────┐
│ ● M236 ⟶ done in 4m12s · 5 findings · 0 open                            │
│ ● M238 ⟶ done in 6m51s · 3 findings · 0 open                           │
│ ● M234 ⟶ done in 1m02s · 2 findings · 0 open                            │
│ ────────────────────────────────────────────────────────────────────────│
│ ✓ all 3 milestones completed in 11m 24s · 0 failures                      │
│                                                                         │
│   ┌─────────────┐  ┌──────────────┐  ┌────────────┐                    │
│   │ Re-run same │  │ Edit & re-run│  │ Collapse   │                    │
│   └─────────────┘  └──────────────┘  └────────────┘                    │
└─────────────────────────────────────────────────────────────────────────┘
```

## 3. Help / keymap

Press `?`:

```
┌── Autopilot · keymap ──────────────────────────────────────┐
│  Space / Enter activates focused chip                     │
│  Tab cycles chip groups                                   │
│  e / c toggle the setup drawer                            │
│  p     pause                                              │
│  r     resume                                             │
│  s     stop                                               │
│  /     search milestones                                  │
│  ?     this overlay                                       │
│  click any chip activates it (Model B)                    │
└──────────────────────────────────────────────────────────┘
```

## 4. Pause-and-edit flow

When the user clicks "Edit & re-run" or any chip in the drawer mid-run, the
screen fades the activity tail and the toolbar changes:

```
│  ⚠ Run is paused — your edits will apply on the next iteration.        │
│  ┌──────────────────┐  ┌──────────────────────────────┐                │
│  │  Continue (no    │  │  Apply edits & continue       │                │
│  │  edits)          │  │                              │                │
│  └──────────────────┘  └──────────────────────────────┘                │
```

This is a feature beyond the canonical option — see notes.md for the
discussion.
