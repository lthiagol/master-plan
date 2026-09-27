# Sub-screens

Every modal / sub-screen attached to the main screen.

## 1. While Executing (full-screen takeover during run)

This is the dashboard shown the moment a run starts and until it
completes. The idle split view disappears; the State tab disappears
with it (Q9). The pause/stop/resume control row stays visible.

### Layout

```
┌─ Autopilot · LIVE · 3-agent · uniform · opencode · per-finding ─────────────┐
│                                                                              │
│  Topo strip (compact, run config recap)                                    │
│    3-agent · uniform · opencode · per-finding commits · detached           │
│                                                                              │
│  ────── milestone lanes ──────                                              │
│                                                                              │
│  ●  M236  ⟶  herdr agent prompt opencode                                    │
│     intent:    close the recurring false-positive at                         │
│                crates/mp-model/src/milestone.rs:155-158                      │
│     reviews:   0 open · 0 resolved                                           │
│     next:      F-01 cycle verification at T=00:13                            │
│     cycle:     14 of 17   ▰▰▰▰▰▰▱▱  ·  est. 7m 22s left                       │
│                                                                              │
│  ●  M238  ⟶  waiting on orchestrator                                        │
│     intent:    M211 file slug + parse_rfc3339 dedup                          │
│     reviews:   0 open · 0 resolved                                           │
│     cycle:      1 of  3   ▰▱▱▱▱▱▱▱  ·  est. 6m 51s left                       │
│                                                                              │
│  ○  M234  ⟶  queued · starts when M238 cycles finish                        │
│                                                                              │
│  ────── activity (auto-sized, recent first) ──────                         │
│  11:14:25  orchestrator → runner:        M238 step S1                       │
│  11:14:26  runner     → orchestrator:    S1 done                            │
│  11:14:27  orchestrator → verifier:      ok                                 │
│  11:14:28  verifier   → orchestrator:    M236/AC-04 ok                     │
│  11:14:29  orchestrator → runner:        M238 step S2                       │
│  11:14:30  runner     → orchestrator:    S2 done                            │
│  11:14:31  orchestrator → verifier:      M236/AC-05 in_flight               │
│                                                                              │
│  ────── telemetry ──────                                                   │
│  lanes 2/3 · cycles 14 · queue 1 · qps 0.4 · cost $0.42                    │
│  last update 50ms ago                                                       │
│                                                                              │
│  ────── controls ──────                                                     │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌───────────────────────┐  │
│  │ ⏸ Pause     │ │ ⏹ Stop      │ │ ↻ Resync    │ │ ⤴ Edit & re-run      │  │
│  └─────────────┘ └─────────────┘ └─────────────┘ └───────────────────────┘  │
│                                                                              │
│  ⏎ peek · Esc → back to setup (with confirm) · ? help                      │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Sections, in order

1. **Topo strip** — single line summarizing the run config: `topology ·
   uniform/per-role · harness · commit-policy · run-mode`. Helps the user
   confirm what they started.
2. **Milestone lanes** — one row per active milestone with:
   - Status dot (● green = running; ● yellow = waiting; ⏳ = queued).
   - ID + `⟶ <human-readable status phrase>`.
   - Optional 1-line intent blurb (truncated).
   - Cycle counter and progress bar.
   - `est. N m SS s left` per cycle.
   - Click on the row → opens the Per-milestone peek view (see §4).
3. **Activity** — auto-sized (Q6) — recent first.
4. **Telemetry** — three stats lines: status counters; cost estimate;
   telemetry freshness.
5. **Controls** — Pause / Stop / Resync / Edit & re-run.

### Controls (always visible)

- **Pause** — `p`. Pauses the run; the lane buttons show "▶ Resume".
  Mid-run edits need the user to Pause first.
- **Stop** — `x` (with confirm). Stop the run; detaches if detached was on,
  aborts clean state otherwise.
- **Resync** — `r` (when paused). Reload session.json + override panel from
  disk; useful if the user edited the plan via CLI.
- **Edit & re-run** — `Esc`. Pops a confirm dialog (Q2-style), then
  returns to the idle split view with the run config pre-filled.

### State tab ↔ peek view

The State tab is gone during a run; its job is fulfilled by:
- **Per-milestone peek view** (Q9 alternative): click any milestone row.
- **Topo strip recap** for session-level config.

This avoids showing a static dump that doesn't reflect live changes.

### Return-to-setup path

Two options:
- `Esc` → `Edit & re-run` modal (auto-pauses the run first).
- Run completes → auto-return to split view with pre-filled config.

---

## 2. Floating milestone picker (compact anchor-center overlay)

See `floating-picker.md` for the full layout and behavior. The picker
covers the middle 2/3 of the screen, dimming the surroundings to 30%.

## 3. Advanced override (reveal)

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

`Topo id` is **auto-derived** from topology + counter (Q8). The field
is read-only; users see what's chosen but don't type it.

## 4. Per-role harness picker (drilldown)

Single-select per role, opened when the user toggles off `✓ uniform`:

```
┌── Orchestrator harness ──────────────────────────┐
│                                                  │
│   ● opencode                                     │
│   ○ cursor                                       │
│   ○ pi                                           │
│                                                  │
│   ┌──────────┐  ┌────────────┐                  │
│   │  Apply   │  │   Cancel    │                  │
│   └──────────┘  └────────────┘                  │
└─────────────────────────────────────────────────┘
```

## 5. Detached / Start confirm (Q2)

Because we picked "confirm every click", the Detached chip shows a confirm
modal every time:

```
┌── Detached mode ─────────────────────────────────────────────┐
│                                                              │
│  ⚠  Detached runs keep the autopilot child alive after this   │
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

## 6. Per-milestone peek view

Clicking a milestone lane (in the picker or in the While Executing
view) opens this:

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

## 7. Sidebar tab: Activity (full log)

```
┌─ activity · full log ────────────────────────────────────────┐
│  filter: [ all  ● orch  ○ ver  ○ runner  ○ lifecycle ]       │
│                                                                │
│  11:14:23  orchestrator → verifier:    ok                     │
│  11:14:24  verifier   → orchestrator:  M236/AC-03 ok         │
│  11:14:25  orchestrator → runner:      M238 step S1         │
│  11:14:26  runner     → orchestrator:  S2 done              │
│  ... 22 more lines ...                                       │
│                                                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                    │
│  │  Reload  │  │  Filter  │  │   Copy   │                    │
│  └──────────┘  └──────────┘  └──────────┘                    │
└──────────────────────────────────────────────────────────────┘
```

## 8. Sidebar tab: State (read-only dump, idle-only)

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
└──────────────────────────────────────────────────────────────┘
```

Note: this tab is **hidden while a run is live** (Q9); the per-milestone
peek view (§6) replaces it for inspecting a single milestone mid-run.

## 9. Post-run summary

After the run completes (or aborts), the screen flips back to the idle
split view. The Start button becomes:

```
┌─────────────────────────────────────────┐
│  ✓  Run completed                        │
│  3 milestones · 11m 24s                  │
│  ↻ re-run   ✏ edit & re-run    close    │
└─────────────────────────────────────────┘
```

`edit & re-run` returns to setup with the previous config pre-filled.

## 10. Resume / restart picker

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

## 11. Start button pulse (Q10)

After clicking **Confirm** in the floating picker, the Start button in
the idle split view does a brief 100ms cyan-border pulse — a tactile
click registration. Then the screen flips to the While Executing view.

Mock local behavior:

```
T+0ms     : confirm clicked
T+10ms    : start button border thickens, label pulses
T+100ms   : pulse fades
T+~300ms  : first orchestrator event arrives, While Executing view is fully rendered
```
