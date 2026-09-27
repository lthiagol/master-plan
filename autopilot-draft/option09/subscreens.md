# Option 09 — sub-screens

## 1. Per-role harness picker (sidebar appears on click)

```
┌── Harness per role ────────────────────── ✕ ─┐
│                                              │
│   ✓ uniform: opencode                       │
│                                              │
│   Orchestrator:   ( opencode ) (cursor) (pi) │
│   Runner:         ( opencode ) (cursor) (pi) │
│   Verifier:       ( opencode ) (cursor) (pi) │
│                                              │
│   ┌──────────┐  ┌────────────┐               │
│   │  Apply   │  │   Cancel    │               │
│   └──────────┘  └────────────┘               │
└──────────────────────────────────────────────┘
```

## 2. State inspector tab

```
┌── state inspector ──────── ✕ ─┐
│                              │
│  session.json                │
│    schema_version: 2         │
│    pane_count: 3             │
│    active_run: 235           │
│    topology: 3-agent         │
│                              │
│  override panel             │
│    orchestrator.role: planner│
│    runner.role: implementer  │
│    verifier.role: reviewer   │
│                              │
│  autopilot-state.json       │
│    last_run: 235             │
│    last_topology: 3-agent    │
│    drawer_open: false        │
│                              │
│  [ refresh ]  [ copy ]      │
└──────────────────────────────┘
```

## 3. Activity tab — extended tail

```
┌── activity · 12-line tail ────────────────────── ✕ ─┐
│                                                       │
│  11:14:23 orchestrator → verifier: ok                  │
│  11:14:24 verifier   → orchestrator: M236/AC-03 ok    │
│  11:14:25 orchestrator → runner: M238 step S1         │
│  11:14:26 runner     → orchestrator: S1 done          │
│  11:14:27 orchestrator → verifier: ok                  │
│  11:14:28 verifier   → orchestrator: M236/AC-04 ok    │
│  11:14:29 orchestrator → runner: M238 step S2         │
│  11:14:30 runner     → orchestrator: S2 done          │
│  11:14:31 orchestrator → verifier: ok                  │
│  11:14:32 verifier   → orchestrator: M236/AC-05 ok    │
│  11:14:33 orchestrator → runner: M238 step S3         │
│  11:14:34 runner     → orchestrator: S3 done          │
│                                                       │
│  [ reload ]  [ filter ]  [ copy ]                     │
└───────────────────────────────────────────────────────┘
```

## 4. Sidebar resize drag

The sidebar's right edge is a drag-handle. Drag to resize from 25% to 50%
(and the sidebar collapses to 0% if dragged far enough).

## 5. Tab-strip persistence

`.mp/autopilot-state.json` records which sidebar tab is active. Refresh
restores the same tab.
