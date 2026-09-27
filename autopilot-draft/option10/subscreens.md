# Option 10 — sub-screens

## 1. Preset detail (when a card is highlighted but not yet activated)

```
┌── Heavy preset · details ──────────────────────────────────────────┐
│                                                                   │
│  Topology      3-agent                                           │
│  Harness       uniform · opencode                                 │
│  Milestones    every "ready" milestone (no max cap)              │
│  Commit policy batched (commit at end-of-run)                     │
│  Run mode      detached (recommended for long runs)               │
│                                                                   │
│  best for:    full-fleet certification, batched flywheel runs     │
│                                                                   │
│  est. time     up to 40 min                                       │
│  est. cost     ~$1.20                                             │
│                                                                   │
│           ┌────────────┐  ┌─────────────┐                        │
│           │  Activate  │  │   Cancel     │                       │
│           └────────────┘  └─────────────┘                        │
└───────────────────────────────────────────────────────────────────┘
```

## 2. Preset editor (manage custom presets)

```
┌── Custom presets ─────────────────────────────────────────────────┐
│                                                                  │
│   + New preset                                                   │
│                                                                  │
│   Daily M-series                                                │
│      topology 3-agent · opencode · per-finding · 3 milestones  │
│      [ edit ]   [ delete ]                                      │
│                                                                  │
│   Long weekly                                                    │
│      topology 3-agent · opencode · batched · all-ready           │
│      [ edit ]   [ delete ]                                      │
│                                                                  │
│  Custom presets are stored in ~/.ra_cache/autopilot-presets.json │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

## 3. Picker modal (slide from bottom)

Same picker modal as option 01 — opens when `[ + ]` is clicked in the
configuration grid.

## 4. Run preview (before start)

```
┌── Run preview ─────────────────────────────────────────────────────┐
│                                                                   │
│  Topology         3-agent                                          │
│  Harness          uniform · opencode                              │
│  Milestones       M236 · M238 · M234                              │
│  Commit policy    per-finding                                     │
│  Run mode         normal                                           │
│                                                                   │
│  est. time        ~12 min                                          │
│  est. cycles      17                                               │
│                                                                   │
│         ┌──────────────────┐  ┌─────────────┐                     │
│         │   ▶ Start run     │  │   Cancel     │                     │
│         └──────────────────┘  └─────────────┘                     │
└─────────────────────────────────────────────────────────────────┘
```

## 5. Post-start full progress screen

```
┌────────────── Autopilot · running · Heavy preset ───────────────────┐
│   progress                                                          │
│   ● M236 ⟶ herdr agent prompt opencode  ▰▰▰▱ 4/7   cycle 11  │ │
│   ● M238 ⟶ waiting on orchestrator       ▰▱▱▱ 1/3  cycle  1    │ │
│   ○ M234 (queued)                                               │
│                                                                  │
│   [ pause ]  [ stop ]  [ view findings ]                       │
└──────────────────────────────────────────────────────────────────┘
```

After run completion, the user is returned to the preset+grid screen with
"Recent runs" updated.
