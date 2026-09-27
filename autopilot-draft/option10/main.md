# Option 10 — Preset card-stack

Click a preset card → load its config into the editable grid below → edit if
you want → start.

## Layout

```
┌───────────────────────────────────── Autopilot · pick a preset ────────────────┐
│                                                                                  │
│   ┌────────────────┐   ┌────────────────┐   ┌────────────────┐                │
│   │  Default       │   │  Lightweight   │   │  Heavy         │                │
│   │                │   │                │   │                │                │
│   │  3-agent       │   │  1-agent       │   │  3-agent       │                │
│   │  opencode      │   │  opencode      │   │  opencode      │                │
│   │  per-finding   │   │  per-step      │   │  batched       │                │
│   │  3 milestones  │   │  1 milestone   │   │  all ready     │                │
│   │                │   │                │   │                │                │
│   │  ~12m · $0.40  │   │  ~3m · $0.05   │   │  ~40m · $1.20  │                │
│   └────────────────┘   └────────────────┘   └────────────────┘                │
│                                                                                  │
│   ┌────────────────┐   ┌────────────────┐   ┌────────────────┐                │
│   │  Resume        │   │  Debug         │   │  Custom       │                 │
│   │                │   │                │   │                │                │
│   │  last known    │   │  2-agent,      │   │  start from   │                │
│   │  configuration │   │  per-cycle,    │   │  scratch       │                │
│   │                │   │  verbose log   │   │                │                │
│   │                │   │                │   │  (uses grid    │                │
│   │  2 milestones  │   │  just M236     │   │   below)       │                │
│   │  ~9m · $0.30   │   │  ~5m · $0.20   │   │                │                │
│   └────────────────┘   └────────────────┘   └────────────────┘                │
│                                                                                  │
│ ─── Configuration (loaded from selected card; editable) ──────────────────────  │
│   Topology     [1-agent ]  [2-agent ]  [*3-agent ]                                │
│   Harness      uniform · opencode / cursor / pi                                  │
│   Milestones   M236 · M238 · M234    [ + ] [ ↻ ] [ ✕ ]                            │
│   Commit       ( per-finding ) [edit]                                             │
│   Run mode     ( normal run )  ( detached )                                       │
│                                                                                   │
│              ┌─────────────────────────────────────────────────────┐           │
│              │  ▶ Start · 3 milestones · 3-agent · ~12 min           │           │
│              └─────────────────────────────────────────────────────┘           │
└──────────────────────────────────────────────────────────────────────────────┘
```

## How loading a preset works

Click "Default":
- Topology → 3-agent
- Harness → uniform · opencode
- Milestones → top 3 ready (M236, M238, M234)
- Commit policy → per-finding
- Run mode → normal run

The configuration grid below populates *visibly* — the user sees each field
change with a brief highlight to indicate "loaded from preset."

## Mouse model

Model B (free click + focus) is mandatory. Cards are click targets; the
configuration grid is its own widget.

## Why this option

- Most users want a named preset; let them pick one and go.
- The grid below is editable, so the "Custom" preset is a logical dovetail
  with the same primitive.
- The card stack reads as an opinionated product: "we picked these defaults;
  here's what they mean."

## Tradeoffs

- Naming the presets correctly is editorial work ("Default", "Heavy",
  "Debug" — these are subjective).
- Adding/removing presets after launch is a UX change (need a migration
  for users who picked "Default" before it was renamed to something else).
- Users who *want* to fine-tune might find the cards redundant with the
  grid below.
