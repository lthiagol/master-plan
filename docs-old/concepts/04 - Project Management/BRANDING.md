# Master Plan — Branding

**Tagline:** Your master plan — structured for agents, readable for humans.

---

## Product

| | |
|--|--|
| **Name** | Master Plan |
| **What it is** | Spec-driven planning toolkit for agentic software development |
| **Repository** | `master-plan` |
| **Voice** | Clear, capable, slightly playful — planning should feel worth doing |

**One-liners (use in README, install, talks):**

- Spec before code. Agents use `mp`.
- The control plane for agentic project planning.
- Your master plan — structured for agents, readable for humans.

---

## Artifact (per project)

| | |
|--|--|
| **Name** | `master-plan/` |
| **Role** | Single source of truth for what to build, in what order, and how to verify |
| **Rule** | Agents never edit files here directly — they use the `mp` CLI |

---

## CLI

| | |
|--|--|
| **Binary** | `mp` |
| **Full name** | Master Plan CLI |
| **Do not call it** | helper script, helper, mph (legacy) |

The CLI is the **engine** / **runtime** for the plan. It owns all reads and writes.

```bash
mp init
mp idea create --title "Installer design"
mp show milestone 03
```

### Environment variables

| Variable | Purpose |
|----------|---------|
| `MP_HOME` | Toolkit root (default: `~/.agents/master-plan`) |
| `MP_PROJECT` | Project root override |
| `MP_CONFIG` | Global config override |

**Legacy aliases** (supported, do not document prominently): `MPH_HOME`, `MPH_PROJECT`, `MPH_CONFIG`.

---

## Agent skill

| | |
|--|--|
| **Name** | `master-planner` |
| **Path** | `~/.agents/skills/master-planner/SKILL.md` |
| **Slash command** | `/mp` |

Harness-specific wiring for **Cursor** and **OpenCode** is documented in
[INSTALL.md](./INSTALL.md). v1 installer (`mp install`, P0.9) mirrors the skill to
`~/.cursor/skills/` and installs to `~/.agents/skills/` (OpenCode-native). The skill
and `mp` CLI remain harness-agnostic in content.

### Auto-trigger hints

Activate when the user mentions: master plan, brief, milestone, roadmap, spec, backlog, ideas, tracks, `/mp`, or “what’s next?”

### Skill contract (summary)

1. Read `master-plan/AGENTS.md` in the project.
2. Use `mp` for all plan I/O — never edit `master-plan/` directly.
3. Reads: `mp <cmd>` (JSON default). User display: `raul` or summarize JSON.

---

## Install layout

```text
~/.agents/
├── master-plan/              # toolkit (templates, schemas, bin)
│   └── bin/mp
└── skills/
    └── master-planner/
        └── SKILL.md
```

---

## Naming map

```text
Master Plan                 ← product
├── master-plan/            ← artifact (in each repo)
├── mp                        ← CLI
├── master-planner            ← skill
├── /mp                       ← slash
└── ~/.agents/master-plan/    ← installed toolkit
```

---

## Display IDs (CLI output)

| Entity | On-disk / canonical | Display example |
|--------|---------------------|-----------------|
| Brief topic | `T01` | `T01 — Problem & motivation` |
| Milestone | `03`, `03.1` | `M3 — OAuth Login`, `M3.1 — OAuth UI` |
| Step | `S1`, `S3.1` | `S3.1` (scoped to milestone; CLI: `03 / S3.1`) |
| Acceptance | `AC-01` | `AC-01` |
| Work package | `WP1` | `WP1 — OAuth endpoints` (grouping only) |
| Track item | `BF-01`, `TW-01` | `BF-01` |
| Idea | `ID-01` | `ID-01` |
| Backlog | `B-01` | `B-01` |
| Challenge finding | `F-01` | `F-01` |

**Full rules:** [IDS.md](./IDS.md).

On-disk milestone files use `03`, `03.1`; step ids use the `S` prefix inside milestone JSON.
Pretty names are for humans (`raul` display) and agent summaries.

---

## Words to use / avoid

| Prefer | Avoid |
|--------|-------|
| Master Plan CLI, `mp` | helper script, mph |
| engine, runtime, toolkit | utility, wrapper |
| master-plan/ (artifact) | .master-plan/ (unless we change policy) |
| spec-driven | waterfall (unless contrasting) |
| outline IDs `S3.1` | WP-local `1.2` step numbering (deprecated) |
