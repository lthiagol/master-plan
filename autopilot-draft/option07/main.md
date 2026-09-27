# Option 07 — Topology-as-graph (circles + arrows)

Roles are first-class visual objects (circles connected by arrows). Setup
happens by clicking the circles.

## Layout

```
┌──────────────────────────────────────── Autopilot · topology canvas ──────┐
│  Topology       [ 1-agent ]  [ 2-agent ]  [ *3-agent ]                    │
│                                                                            │
│                                                                            │
│                    ╭─────────────────────╮                                 │
│                    │     orchestrator    │                                 │
│                    │                     │                                 │
│                    │     (opencode)      │      ← click to change          │
│                    ╰────────┬────────────╯                                 │
│                             │                                              │
│              ┌──────────────┼──────────────┐                               │
│              │                              │                               │
│              ▼                              ▼                               │
│   ╭─────────────────────╮       ╭─────────────────────╮                   │
│   │      runner         │       │      verifier       │                   │
│   │     (opencode)      │       │     (opencode)      │                   │
│   ╰─────────────────────╯       ╰─────────────────────╯                   │
│              │                              │                               │
│              └──────────────┬───────────────┘                               │
│                             │                                              │
│                             ▼                                              │
│   ┌────────────────────────┴────────────────────────┐                  │
│   │  commit policy  ( per-step )  ( per-finding )  │                  │
│   │                  ( per-cycle ) ( batched ) *    │                  │
│   └─────────────────────────────────────────────────────┘                │
│                                                                            │
│  Milestones   M236 · M238 · M234                  [ + ] [ ↻ ]              │
│                                                                            │
│              ┌──────────────────────────────────────────────────┐         │
│              │  ▶ Start  3 milestones · 3-agent · ~12 min         │         │
│              └──────────────────────────────────────────────────┘         │
│  activity tail                                                              │
│  11:14:25 orchestrator → runner: M238 step S1                              │
│  11:14:26 runner     → orchestrator: S1 done                                │
└─────────────────────────────────────────────────────────────────────────
```

## Per-element interactions

- **Click a role circle** → opens the harness picker for that role.
- **Click an arrow** → opens the commit policy menu (per-edge).
- **Click the topology buttons** at the top → redraws the canvas (3-agent /
  2-agent / 1-agent).
- **Click the milestone line** → opens the picker modal.

```
  Per-role harness picker (after click)
  ┌── Orchestrator harness ──────────────────────────┐
  │   ( opencode ) ← currently selected               │
  │   ( cursor )                                     │
  │   ( pi )                                         │
  │   ✓ use the same harness for every role (uniform)│
  │                                                  │
  │                              ┌────────────┐      │
  │                              │   Apply    │      │
  │                              └────────────┘      │
  └──────────────────────────────────────────────────┘
```

## Why this option

- The role diagram mirrors the actual code (orchestrator / runner / verifier);
  a reader of the diagram learns the topology by clicking.
- Each circle becomes a focal point; new users naturally click the big shapes
  before discovering the chips.
- The diagram makes a 2-agent mode feel *meaningfully different* from 3-agent
  (the 3rd circle disappears).

## Tradeoffs

- Drawing circles and arrows in a terminal requires unicode characters and a
  careful layout engine; not every terminal renders it cleanly.
- ASCII fallbacks are required for non-UTF8 environments (the F-? historical
  TUI assumption).
- Diagram mode is visually distinct from the rest of the app — could feel
  out-of-place.

## Engineering cost (rough)

- 1 graph-layout widget ~250 LOC (drawing boxes + arrows in unicode).
- Topology variants (1-agent, 2-agent, 3-agent) wired into 3 different canvas
  states.
- Existing picker / override reused.
- ASCII fallback ~50 LOC.
