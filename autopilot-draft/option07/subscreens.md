# Option 07 — sub-screens

## 1. Per-role harness picker (shown above)

## 2. Per-arrow commit policy menu

```
┌── Commit policy · orchestrator → runner ─────────────────────┐
│                                                               │
│  when an AC passes in this edge,                            │
│  what commit strategy applies?                                │
│                                                               │
│  ( per-step   )  ← default                                   │
│  ( per-finding)                                              │
│  ( per-cycle  )                                              │
│  ( batched    )                                              │
│                                                               │
│           ┌────────────┐  ┌─────────────┐                  │
│           │   Apply    │  │   Cancel    │                  │
│           └────────────┘  └─────────────┘                  │
└───────────────────────────────────────────────────────────────┘
```

## 3. Topology variants

**1-agent:**

```
┌─────────────────────────────────────────────────────────────────┐
│  [ 1-agent ] * [ 2-agent ]  [ 3-agent ]                          │
│                                                                  │
│                       ╭─────────────────╮                       │
│                       │   orchestrator  │   ← also runner       │
│                       │                 │   ← also verifier      │
│                       │    (opencode)   │                       │
│                       ╰─────────────────╯                       │
│                                                                  │
│  commit policy   ( per-finding ) [edit]                         │
└─────────────────────────────────────────────────────────────────┘
```

**2-agent:**

```
┌─────────────────────────────────────────────────────────────────┐
│  [ 1-agent ]  [ 2-agent ] * [ 3-agent ]                        │
│                                                                  │
│        ╭─────────────────╮                                      │
│        │   orchestrator  │                                      │
│        ╰────────┬────────╯                                      │
│                 │                                               │
│                 ▼                                               │
│        ╭─────────────────╮                                      │
│        │     runner      │   (verifier merged into orchestrator)│
│        ╰─────────────────╯                                      │
│                                                                  │
│  commit policy   ( per-finding ) [edit]                         │
└─────────────────────────────────────────────────────────────────┘
```

## 4. Run-intent confirmation (pre-start)

```
┌── Confirm run ──────────────────────────────────────────────────┐
│                                                                 │
│  Topology      3-agent (orchestrator + runner + verifier)         │
│  Harness       uniform · opencode                                │
│  Commit policy per-finding                                       │
│  Milestones    M236 · M238 · M234                                │
│  est. time     ~12 min                                            │
│  est. cycles   17                                                 │
│                                                                 │
│         ┌──────────────────┐  ┌─────────────┐                  │
│         │  ▶ Start run     │  │   Cancel     │                  │
│         └──────────────────┘  └─────────────┘                  │
└─────────────────────────────────────────────────────────────────┘
```

## 5. Post-start live canvas

After clicking start, the canvas keeps the topology diagram (now with
status indicators on each edge):

```
                       ╭─────────────────╮
                       │   orchestrator  │   ● busy
                       ╰────────┬────────╯
                                │
                                ▼
                       ╭─────────────────╮
                       │     runner      │   ● busy
                       ╰─────────────────╯
                                │
                                ▼
                       ╭─────────────────╮
                       │    verifier     │   ⏳ waiting
                       ╰─────────────────╯

  ● M236  cycle 11/17  ▰▰▰▱ 4/7
```

Status indicators become clickable to drill into per-role state.
