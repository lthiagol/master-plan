# The CLIs and the plan directory

`mp` is the agent's instrument, `raul` is the human's window, and the plan
directory is the only state either of them has. This file shows how they
connect, what happens inside `mp`, and how a change finds its lane.

Related: [`README.md`](./README.md) for the console view,
[`../mp/commands.md`](../mp/commands.md) for every command.

---

## Two CLIs, two audiences, one store

The routing rule is non-negotiable: agents speak to `mp` and get JSON; humans
launch `raul` and get a TUI. `raul` never writes plan files — every status
change it offers is a hint to run the equivalent `mp` command.

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 300, "nodeSpacing": 28, "rankSpacing": 42}} }%%
flowchart TB
  AG["<b>agent session</b><br/>mp-flow + a role skill"]
  HUM["<b>human</b><br/>status · triage · approval"]
  RAUL["<b>raul</b> · read-only TUI<br/>seven lanes"]
  MP["<b>mp</b> · agent CLI<br/>JSON by default<br/>exit 2 on a gate failure"]
  PLAN[("<b>plan directory</b><br/>master-plan/ or .mp/")]
  CFG["config.json<br/>profile + UI preferences"]

  AG -->|"mp status · mp milestone …"| MP
  HUM --> RAUL
  RAUL -->|"shells out: mp status · list · show"| MP
  MP <-->|"writes · reads"| PLAN
  MP -->|"reads"| CFG
  RAUL -.->|"settings lane · mp config set"| CFG
  RAUL -.->|"every status change is a hint to run mp"| MP

  class AG coord
  class HUM,RAUL human
  class MP,PLAN,CFG store

  classDef coord fill:#2b1a11,stroke:#e8834a,stroke-width:2px,color:#f7d9c2
  classDef run fill:#0f2b31,stroke:#4fc3d9,stroke-width:2px,color:#d3f0f7
  classDef rev fill:#241a33,stroke:#a98bd6,stroke-width:2px,color:#e6dbf7
  classDef gate fill:#122a1b,stroke:#6fbf7f,stroke-width:2px,color:#d6f0dc
  classDef human fill:#2b2410,stroke:#d8b34a,stroke-width:2px,color:#f4e7c2
  classDef done fill:#2b2410,stroke:#d8b34a,stroke-width:3px,color:#f4e7c2
  classDef store fill:#1c1c1c,stroke:#6a6a6a,stroke-width:2px,color:#d0d0d0
```

Two consequences fall out of the picture, and both are load-bearing:

- **`raul` is always consistent with `mp`.** There is no second data path — it
  renders the same JSON the agent reads, so a view can never drift from the plan.
- **`raul` needs `mp` on `PATH`.** Without it, it falls back to defaults and
  shows an empty plan rather than inventing data.

---

## Inside `mp`

Argument in, validated write out. The layering is a single downward flow, and
`store.rs` is the only seam that touches the filesystem.

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 300, "nodeSpacing": 28, "rankSpacing": 42}} }%%
flowchart TB
  ARGS["argv"]
  CLI["<b>cli/</b> · clap command groups<br/>milestone · reviews · execution · plan"]
  DISP["<b>app/dispatch.rs</b><br/>one router per command"]
  CMD["<b>commands/</b> · handlers<br/>emit · emit_gate_failure · read_evidence"]
  VAL["<b>validate/</b><br/>gates · warnings · report"]
  DOM["<b>domain</b><br/>milestone io/spec/complete · step · wp<br/>path_engine · plan_gaps · ac_verify"]
  STORE["<b>store.rs</b><br/>load · save · atomic_write"]
  SCHEMA["<b>schema.rs</b> → mini_schema<br/>shape enforcement"]
  FILES[("<b>master-plan/</b><br/>plan.json · milestones/*.json<br/>activity.json · reviews.json")]
  ASSETS["<b>assets.rs</b><br/>embedded templates + schemas"]

  ARGS --> CLI --> DISP --> CMD
  CMD --> VAL
  CMD --> DOM
  DOM --> STORE --> FILES
  DOM --> SCHEMA --> FILES
  ASSETS -.-> CMD
  VAL -.->|"violation → ok:false · exit 2"| CMD

  class ARGS,CLI,DISP,STORE,FILES store
  class CMD,DOM run
  class VAL,SCHEMA,ASSETS gate

  style FILES fill:#1f1f1f,stroke:#8a8a8a,stroke-width:2px,color:#e0e0e0

  classDef coord fill:#2b1a11,stroke:#e8834a,stroke-width:2px,color:#f7d9c2
  classDef run fill:#0f2b31,stroke:#4fc3d9,stroke-width:2px,color:#d3f0f7
  classDef rev fill:#241a33,stroke:#a98bd6,stroke-width:2px,color:#e6dbf7
  classDef gate fill:#122a1b,stroke:#6fbf7f,stroke-width:2px,color:#d6f0dc
  classDef human fill:#2b2410,stroke:#d8b34a,stroke-width:2px,color:#f4e7c2
  classDef done fill:#2b2410,stroke:#d8b34a,stroke-width:3px,color:#f4e7c2
  classDef store fill:#1c1c1c,stroke:#6a6a6a,stroke-width:2px,color:#d0d0d0
```

The output contract is uniform, which is what lets `raul`, scripts, and agents
all consume the same commands:

| Shape | When |
|-------|------|
| `{ "ok": true, … }` | the write landed |
| `{ "ok": false, "errors": [ … ] }` | a gate or a schema rejected it — **exit code 2** |
| JSON (default) | every read; add `--fields 'a.b,c[].d'` to project |
| `--summary` | rollups instead of full documents |

---

## Intake lanes

Not every change deserves a full milestone. Four lanes, sized by commitment.

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 300, "nodeSpacing": 28, "rankSpacing": 42}} }%%
flowchart LR
  Q{"a change arrives<br/>how big is it?"}
  IDEA["<b>idea</b><br/>open note<br/>no commitment"]
  TRACK["<b>track</b><br/>bugfix · tweak · chore<br/>one pass + a test command"]
  BACK["<b>backlog</b><br/>prioritized<br/>deferred scope"]
  MS["<b>milestone</b><br/>spec + ACs + work packages<br/>full two-round review"]

  Q -->|"a someday thought"| IDEA
  Q -->|"hours · one pass"| TRACK
  Q -->|"not now, but real"| BACK
  Q -->|"days · new behavior"| MS
  IDEA -.->|"promote"| TRACK
  IDEA -.->|"promote"| MS
  BACK -.->|"promote"| MS
  TRACK -->|"grew complex"| MS

  class Q human
  class IDEA,BACK,TRACK run
  class MS coord

  classDef coord fill:#2b1a11,stroke:#e8834a,stroke-width:2px,color:#f7d9c2
  classDef run fill:#0f2b31,stroke:#4fc3d9,stroke-width:2px,color:#d3f0f7
  classDef rev fill:#241a33,stroke:#a98bd6,stroke-width:2px,color:#e6dbf7
  classDef gate fill:#122a1b,stroke:#6fbf7f,stroke-width:2px,color:#d6f0dc
  classDef human fill:#2b2410,stroke:#d8b34a,stroke-width:2px,color:#f4e7c2
  classDef done fill:#2b2410,stroke:#d8b34a,stroke-width:3px,color:#f4e7c2
  classDef store fill:#1c1c1c,stroke:#6a6a6a,stroke-width:2px,color:#d0d0d0
```

| Lane | Commitment | External review |
|------|------------|-----------------|
| `idea` | none — a parked thought | no |
| `track` | minutes to hours, one pass | no — low blast radius |
| `backlog` | deferred, promotable later | no |
| `milestone` | days, full spec | **yes** — higher risk earns the loop |

A track that outgrows itself is promoted to a milestone and inherits the full
flow, so the cheap lane never becomes a place where risky work hides.

---

## The plan directory

One resource per file, so a write is a targeted write and a diff is readable.
`mp` owns all of it.

| Path | Holds |
|------|-------|
| `plan.json` | project config, the milestone index, adoption order and pins |
| `config.json` | workflow profile — `full` / `hybrid` / `session` — plus UI preferences |
| `milestones/<nn>-<slug>.json` | one self-contained milestone: spec, acceptance criteria, work packages, steps, stage map |
| `tracks/` | `bugfix` / `tweak` / `chore` item lists |
| `backlog.json` | formally deferred scope |
| `ideas.json` | parked ideas |
| `brief.json` | project brief topics from the first planning session |
| `decisions.json` | decision records |
| `annotations.json` | review requests, approval requests, notes |
| `reviews.json` + `reviews/` | verdicts, findings, challenge sessions |
| `activity.json` | the journal: who did what, when |
| `autopilot/<id>/session.json` | one autopilot run, self-contained and archivable |
| `archive/` | soft-deleted milestones and backlog items |
| `AGENTS.md` | generated session-start instructions for agents |

A milestone is a small document with four parts, and they are read by different
consumers — which is why the stage map is stored rather than derived:

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 300, "nodeSpacing": 28, "rankSpacing": 42}} }%%
flowchart LR
  A["<b>milestone</b><br/>spec + intent + scope"]
  B["<b>steps</b><br/>files · tests · done-when"]
  C["<b>flow_stages</b><br/>per-stage status"]
  D["<b>lifecycle</b><br/>one phase + overlays"]
  E["<b>reviews</b><br/>verdict + findings"]
  F["complete<br/>only via an independent pass"]

  A --> B
  A --> C
  A --> D
  D --> F
  E --> F

  class A,C,D store
  class B run
  class E rev
  class F done

  classDef coord fill:#2b1a11,stroke:#e8834a,stroke-width:2px,color:#f7d9c2
  classDef run fill:#0f2b31,stroke:#4fc3d9,stroke-width:2px,color:#d3f0f7
  classDef rev fill:#241a33,stroke:#a98bd6,stroke-width:2px,color:#e6dbf7
  classDef gate fill:#122a1b,stroke:#6fbf7f,stroke-width:2px,color:#d6f0dc
  classDef human fill:#2b2410,stroke:#d8b34a,stroke-width:2px,color:#f4e7c2
  classDef done fill:#2b2410,stroke:#d8b34a,stroke-width:3px,color:#f4e7c2
  classDef store fill:#1c1c1c,stroke:#6a6a6a,stroke-width:2px,color:#d0d0d0
```

`flow_stages` is the single source of truth for *which stage* a milestone is
on — both the CLI overview rollup and the TUI stage cell read it, so the two
views cannot disagree.

> Plans created by older versions carry TOML artifacts. `mp` detects them and
> refuses to initialize over them, pointing at the migration example rather than
> mixing formats in one directory.

---

## Related reading

- [`../mp/README.md`](../mp/README.md) — global flags and output conventions
- [`../mp/config.md`](../mp/config.md) — profiles and every config key
- [`../raul/README.md`](../raul/README.md) — the seven lanes and key bindings
- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — the module map, in prose
