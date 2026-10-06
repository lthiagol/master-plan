# Roles, delegation, and topology

Who holds which role, what skill teaches it, and how work fans out from one
coordinator session to many runners and their subagents.

Related: [`README.md`](./README.md) for the console view,
[`flow-and-gates.md`](./flow-and-gates.md) for the stage timeline.

---

## Two agents, two roles, one shared map

The system is deliberately small: a **coordinator** that plans and reviews, a
**runner** that executes and fixes, and the `mp-flow` skill both load so they
share the same 12-stage map. The split exists for one reason — the agent that
executes a milestone must not be the agent that reviews it.

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 300, "nodeSpacing": 28, "rankSpacing": 42}} }%%
flowchart TB
  FLOW["<b>mp-flow</b><br/>the shared 12-stage map<br/>loaded first by any session"]

  subgraph COORD["coordinator role · plans and reviews"]
    C1["mp-coordinator<br/>spec + external review"]
    C2["spec-grill<br/>adversarial spec design"]
    C3["stages 1–4 · 8 · 10 · 11–12"]
    C1 --> C3
    C2 --> C3
  end

  subgraph RUNR["runner role · executes and fixes"]
    R1["mp-runner<br/>claim · implement · remediate"]
    R2["diagnosing-bugs<br/>codebase-design · explore"]
    R3["stages 5–7 · 9"]
    R1 --> R3
    R2 --> R3
  end

  FLOW --> C1
  FLOW --> R1
  R1 -.->|"a different session reviews<br/>what the runner shipped"| C1

  class C1,C2,C3 coord
  class R1,R2,R3 run
  class FLOW store

  style COORD fill:#241609,stroke:#e8834a,stroke-width:1px,color:#f7d9c2
  style RUNR fill:#0f2b31,stroke:#4fc3d9,stroke-width:1px,color:#d3f0f7

  classDef coord fill:#2b1a11,stroke:#e8834a,stroke-width:2px,color:#f7d9c2
  classDef run fill:#0f2b31,stroke:#4fc3d9,stroke-width:2px,color:#d3f0f7
  classDef rev fill:#241a33,stroke:#a98bd6,stroke-width:2px,color:#e6dbf7
  classDef gate fill:#122a1b,stroke:#6fbf7f,stroke-width:2px,color:#d6f0dc
  classDef human fill:#2b2410,stroke:#d8b34a,stroke-width:2px,color:#f4e7c2
  classDef done fill:#2b2410,stroke:#d8b34a,stroke-width:3px,color:#f4e7c2
  classDef store fill:#1c1c1c,stroke:#6a6a6a,stroke-width:2px,color:#d0d0d0
```

| Role | Owns | Never does |
|------|------|-----------|
| **Coordinator** | Spec authoring (stages 1–4), external review (8), re-review (10), documenting and hand-off (11–12) | Edit application code during review |
| **Runner** | Execution (5–7) and remediation (9) | Approve its own spec or sign off on its own work |

> Autopilot sessions use a finer split of the same idea: **orchestrator**
> (cycle decisions) + **runner** + **reviewer**. See
> [the topology below](#autopilot-topology).

---

## How work fans out

One coordinator owns the whole plan. It asks the path engine what is drivable,
hands that item to a runner session, and takes the verified result back. The
runner's own subagents each get one bounded task.

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 300, "nodeSpacing": 28, "rankSpacing": 42}} }%%
flowchart TB
  MAIN["<b>coordinator session</b><br/>owns the plan"]
  Q{"mp next · mp path<br/>what is drivable now?"}
  GR["<b>not ready</b><br/>groom · decompose<br/>add work packages + steps"]
  RS["<b>runner session</b> · one per milestone<br/>claim → step in-progress → step done → verify"]

  MAIN -->|"mp next"| Q
  Q -->|"deps met · spec approved"| RS
  Q -->|"gaps or unapproved spec"| GR
  GR -->|"re-groom, then ask again"| Q

  RS --> S1["explore<br/>map the code first"]
  RS --> S2["worker<br/>edit + run step tests"]
  RS --> S3["code reviewer<br/>read the diff, not the claim"]
  S3 --> RS

  RS -->|"mp step done · mp milestone complete"| MAIN
  RS -.->|"blocked? block + escalate"| MAIN

  class MAIN,Q coord
  class GR gate
  class RS,S1,S2,S3 run

  style Q fill:#241609,stroke:#e8834a,stroke-width:2px,color:#f7d9c2

  classDef coord fill:#2b1a11,stroke:#e8834a,stroke-width:2px,color:#f7d9c2
  classDef run fill:#0f2b31,stroke:#4fc3d9,stroke-width:2px,color:#d3f0f7
  classDef rev fill:#241a33,stroke:#a98bd6,stroke-width:2px,color:#e6dbf7
  classDef gate fill:#122a1b,stroke:#6fbf7f,stroke-width:2px,color:#d6f0dc
  classDef human fill:#2b2410,stroke:#d8b34a,stroke-width:2px,color:#f4e7c2
  classDef done fill:#2b2410,stroke:#d8b34a,stroke-width:3px,color:#f4e7c2
  classDef store fill:#1c1c1c,stroke:#6a6a6a,stroke-width:2px,color:#d0d0d0
```

Three escape hatches are load-bearing here:

- **Not drivable yet** → go back to grooming, not forward to coding. A milestone
  without approved specs has no executable path.
- **Blocked** → `mp milestone block --reason` plus escalation. `--force` is
  recorded debt, not a shortcut.
- **One milestone at a time per runner** → a runner session's blast radius is
  its own diff, which is what makes the independent review tractable.

---

## Autopilot topology

`mp autopilot` turns the manual dance into a supervised run: three panes, one
durable session file, and a queue that advances cycle by cycle. The state lives
in `<plan_dir>/autopilot/<id>/session.json`, so a run can be archived, diffed,
and recovered in isolation.

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 300, "nodeSpacing": 28, "rankSpacing": 42}} }%%
flowchart TB
  CLI["mp autopilot<br/>start · status · wait · tail"]

  subgraph TOPO["three-pane topology"]
    ORCH["<b>orchestrator</b><br/>decides the next cycle"]
    RUNR["<b>runner</b><br/>executes · leaves notes"]
    REVR["<b>reviewer</b><br/>verdict + findings"]
    ORCH -->|"queue item + cycle"| RUNR
    RUNR -->|"notes + evidence"| REVR
    REVR -->|"findings reopen"| ORCH
  end

  SESS["<b>autopilot/ID/session.json</b><br/>topology · roles · queue · events<br/>ac_projections · cycle history"]
  MS[("<b>plan directory</b><br/>lifecycle + reviews")]

  CLI --> TOPO
  ORCH -->|"every transition is an mp write"| SESS
  SESS --> MS
  CLI -.->|"wait --timeout · tail --follow"| SESS

  class ORCH coord
  class RUNR run
  class REVR rev
  class SESS,MS store
  class CLI human

  style TOPO fill:#141414,stroke:#5a5a5a,stroke-width:1px,color:#d0d0d0

  classDef coord fill:#2b1a11,stroke:#e8834a,stroke-width:2px,color:#f7d9c2
  classDef run fill:#0f2b31,stroke:#4fc3d9,stroke-width:2px,color:#d3f0f7
  classDef rev fill:#241a33,stroke:#a98bd6,stroke-width:2px,color:#e6dbf7
  classDef gate fill:#122a1b,stroke:#6fbf7f,stroke-width:2px,color:#d6f0dc
  classDef human fill:#2b2410,stroke:#d8b34a,stroke-width:2px,color:#f4e7c2
  classDef done fill:#2b2410,stroke:#d8b34a,stroke-width:3px,color:#f4e7c2
  classDef store fill:#1c1c1c,stroke:#6a6a6a,stroke-width:2px,color:#d0d0d0
```

| Piece | What it is |
|-------|------------|
| `topology` | The three panes: `orchestrator`, `runner`, `reviewer` |
| `roles` | Per-role config snapshot, so a re-spawned pane resumes with the same model, harness, and skill |
| `queue` | Ordered milestones with a `stage` and a `cycle` counter per item |
| `role_state` | Per-role `idle` / `starting` / `working` / `blocked` / `done` |
| `runner_notes` | Typed handoffs from runner to reviewer — the structured version of "here's what I did" |
| `events` | The journal `mp autopilot tail` streams, one compact object per line |
| `ac_projections` | Per-AC status the verifier reads without re-deriving it |

Two knobs decide how patiently a run waits on a slow agent:
`--prompt-settle-ms` (how long a harness must report idle *continuously* before
a prompt lands — a fresh pane's TUI is still booting) and
`agent.automation.stall_timeout_minutes` (how long a stalled runner is tolerated
— the timer only accrues while the runner is *not* working, so a long build is
never mistaken for a hang).

---

## Skill catalog

| Skill | Category | Teaches |
|-------|----------|---------|
| `mp-flow` | core | The 12-stage timeline and which role owns each stage |
| `mp-coordinator` | core | Spec authoring, external review, re-review, hand-off |
| `mp-runner` | core | Claiming steps, implementing, self-review, remediation |
| `spec-grill` | catalog | Multi-round adversarial questioning before a spec is written |
| `mp-orchestrator` | catalog | Autopilot cycle decisions and state writes |
| `mp-reviewer` | catalog | Autopilot independent verification and verdict writes |
| `codebase-design` | catalog | Deep-module vocabulary: small interface, clean seam |
| `diagnosing-bugs` | catalog | Build a tight feedback loop first, then bisect and instrument |

`core` skills deploy on a bare `mp install`. `catalog` skills are opt-in with
`mp install --skills <id>`.

See [`../skills/`](../skills/) for the full manifests and authoring rules.
