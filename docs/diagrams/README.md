# Diagrams

Visual maps of the system: who does what, in which order, and which gate stops
them. Everything here is [Mermaid](https://mermaid.js.org), so it renders in
Obsidian, GitHub, and any Mermaid-aware renderer — and it diffs cleanly because
it is text.

| File | Shows |
|------|-------|
| [`README.md`](./README.md) | The agent console — the whole picture in one graph, plus the color legend |
| [`agent-roles.md`](./agent-roles.md) | Roles, skills, delegation, and the three-pane autopilot topology |
| [`flow-and-gates.md`](./flow-and-gates.md) | The 12-stage timeline, the lifecycle state machine, and the gates on every edge |
| [`cli-and-plan.md`](./cli-and-plan.md) | `mp` vs `raul`, the inside of `mp`, intake lanes, and the plan directory |

Every diagram uses the same palette, so a color means the same thing everywhere:

| Color | Meaning |
|-------|---------|
| 🟠 orange | coordinator — plans the spec, runs the external review |
| 🔵 cyan | runner — executes steps, remediates findings |
| 🟣 violet | reviewer rail — reads everything, writes findings only |
| 🟢 green | gates and the path engine — the rules, not the work |
| 🟡 amber | human — `raul`, approvals, escalation |
| ⚪ gray | the store — the plan directory and its journal |

---

## The agent console

The whole system in one graph. Read it top-down: the coordinator decides, the
path engine picks the next drivable item, a runner session executes it,
evidence comes back, and an independent reviewer — never the same session —
signs off before anything reaches `complete`.

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 300, "nodeSpacing": 28, "rankSpacing": 42}} }%%
flowchart TB

  subgraph RAIL["▌ reviewer · on call"]
    HOOKS["<b>hooks</b><br/>before approve<br/>after executed<br/>before complete"]
    READS["reads the whole record<br/>plan · diff · journal"]
    ADVICE["<b>last advice</b><br/>plan skips auth<br/>split into three changes"]
    HOOKS --> READS
    READS --> ADVICE
  end

  MAIN["<b>coordinator</b> · main session<br/>specs · gates · review<br/>stages 1–4 · 8 · 10 · 11–12"]
  ENG["<b>plan path engine</b><br/>which item · which gate<br/>retry or escalate"]
  RUN["<b>runner session</b><br/>claims steps · runs tests<br/>stages 5–7 · 9"]
  BACK["<b>back to the coordinator</b><br/>review + verify"]
  PLAN["<b>plan directory</b><br/>mediated by mp"]

  subgraph SUBS["delegate to subagents"]
    S1["explore<br/>maps the code"]
    S2["worker<br/>edits + runs tests"]
    S3["code reviewer<br/>reads the diff"]
  end

  subgraph RULES["▌ the rules"]
    GATES["<b>gates on every write</b><br/>G1 approved before code<br/>G3 one acceptance criterion<br/>G4 out-of-scope minimum<br/>G8 dependencies executed<br/>G14 no pending approval"]
    VERIF["<b>evidence, not prose</b><br/>every step test runs green<br/>every AC command runs<br/>criterion pass --evidence<br/>no open self findings"]
  end

  ADVICE -.->|"findings"| MAIN
  MAIN -->|"mp next"| ENG
  ENG -->|"claim + execute"| RUN
  RUN --> S1
  RUN --> S2
  RUN --> S3
  S3 --> RUN
  RUN -->|"mp step done"| PLAN
  RUN --> VERIF
  VERIF --> BACK
  BACK --> MAIN
  MAIN <-->|"mp write · mp read"| PLAN
  GATES -.-> MAIN
  GATES -.-> RUN
  VERIF -.->|"red blocks completion"| RUN

  class HOOKS,READS,ADVICE rev
  class MAIN,BACK coord
  class ENG,GATES,VERIF gate
  class RUN,S1,S2,S3 run
  class PLAN store

  style RAIL fill:#1b1526,stroke:#a98bd6,stroke-width:1px,color:#e6dbf7
  style SUBS fill:#0d1f24,stroke:#4fc3d9,stroke-width:1px,color:#d3f0f7
  style RULES fill:#132018,stroke:#6fbf7f,stroke-width:1px,color:#d6f0dc

  classDef coord fill:#2b1a11,stroke:#e8834a,stroke-width:2px,color:#f7d9c2
  classDef run fill:#0f2b31,stroke:#4fc3d9,stroke-width:2px,color:#d3f0f7
  classDef rev fill:#241a33,stroke:#a98bd6,stroke-width:2px,color:#e6dbf7
  classDef gate fill:#122a1b,stroke:#6fbf7f,stroke-width:2px,color:#d6f0dc
  classDef human fill:#2b2410,stroke:#d8b34a,stroke-width:2px,color:#f4e7c2
  classDef done fill:#2b2410,stroke:#d8b34a,stroke-width:3px,color:#f4e7c2
  classDef store fill:#1c1c1c,stroke:#6a6a6a,stroke-width:2px,color:#d0d0d0
```

Four things worth noticing:

1. **The plan directory has no inbound edge from a human.** Nothing but `mp`
   writes it. `raul` is a renderer, not an editor.
2. **The reviewer never shares a session with the runner.** That separation is
   the entire trust model — it is why `complete` is reachable only through
   `mp reviews pass`, never through `complete` alone.
3. **The path engine is a gate, not a worker.** It answers *what is drivable
   now*; it never edits code.
4. **Evidence flows back, claims do not.** An AC is passed by running its
   verification command, not by asserting that it would pass.

The store itself, and the split between the two CLIs, is in
[`cli-and-plan.md`](./cli-and-plan.md).

## Related reading

- [`../agent-guide/`](../agent-guide/) — orientation plus per-workflow detail
- [`../milestones/`](../milestones/) — the lifecycle, the gates, the milestone anatomy
- [`../skills/`](../skills/) — what each shipped skill teaches an agent
- [`../mp/commands.md`](../mp/commands.md) — the full command surface
- [`../raul/`](../raul/) — the seven lanes and their key bindings
