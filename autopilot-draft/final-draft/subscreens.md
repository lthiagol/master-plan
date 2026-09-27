# Other sub-screens

Every modal / drill-down attached to the main screen besides the floating
picker. (The floating picker is its own document.)

## 1. Advanced override (reveal)

The `⋯ advanced override` link expands inline below the harness grid:

```
│  Harness per role     ✓ uniform
│     Orchestrator:  ( opencode )(cursor)(pi)
│     Runner:        ( opencode )(cursor)(pi)
│     Verifier:      ( opencode )(cursor)(pi)
│     ▾ hide advanced ▴
│     ┌── advanced override ────────────────────────────┐
│     │  Topology:        3-agent                       │
│     │  Orchestrator role: planner                     │
│     │  Runner role:       implementer                │
│     │  Verifier role:     reviewer                   │
│     │  Topo id:           three-agent-001            │
│     │  Extras:           { "shell": "bash" }         │
│     │                                                  │
│     │         [ reset to defaults ]                   │
│     └─────────────────────────────────────────────────┘
```

The advanced panel is the existing `OverridePanel` collapsed. "Reset"
copies the canonical pre-set values back into the harness chips.

## 2. Per-role harness picker (drilldown)

If the user toggles off `✓ uniform`, each role chip becomes clickable:

```
┌── Orchestrator harness ──────────────────────────┐
│                                                  │
│   ● opencode                                     │
│   ○ cursor                                       │
│   ○ pi                                           │
│                                                  │
│   ┌──────────┐  ┌────────────┐                  │
│   │  Apply   │  │   Cancel   │                  │
│   └──────────┘  └────────────┘                  │
└─────────────────────────────────────────────────┘
```

Single-select per role. Click Apply → harness chip updates → back to main.

## 3. Detached / Start confirm

The Start button shows a tooltip on hover/focus:

```
           ┌─────────────────────────────────────────┐
           │  normal run       (raul must stay open)│
           │  detached run  ●  (child survives)      │
           └─────────────────────────────────────────┘
```

Choosing detached shows a confirmation popover:

```
┌── Detached mode ─────────────────────────────────────────────┐
│                                                              │
│  ⚠  Detached runs keep the autopilot child alive after this    │
│  raul window closes. To stop it later, re-attach via          │
│  `mp autopilot attach <session-id>`.                         │
│                                                              │
│   ┌──────────┐  ┌───────────────┐  ┌──────────┐              │
│   │  Confirm │  │  Configure    │  │   Back   │              │
│   │  detach  │  │  extras...    │  │          │              │
│   └──────────┘  └───────────────┘  └──────────┘              │
└──────────────────────────────────────────────────────────────┘
```

"Configure extras" reveals the detached-mode knobs (herdr spawn shell,
session-id prefix, environment file).

## 4. Per-milestone peek view (clickable row in any list)

Pressing `⏎` on a row in the picker OR on the sidebar's activity tail OR
on the setup's milestone chip opens this:

```
┌── M236 ─────────────────────────────────────────────────────┐
│                                                            │
│  title       M202 doc-overindented-list-items clippy lint   │
│  priority    high                                           │
│  lifecycle   approved                                       │
│  spec_status ready                                           │
│  intent      close the recurring false-positive at          │
│              crates/mp-model/src/milestone.rs:155-158        │
│  ─────────                                                ── │
│  AC-01  clippy passes  ·   exit 0    · covered by step S1 │
│  AC-02  make test      ·   exit 0    · covered by step S2 │
│  AC-03  validate       ·   exit 0    · covered by step S3 │
│  AC-04  lint + fmt     ·   exit 0    · covered by step S4 │
│  ────────────────────────────────────────────              │
│  [ view reviews ]  [ open commit log ]  [ back ]           │
└────────────────────────────────────────────────────────────┘
```

Read-only by default. The "view reviews" button loads the milestone's
reviews.json in a sub-panel.

## 5. Sidebar tab: Activity (full log)

```
┌─ activity · full log ────────────────────────────────────────┐
│  filter: [ all  ● orch  ○ ver  ○ runner  ○ lifecycle ]       │
│                                                                │
│  11:14:23  orchestrator → verifier:    ok                     │
│  11:14:24  verifier   → orchestrator:  M236/AC-03 ok         │
│  11:14:25  orchestrator → runner:      M238 step S1         │
│  11:14:26  runner     → orchestrator:  S1 done              │
│  11:14:27  orchestrator → verifier:    ok                     │
│  11:14:28  verifier   → orchestrator:  M236/AC-04 ok         │
│  11:14:29  orchestrator → runner:      M238 step S2         │
│  11:14:30  runner     → orchestrator:  S2 done              │
│  ... 22 more lines ...                                       │
│                                                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                    │
│  │  Reload  │  │  Filter  │  │   Copy   │                    │
│  └──────────┘  └──────────┘  └──────────┘                    │
└──────────────────────────────────────────────────────────────┘
```

## 6. Sidebar tab: State (read-only dump)

```
┌─ state inspector ────────────────────────────────────────────┐
│                                                                │
│  session.json                                                  │
│    schema_version:   2                                         │
│    pane_count:       3                                         │
│    active_run:       236                                       │
│    topology:         3-agent                                   │
│    created_at:       2026-09-27T11:14:23Z                     │
│                                                                │
│  override-panel.json                                           │
│    orchestrator.role: planner                                  │
│    runner.role:       implementer                            │
│    verifier.role:     reviewer                                │
│    extras:            { "shell": "bash" }                     │
│                                                                │
│  .mp/autopilot-state.json                                      │
│    last_run:          236                                      │
│    last_topology:     3-agent                                 │
│    last_milestones:   [M236, M238, M234]                       │
│    picker_open:       false                                    │
│    drawer_open:       false                                    │
│    sidebar_tab:       progress                                 │
│                                                                │
│         ┌──────────┐  ┌──────────┐                            │
│         │  Refresh │  │   Copy   │                            │
│         └──────────┘  └──────────┘                            │
└────────────────────────────────────────────────────────────────┘
```

## 7. Post-run summary

After the run completes (or aborts), the Start button is replaced:

```
┌─────────────────────────────────────────┐
│  ✓  Run completed                        │
│  3 milestones · 11m 24s                  │
│  ↻ re-run   ✏ edit & re-run   close    │
└─────────────────────────────────────────┘
```

Click "edit & re-run" → returns to setup region with the previous
configuration pre-filled.

## 8. Resume / restart picker

When the user clicks the `↻ Resume` button mid-run (or on a completed run):

```
┌── Resume ────────────────────────────────────────────────────┐
│                                                              │
│  ⓘ Two options:                                               │
│                                                              │
│   Resume from current state                                   │
│      continues from the last successful step                  │
│                                                              │
│   Cold restart                                                │
│      re-runs the same milestones from scratch                 │
│                                                              │
│        ┌──────────┐  ┌──────────┐  ┌────────────┐            │
│        │  Resume  │  │  Restart │  │  Cancel    │            │
│        └──────────┘  └──────────┘  └────────────┘            │
└──────────────────────────────────────────────────────────────┘
```
