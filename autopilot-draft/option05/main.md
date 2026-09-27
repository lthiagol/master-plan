# Option 05 — Wizard stepper (1 → 2 → 3 → 4 → 5)

A linear wizard. One step per screen. Press Next or Back to navigate.

## Layout — Step 1: Topology

```
┌────────────────── Autopilot · Step 1 of 5 ────────────────────────┐
│                                                                   │
│   ▶ Choose a topology                                            │
│                                                                   │
│       ┌──────────────┐   ┌──────────────┐   ┌──────────────┐     │
│       │   1-agent    │   │   2-agent    │   │   3-agent    │     │
│       │              │   │              │   │              │     │
│       │  planner +   │   │  planner +   │   │  planner +   │     │
│       │  runner      │   │  runner +    │   │  runner +    │     │
│       │              │   │  verifier    │   │  verifier    │     │
│       │              │   │              │   │              │     │
│       │  ~3 min      │   │  ~8 min      │   │  ~12 min     │     │
│       │  $0.05 est.  │   │  $0.20 est.  │   │  $0.40 est.  │     │
│       └──────────────┘   └──────────────┘   └──────────────┘     │
│                                                                   │
│       gate ✓ herdr 0.7.4                                         │
│                                                                   │
│                           ┌────────────┐                        │
│                           │   Next ➜   │                        │
│                           └────────────┘                        │
└───────────────────────────────────────────────────────────────────┘
```

## Layout — Step 2: Harness

```
┌────────────────── Autopilot · Step 2 of 5 ────────────────────────┐
│   ▶ Choose a harness per role                                     │
│                                                                   │
│       ✓ uniform: same harness for every role                     │
│                                                                   │
│       ( opencode )  ( cursor )  ( pi )   ← clickable             │
│                                                                   │
│       ── or ──                                                    │
│                                                                   │
│         ◯ per-role: orchestrator + runner + verifier can differ  │
│                                                                   │
│         Orchestrator:   ( opencode ) ( cursor ) ( pi )            │
│         Runner:         ( opencode ) ( cursor ) ( pi )            │
│         Verifier:       ( opencode ) ( cursor ) ( pi )            │
│                                                                   │
│       [ ⋯ advanced override ]                                     │
│                                                                   │
│        ┌────────────┐  ┌────────────────────┐                   │
│        │  ◂ Back    │  │   Next ➜           │                   │
│        └────────────┘  └────────────────────┘                   │
└───────────────────────────────────────────────────────────────────┘
```

## Layout — Step 3: Milestones

```
┌────────────────── Autopilot · Step 3 of 5 ────────────────────────┐
│   ▶ Pick milestones to run                                        │
│                                                                   │
│       / M236                                                      │
│       ▶ M236   selected                                          │
│         M238   selected                                          │
│         M234   selected                                          │
│         M231                                                     │
│         M235                                                     │
│         M232                                                     │
│         ...                                                      │
│                                                                   │
│       [ ↻ refresh ]   [ select all ready ]   [ ✕ clear ]         │
│                                                                   │
│        ┌────────────┐  ┌────────────────────┐                   │
│        │  ◂ Back    │  │   Next ➜           │                   │
│        └────────────┘  └────────────────────┘                   │
└───────────────────────────────────────────────────────────────────┘
```

## Layout — Step 4: Commit policy

```
┌────────────────── Autopilot · Step 4 of 5 ────────────────────────┐
│   ▶ Pick a commit policy                                          │
│                                                                   │
│       ( per-step )    ( per-finding ) *                          │
│       ( per-cycle )   ( batched )                                │
│                                                                   │
│       ℹ per-finding makes 1 commit per AC verification;          │
│         useful for review attribution.                            │
│                                                                   │
│       [ ? ]  [ glossary ]                                         │
│                                                                   │
│        ┌────────────┐  ┌────────────────────┐                   │
│        │  ◂ Back    │  │   Next ➜           │                   │
│        └────────────┘  └────────────────────┘                   │
└───────────────────────────────────────────────────────────────────┘
```

## Layout — Step 5: Review & run

```
┌────────────────── Autopilot · Step 5 of 5 ────────────────────────┐
│   ▶ Review & run                                                  │
│                                                                   │
│       Topology      3-agent                                       │
│       Harness       uniform · opencode                            │
│       Milestones    M236 · M238 · M234                            │
│       Commit policy per-finding                                   │
│       Run mode      ( normal )  ( detached )                     │
│                                                                   │
│       est. time     ~12 min                                       │
│       est. cycles   17                                            │
│                                                                   │
│              ┌──────────────────────┐                           │
│              │   ▶ Start run          │                         │
│              └──────────────────────┘                           │
│        ┌────────────┐                                            │
│        │  ◂ Back    │                                            │
│        └────────────┘                                            │
└───────────────────────────────────────────────────────────────────┘
```

## Why this option

- One choice at a time = low cognitive load.
- Each step is small, testable, can be shipped independently.
- Hard to make mistakes; you can't accidentally skip a field.

## Tradeoffs

- 5 transitions before run; users running autopilot daily will hate the clicks.
- Can't easily go back to step 3 to add a milestone mid-prep.
- Visual status progress is replaced by stepper progress.
