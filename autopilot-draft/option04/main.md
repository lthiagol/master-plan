# Option 04 — Hub-and-spoke (5 quick-launch cards)

Five large clickable cards on the dashboard. Each card opens a config modal.

## Layout

```
┌──────────────────────────────────── Autopilot · pick a launch ──────────────┐
│                                                                             │
│   ┌────────────────────┐   ┌────────────────────┐   ┌────────────────────┐  │
│   │     ▶ 1-agent      │   │     ▶ 2-agent      │   │     ▶ 3-agent      │  │
│   │   ┌──────────┐     │   │   ┌──────────┐     │   │   ┌──────────┐     │  │
│   │   │  planner │     │   │   │ planner  │     │   │   │ planner  │     │  │
│   │   │  runner  │     │   │   │ runner   │     │   │   │ runner   │     │  │
│   │   │          │     │   │   │ verifier │     │   │   │ verifier │     │  │
│   │   └──────────┘     │   │   └──────────┘     │   │   └──────────┘     │  │
│   │                    │   │                    │   │                    │  │
│   │  lightweight       │   │  balanced          │   │  full (default)    │  │
│   │  ~3 min · $0.05    │   │  ~8 min · $0.20    │   │  ~12 min · $0.40   │  │
│   └────────────────────┘   └────────────────────┘   └────────────────────┘  │
│                                                                             │
│   ┌────────────────────┐   ┌────────────────────┐                            │
│   │     ↻ Resume       │   │     ↯ Detached     │                            │
│   │   continue from    │   │   start, raul can  │                            │
│   │   last failure     │   │   exit safely      │                            │
│   │                    │   │                    │                            │
│   │  2 milestones left │   │  (via forked child)│                            │
│   │  ~9 min · $0.30    │   │  ~12 min · $0.40   │                            │
│   └────────────────────┘   └────────────────────┘                            │
│                                                                             │
│   gate ✓ herdr 0.7.4   •   session: master-plan   •   ⏎ activate · ? help  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## What this isn't

This is a launcher. After you pick a card, you don't come back here — the
**per-card config modal** opens (the canonical option's Setup row, but in a
modal). The hub is purely the entry point.

## Layout — per-card modal (e.g. "▶ 3-agent" card clicked)

```
┌── 3-agent run · configure ───────────────────────── esc ──┐
│  Topology     3-agent (locked)                            │
│  Harness      ✓ uniform                                   │
│     ( opencode ) ( cursor ) ( pi )                        │
│  Milestones   M236 · M238 · M234  [ + ] [ ↻ ] [ ✕ ]      │
│  Commit policy ( per-finding ) [edit]                     │
│  Run mode     ( normal run ) ( detached )                 │
│                                                            │
│  est. time    ~12 min                                      │
│  est. cycles  17                                           │
│                                                            │
│             ┌──────────────────┐    ┌──────────┐          │
│             │   ▶ Start run    │    │ Cancel   │          │
│             └──────────────────┘    └──────────┘          │
└────────────────────────────────────────────────────────────┘
```

After clicking **▶ Start run**, the modal closes and the screen flips to a
full-screen **progress** view that mirrors what option 01's progress region
shows — but full-tab.

## Why this option

- Marketing / first-impression: a 5-card grid reads instantly.
- Each card is a click target — no need to learn which chip is which.
- The grid matches a 108-col × 32-row layout cleanly; no scrollbars.

## Tradeoffs

- Five cards × N choices per modal = a lot of navigation state to track.
- The "Resume" card needs to detect that nothing's paused; otherwise it shows
  a disabled state.
- Pre-fills: clicking "1-agent" or "2-agent" sets the topology but the harness
  and milestone picker still need to be filled in the modal.

## Engineering cost (rough)

- New `launch_hub` widget ~150 LOC.
- Existing picker / override / advanced wiring reused from option 01.
- 5 card components + 5 modal "configure" variants.
- 5 entry points in dashboard nav.
