# Option 09 — Right sidebar (collapsible tool-window)

Setup + controls full-width on the left. Right sidebar (25% width, collapsible)
holds progress / activity / state inspector.

## Layout — sidebar visible (default)

```
┌───────────────────────────────────────────────┬──── progress ────────────┐
│  Autopilot · setup                              │  ● M236  ▰▰▰▱ 4/7      │
│                                                  │  ● M238  ▰▱▱▱ 1/3      │
│  Topology    [ 1-agent ]  [ 2-agent ]  [*3-agent] │  ○ M234  queued       │
│                                                  │                        │
│  Harness     uniform                            │  activity              │
│              Orch / Run / Verif:  opencode       │  11:14:23 o → v: ok    │
│              [ edit per role ]                   │  11:14:24 v → o: M236 │
│              [ ⋯ advanced override ]             │  11:14:25 o → r: M238 │
│                                                  │  11:14:26 r → o: S1   │
│  Milestones  M236 · M238 · M234   [ + ] [ ↻ ]   │                        │
│                                                  │  lanes 2/3 · cyc 2    │
│  Commit      ( per-finding ) [edit]             │  ▶ toggle (Sidebar)   │
│                                                  │                        │
│  Run mode    ( normal )  ( detached )            │  · queues / telemetry │
│                                                  │  · activity tail      │
│              ┌─────────────────────────┐         │  · state inspector    │
│              │  ▶ Start · 3 ms · ~12m  │         │                        │
│              └─────────────────────────┘         │                        │
└───────────────────────────────────────────────┴────────────────────────────┘
```

## Layout — sidebar collapsed

```
┌────────────────────────────────────────────────────────── Autopilot ───────────────┐
│  Topology    [ 1-agent ]  [ 2-agent ]  [*3-agent]                                    │
│  Harness     uniform · opencode   [ edit ] [ ⋯ advanced ]                          │
│  Milestones  M236 · M238 · M234   [ + ] [ ↻ ]                                      │
│  Commit      ( per-finding ) [edit]                                                 │
│  Run mode    ( normal )  ( detached )                                              │
│                  ┌────────────────────────────┐                                  │
│                  │ ▶ Start · 3 ms · ~12m       │                                  │
│                  └────────────────────────────┘                                  │
│                                                                                       │
│                          ▸ open progress sidebar                                     │
└──────────────────────────────────────────────────────────────────────────────────┘
```

## Sidebar modes (tabs at the right edge)

- **progress** — current lane rows.
- **activity** — 12-line scrolling feed of orchestrator ↔ verifier events.
- **telemetry** — counters (cycles, queue depth, last-update ms).
- **state inspector** — a debug view: session.json fields, override panel
  contents, .mp/autopilot-state.json.

Clicking the right-edge tab strip switches the sidebar's content; the
sidebar itself stays visible.

## During a run

The sidebar's "progress" tab becomes live: dots, queue rows, line bars update
in real time. The user is reading the sidebar while configuring — they can
*see* their changes take effect.

## Why this option

- IDE-pattern: a right tool-window pinned at all times.
- The sidebar is the answer to "I want telemetry but I don't want it
  hiding the setup form."
- Tabbed sidebar (progress / activity / telemetry / inspector) is cheap to
  add; each tab is a small widget.

## Tradeoffs

- 25% sidebar is too small for the live status graph; you see queue rows
  but not the rich chart from M216.
- Mouse hit-testing on the sidebar tabs is an extra surface.
- The state-inspector tab is developer-y; it might be unwanted by users who
  aren't debugging.

## Engineering cost (rough)

- 1 right-sidebar widget ~150 LOC.
- 4 sidebar tabs ~80 LOC each.
- Sidebar visibility state in view_state.
- Existing telemetry / progress rows extracted into reusable widgets.
