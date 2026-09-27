# Option 08 — sub-screens

## 1. Picker modal (slide from top in Idle mode)

```
┌── Pick milestones ────────────────────────── ✕ ─┐
│  / M236                                         │
│                                                  │
│  ▶ M236   clippy lint fix            selected    │
│    M238   M211 slug + parse dedup    selected    │
│    M234   fixture hygiene            selected    │
│    M231   mp-flow lint                             │
│    M235   consumer-surface hygiene                │
│                                                  │
│  ┌────────────┐  ┌─────────────┐                │
│  │  Confirm   │  │   Cancel    │                │
│  └────────────┘  └─────────────┘                │
└─────────────────────────────────────────────────┘
```

## 2. Run-summary modal (post-completion, Active mode flip-back)

```
┌── Run summary · 3 milestones · 11m 24s ────────────── ✕ ─┐
│                                                            │
│   ✓ M236  done in 4m12s   · 5 findings   · 0 open        │
│   ✓ M238  done in 6m51s   · 3 findings   · 0 open        │
│   ✓ M234  done in 1m02s   · 2 findings   · 0 open        │
│                                                            │
│   cycles 17 · rollbacks 0 · $0.42 est.                    │
│                                                            │
│   ┌─────────────┐  ┌──────────────┐  ┌────────────┐      │
│   │ Re-run same │  │ Edit & re-run│  │ Close      │      │
│   └─────────────┘  └──────────────┘  └────────────┘      │
└────────────────────────────────────────────────────────────┘
```

## 3. Active → Idle confirmation (when run is in flight and user clicks "back to setup")

```
┌── Pause the run? ───────────────────────────────────────────── ✕ ─┐
│                                                                  │
│   Going back to setup will pause the run. You can resume        │
│   from the same point once you're done editing.                  │
│                                                                  │
│       ┌──────────┐  ┌────────────┐  ┌───────────┐                │
│       │  Pause   │  │ Continue   │  │  Cancel   │                │
│       │  & edit  │  │ running    │  │           │                │
│       └──────────┘  └────────────┘  └───────────┘                │
└─────────────────────────────────────────────────────────────────┘
```

The "Continue running" button dismisses the popover without changing state.

## 4. Mode transition animation (~150ms fade)

Skip on slow terminals (auto-detect via `setup-config dim-allowed`).

## 5. Recent-runs row click (drill into a finished run)

```
┌── M225 · 3-agent · aborted F-05 · 11m ─────────────── ✕ ──┐
│                                                            │
│   ✕ aborted at cycle 9 of 17                               │
│   F-05: AC-03 verification failed                          │
│   ├── AC-01 ok                                            │
│   ├── AC-02 ok                                            │
│   ├── AC-03 ✕ (failed) — review/raw F-05 captured        │
│   └── queue: 1 milestone left (M226)                     │
│                                                            │
│   [ re-run from same point ]   [ inspect findings ]       │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

## 6. "Recent runs" persistence

`~/.ra_cache/autopilot-recent.json` — last 10 runs, clickable to re-run.
