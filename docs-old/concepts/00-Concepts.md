# Master Plan Concepts

This section contains conceptual documentation for the Master Plan toolkit. For command reference, see the [MP Manual](../mp/00-Manual.md).

## Core principle — size-aware routing

`mp` deliberately ships **separate intake surfaces** for separate sizes of
work, because one workflow to fit all sizes produces the failures that other
SDD tools are routinely mocked for (one-line bug → 16 acceptance criteria).

| Change shape | Surface | Why |
|---|---|---|
| 1-line bug / fix | `mp track add bugfix …` | Single commit, no interview, no approve. |
| 1-file tweak | `mp track add tweak …` | Same shape; routes to the Tweaks TUI lane. |
| Multi-step feature | `mp milestone create …` | Acceptance criteria + steps block. |
| Vague "later" idea | `mp idea create …` | Open ticket; no commitment. |
| Concrete but not-now | `mp backlog add …` | Title + priority + resolution; promote later. |

If you're not sure where something belongs, read
[Getting Started → SIZE-ROUTING.md](02 - Getting Started/SIZE-ROUTING.md)
first. The default when in doubt is the smallest artifact that fits.

## Table of Contents

- [Agent Integration](#01---agent-integration) — How AI assistants work with Master Plan
- [Getting Started](#02---getting-started) — Initial setup and orientation
- [Planning Methodology](#03---planning-methodology) — Core planning concepts and workflows
- [Project Management](#04---project-management) — Project-level management and decisions
- [Technical](#05---technical) — Technical implementation details
- [Reference](#06---reference) — Reference documentation
- [Process visuals](#process-visuals) — Diagram index, style guide, audience routing

---

## 01 - Agent Integration

Documentation for how AI assistants (OpenCode, Cursor) integrate with the Master Plan toolkit.

- [`AGENT-PLAYBOOK.md`](01%20-%20Agent%20Integration/AGENT-PLAYBOOK.md) — When and how to use `mp`, state transitions for agents
- [`AGENT-READINESS.md`](01%20-%20Agent%20Integration/AGENT-READINESS.md) — What Rust implements today vs documented CLI
- [`EXECUTION-MODES.md`](01%20-%20Agent%20Integration/EXECUTION-MODES.md) — Planning vs autonomous handoff modes
- [`EXECUTION-PATH.md`](01%20-%20Agent%20Integration/EXECUTION-PATH.md) — Suggested work order, pins, and focus

---

## 02 - Getting Started

Initial setup, installation, and orientation for new users and projects.
**Start at the SIZE-ROUTING guide** before any flow-specific page — the
"which artifact?" question precedes every other workflow decision.

- [`SIZE-ROUTING.md`](02 - Getting Started/SIZE-ROUTING.md) — **Decision matrix: track vs milestone vs idea vs backlog** (read first)
- [`AGENT-QUICKSTART.md`](02%20-%20Getting%20Started/AGENT-QUICKSTART.md) — Fast session-start → plan-ready path
- [`RAUL.md`](02%20-%20Getting%20Started/RAUL.md) — Human PM entry guide (`raul` CLI & TUI)
- [`raul-tui-walkthrough.md`](raul-tui-walkthrough.md) — Interactive TUI key bindings and review loop
- [`ADOPTION-CHECKLIST.md`](02%20-%20Getting%20Started/ADOPTION-CHECKLIST.md) — Day-one adoption guide
- [`ADOPTION-PROFILES.md`](02%20-%20Getting%20Started/ADOPTION-PROFILES.md) — Workflow profiles (full, session, hybrid)
- [`INSTALL.md`](02%20-%20Getting%20Started/INSTALL.md) — Install `mp` + wire Cursor and OpenCode
- [`README.md`](02%20-%20Getting%20Started/README.md) — Documentation index

---

## 03 - Planning Methodology

Core planning concepts, workflows, and methodologies for effective project planning.

- [`SPEC.md`](03%20-%20Planning%20Methodology/SPEC.md) — Data model, lifecycles, gates G1–G10
- [`PLANNING-STATUS.md`](03%20-%20Planning%20Methodology/PLANNING-STATUS.md) — Pipeline snapshot, phases, feature matrix
- [`PM-WORKFLOWS.md`](03%20-%20Planning%20Methodology/PM-WORKFLOWS.md) — PM cadences, funnel, daily flows
- [`GROOMING.md`](03%20-%20Planning%20Methodology/GROOMING.md) — Decompose, split, challenge, AC coverage
- [`PLANNING-ONLY-MODE.md`](03%20-%20Planning%20Methodology/PLANNING-ONLY-MODE.md) — Planning-vs-execution gate
- [`TEMPLATES.md`](03%20-%20Planning%20Methodology/TEMPLATES.md) — Defaults, views, interview mapping
- [`WALKTHROUGH.md`](03%20-%20Planning%20Methodology/WALKTHROUGH.md) — End-to-end OAuth example (interview → handoff)

---

## 04 - Project Management

Project-level management, decisions, and organizational guidance.

- [`DECISIONS.md`](04%20-%20Project%20Management/DECISIONS.md) — Architecture decisions (ADRs)
- [`BRANDING.md`](04%20-%20Project%20Management/BRANDING.md) — Names, `mp`, skill, display IDs

---

## 05 - Technical

Technical implementation details, design considerations, and operational aspects.

- [`BROWNFIELD.md`](05%20-%20Technical/BROWNFIELD.md) — Greenfield vs brownfield, delta specs
- [`BROWNFIELD-HANDOFF.md`](05%20-%20Technical/BROWNFIELD-HANDOFF.md) — Handoff mapping for existing code
- [`STORAGE.md`](05%20-%20Technical/STORAGE.md) — JSON-canonical persistence, json/raw output modes
- [`TESTING.md`](05%20-%20Technical/TESTING.md) — Fixture-driven TDD, scenarios
- [`CI.md`](05%20-%20Technical/CI.md) — CI validation setup
- [`DESIGN-REVIEW.md`](05%20-%20Technical/DESIGN-REVIEW.md) — Design audit and gap remediation
- [`EDGE-CASES.md`](05%20-%20Technical/EDGE-CASES.md) — Failure paths and concurrency
- [`EMERGENCY.md`](05%20-%20Technical/EMERGENCY.md) — Hotfix policy (tracks, no gate bypass)
- [`IDS.md`](05%20-%20Technical/IDS.md) — Outline IDs (M3, S3.1, splits)
- [`LEGACY.md`](05%20-%20Technical/LEGACY.md) — Superseded workflows

---

## 06 - Reference

Reference documentation and detailed specifications.

- [`MP-COMMANDS.md`](06%20-%20Reference/MP-COMMANDS.md) — Complete CLI reference

---

## Process visuals

Diagrams complement prose in concept docs. **Static model diagrams** (resource tree,
command hierarchy) live in [SPEC.md](03%20-%20Planning%20Methodology/SPEC.md) and
[MP-COMMANDS.md](06%20-%20Reference/MP-COMMANDS.md) (M48). **Process flows** below were
added or upgraded in M75.

### Diagram style guide

| Type | Use when | Example in this repo |
|------|----------|----------------------|
| `sequenceDiagram` | Ceremonies, handoffs, request/response between actors | Handoff baseline + plan diff ([EXECUTION-MODES](01%20-%20Agent%20Integration/EXECUTION-MODES.md)) |
| `flowchart` / `flowchart LR` | Pipelines, intake funnels, session routing | PM intake funnel ([PM-WORKFLOWS](03%20-%20Planning%20Methodology/PM-WORKFLOWS.md)) |
| `graph TD` / `graph LR` | Data models, hierarchies, anatomy | Resource model ([SPEC](03%20-%20Planning%20Methodology/SPEC.md)) |
| ASCII boxes | Quick mode summaries in agent docs; keep when a mermaid adds no scanability win | Planning vs autonomous mode ([EXECUTION-MODES](01%20-%20Agent%20Integration/EXECUTION-MODES.md) §2) |

**Rules:** one diagram per flow per home doc; cross-link instead of copy-paste. Validate
against shipped commands after CLI changes (`mp plan diff`, `mp reviews *`, etc.).

### Audience routing (single-source policy)

| Audience | Tool | Docs home | Maintenance |
|----------|------|-----------|-------------|
| **Shared concepts** | — | This index, SPEC, PM-WORKFLOWS, WALKTHROUGH | Update when methodology changes |
| **Agents (`mp`)** | `mp <cmd>` (JSON default) | AGENT-PLAYBOOK, EXECUTION-MODES, MP-COMMANDS | Command truth in MP-COMMANDS + AGENT-READINESS |
| **Humans (`raul`)** | `raul` CLI/TUI | [RAUL.md](02%20-%20Getting%20Started/RAUL.md), [raul-tui-walkthrough](raul-tui-walkthrough.md) | Daily commands in RAUL.md; MP-COMMANDS §22 is a pointer only |

Human tables and TUI behavior **must not** be duplicated in MP-COMMANDS — link to RAUL.md.
Agent JSON shapes stay in MP-COMMANDS / SPEC.

### Visual index (shipped flow diagrams)

| Flow | Diagram type | Home doc | Audience |
|------|--------------|----------|----------|
| Plan resource model | `graph TD` | [SPEC.md §2](03%20-%20Planning%20Methodology/SPEC.md) | Shared |
| Milestone anatomy (spec vs plan) | `graph LR` | [SPEC.md §2](03%20-%20Planning%20Methodology/SPEC.md) | Shared |
| `mp` command hierarchy | `graph TD` | [MP-COMMANDS.md §0](06%20-%20Reference/MP-COMMANDS.md) | Agent |
| OAuth walkthrough timeline | `flowchart LR` | [WALKTHROUGH.md](03%20-%20Planning%20Methodology/WALKTHROUGH.md) | Shared |
| Planning vs autonomous modes | ASCII | [EXECUTION-MODES.md §2](01%20-%20Agent%20Integration/EXECUTION-MODES.md) | Shared |
| Handoff ceremony + baseline diff | `sequenceDiagram` | [EXECUTION-MODES.md §5.1](01%20-%20Agent%20Integration/EXECUTION-MODES.md#51-handoff-sequence-m70-baseline) | Shared / agent |
| Execute → review → remediate | `sequenceDiagram` | [AGENT-PLAYBOOK.md §9](01%20-%20Agent%20Integration/AGENT-PLAYBOOK.md#9-process-diagrams) | Agent |
| Agent session start | `flowchart TD` | [AGENT-PLAYBOOK.md §9](01%20-%20Agent%20Integration/AGENT-PLAYBOOK.md#9-process-diagrams) | Agent |
| PM intake funnel | `flowchart TD` | [PM-WORKFLOWS.md §1](03%20-%20Planning%20Methodology/PM-WORKFLOWS.md) | Shared |
| Harness install path | `flowchart LR` | [INSTALL.md §4](02%20-%20Getting%20Started/INSTALL.md) | Human / agent |
| Human PM daily commands | prose + tables | [RAUL.md](02%20-%20Getting%20Started/RAUL.md) | Human |
| TUI review loop | prose + keys | [raul-tui-walkthrough.md](raul-tui-walkthrough.md) | Human |

### Flow audit backlog (M75 S1)

Prioritized flows catalogued during the concepts audit. **Done** = diagram shipped in M75;
**Existing** = already had adequate visuals; **ASCII** = prose/boxes only (future upgrade optional).

| Flow | Target doc | Diagram type | Audience | Status |
|------|------------|--------------|----------|--------|
| Handoff check → baseline → plan diff → pause | EXECUTION-MODES | sequence | Agent | Done (M75) |
| Execute → review → remediate (M64) | AGENT-PLAYBOOK | sequence | Agent | Done (M75) |
| Agent session start | AGENT-PLAYBOOK | flowchart | Agent | Done (M75) |
| PM intake funnel | PM-WORKFLOWS | flowchart | Shared | Done (M75) |
| Install / harness registry / doctor | INSTALL | flowchart | Human | Done (M75) |
| Human PM entry + daily raul | RAUL.md | prose | Human | Done (M75) |
| OAuth end-to-end | WALKTHROUGH | flowchart LR | Shared | Existing |
| Resource + milestone anatomy | SPEC | graph | Shared | Existing |
| Command hierarchy | MP-COMMANDS | graph TD | Agent | Existing (drift pass M75) |
| Suggested work order / pins | EXECUTION-PATH | ASCII | Agent | ASCII |
| Test pyramid | TESTING | ASCII | Agent | ASCII |
| Brownfield adoption fork | BROWNFIELD | ASCII | Agent | ASCII |
| Spec grooming challenge loop | GROOMING | tables | Agent | ASCII |
| **Raul doc gap (pre-M75)** | — | — | Human | Closed via RAUL.md + index |

### M75 drift pass (S7)

Validated existing mermaid against shipped CLI (post-M70/M74):

| Doc | Check | Result |
|-----|-------|--------|
| MP-COMMANDS §0 hierarchy | Added `plan diff`, `reviews *`, `execution handoff-show`, `execution report` | Fixed |
| SPEC resource/anatomy | Resource names match `master-plan/` layout | OK |
| WALKTHROUGH timeline | Handoff phase links to EXECUTION-MODES §5.1 + plan diff | Updated |
| MP-COMMANDS §22 | Slimmed to RAUL.md pointer | Updated |

---

*Conceptual documentation provides methodology, best practices, and implementation guidance separate from the command reference in the MP Manual.*