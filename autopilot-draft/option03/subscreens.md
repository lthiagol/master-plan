# Option 03 — sub-screens

Same picker / advanced override / detached confirm / post-run summary as
Option 01, but they overlay the entire screen (modal, not pane-anchored).

## 1. Picker modal (slides from the left edge)

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
│ Pick milestones                                          esc to close    │
─────────────────────────────────────────────────────────────────────────────
│ / M236                                                                  │
│                                                                              │
│ ▶ M236  clippy lint fix                  selected                          │
│   M238  M211 slug + parse dedup          selected                          │
│   M234  fixture hygiene                  selected                          │
│   M231  mp-flow lint                                                           │
│   M235  consumer-surface hygiene                                                │
│                                                                              │
│       ┌────────────┐  ┌─────────────┐                                  │
│       │  Confirm   │  │   Cancel    │                                  │
│       └────────────┘  └─────────────┘                                  │
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

The modal blocks mouse clicks on the underlying panes. Background dims by
overlay alpha.

## 2. Resume / Detached picker

The Start button's tooltip drops down a small chooser showing both options:

```
           ┌─────────────────────────────────┐
           │  Start normal   (in-process)    │
           │  Start + detach (survives exit) │
           └─────────────────────────────────┘
```

Each is its own button. Clicking either starts; the choice persists in
`.mp/autopilot-state.json` for the next session.

## 3. Right-pane expanded view (`f` or click the ⤢ button)

```
┌── Progress · focus: M236 ───────────────────────────────────────────────────┐
│  ● M236 ⟶ herdr agent prompt opencode  ▰▰▰▱ 4/7    cycle 11 of 17            │
│     intent:   close recurring M202 doc-overindented-list-items lint          │
│     files:    crates/mp-model/src/milestone.rs:155-158                        │
│     reviews:  0 open · 0 resolved                                              │
│     next:     F-01 cycle verification at T=00:13                              │
│                                                                                 │
│     [ view reviews ]  [ view logs ]  [ jump to lane ]                        │
│                                                                                 │
│  ● M238 (collapsed)                                                            │
│  ○ M234 (collapsed)                                                            │
└─────────────────────────────────────────────────────────────────────────────────┘
```

Single-row focus mode; press `Esc` or `q` to dismiss.
