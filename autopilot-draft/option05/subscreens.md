# Option 05 — sub-screens

## 1. Glossary popover (help icon on Step 4)

```
┌── Glossary ────────────────────────────────────────────┐
│  per-step       commit after every step action        │
│  per-finding    commit after every AC verification    │
│  per-cycle      1 commit per cycle boundary            │
│  batched        bundle cycles, commit at end-of-run    │
│                                                       │
│  recommended: per-finding (audit-friendly)            │
│                                                       │
│                            ┌──────────┐              │
│                            │  Close    │              │
│                            └──────────┘              │
└───────────────────────────────────────────────────────┘
```

## 2. Skip-state persistence

The wizard writes each completed step to `.mp/autopilot-state.json`. If the
user closes raul after step 3 and reopens, they land on step 3 with the
earlier selections pre-filled.

```
│  state from previous session detected:                  │
│      step 1 ✓    step 2 ✓    step 3   ← continue       │
│                                                       │
│              ┌──────────────┐  ┌────────────┐        │
│              │  Continue ➜ │  │   Restart   │        │
│              └──────────────┘  └────────────┘        │
└───────────────────────────────────────────────────────┘
```

## 3. Post-start full progress screen

```
┌────────────── Autopilot · running · 3 milestones ───────────────┐
│   progress                                                      │
│   ● M236 ⟶ herdr agent prompt opencode  ▰▰▰▱ 4/7   cycle 11  │
│   ● M238 ⟶ waiting on orchestrator       ▰▱▱▱ 1/3   cycle  1  │
│   ○ M234 (queued)                                                │
│                                                                  │
│   [ pause ]  [ stop ]  [ view findings ]                       │
└──────────────────────────────────────────────────────────────────┘
```

No "back to setup" option while running — too easy to break the run.

## 4. Cycle results drill-down

After a milestone completes, clicking its row opens:

```
┌── M236 · done in 4m 12s ──────────────────────────────────────────┐
│                                                                  │
│  AC-01  clippy passes  ·   exit 0    · 4 of 4                  │
│  AC-02  ...ready path-line ...    ·   exit 0    · 5 of 5        │
│  ...                                                             │
│                                                                  │
│  [ view reviews ]  [ open commit log ]  [ back to run ]          │
└──────────────────────────────────────────────────────────────────┘
```

## 5. Setup-error overlay

If the gate fails (herdr too old, no milestones, etc.) between steps, the
wizard shows the offending step with a red border:

```
┌── Step 2 of 5 ──────────────────────────────────────────────────────┐
│   ▶ Choose a harness per role                                      │
│                                                                   │
│       ⚠ herdr 0.6.x detected — autopilot requires ≥ 0.7.x          │
│       [ install herdr ]  [ cancel ]                                │
└───────────────────────────────────────────────────────────────────┘
```
