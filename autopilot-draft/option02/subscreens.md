# Option 02 — sub-screens

Same picker / advanced override / detached confirm / post-run summary as
Option 01, accessible from the Setup tab.

## 1. Per-milestone drill-down (Progress tab)

Clicking a row in the progress list expands an inline detail panel below the row:

```
│  ● M236 ⟶ herdr agent prompt opencode  ▰▰▰▱ 4/7    cycle 11 of 17
│  ┃  intent:   close recurring M202 doc-overindented-list-items lint
│  ┃  path:     E:\master-plan\crates\mp-model\src\milestone.rs:155-158
│  ┃  findings: 0 open · 0 resolved
│  ┃  next:     F-01 cycle verification at T=00:13
│  ┃  [ view reviews ]  [ view logs ]  [ jump to lane ]
```

No modal — extends the progress row downward.

## 2. Help overlay (`?`)

```
┌── Autopilot · keymap ─────────────────────────────────────────┐
│  Tab           next sub-screen (Setup / Controls / Progress)   │
│  s             start normal                                     │
│  d             start detached                                   │
│  p             pause                                            │
│  r             resume                                           │
│  x             stop / abort                                     │
│  /             search milestones                                │
│  [             fold progress                                    │
│  ]             unfold progress                                  │
│  ⌫ / Esc       back to setup if on Controls/Progress           │
│  ? / F1        this overlay                                     │
│  q             quit                                             │
│  ⏎ any focused chip activates it                              │
│  click any chip activates it (Model B focus model)             │
└──────────────────────────────────────────────────────────────┘
```

## 3. Findings audit drawer (Controls tab → "review findings")

```
┌── Findings audit ─────────────────────────────────────────────┐
│  M236 · cycle 11                                               │
│   ✓ AC-04 lint exit 0                                         │
│   ⋯ AC-05 path_engine emit exit 0 (running)                    │
│                                                                  │
│  M238 · cycle 2                                                │
│   ✓ AC-01 rename exit 0                                       │
│   ⋯ AC-02 dedup exit 0 (running)                              │
│                                                                  │
│  ▶ verify early on M236          ▶ verify early on M238         │
└─────────────────────────────────────────────────────────────────┘
```

A "verify early" button jumps the orchestrator to a verification phase for
that milestone without waiting for the cycle to complete.

## 4. Tab strip persistence

The current active tab is persisted in `.mp/autopilot-state.json`. Reopening
raul restores the last tab.
