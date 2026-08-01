# Master Plan — Documentation Index

All planning docs live here. The repository [README](../README.md) is the short entry
point; this file is the **full map**.

**New here?** [PLANNING-STATUS.md](./PLANNING-STATUS.md) → [AGENT-READINESS.md](./AGENT-READINESS.md) → [INSTALL.md](./INSTALL.md).

---

## Getting started

**Start here:** [SIZE-ROUTING.md](./SIZE-ROUTING.md) — the smallest-artifact-first
decision matrix. Pick the right intake surface (track vs milestone vs idea vs
backlog) before consulting any flow-specific page.

| Doc | What |
|-----|------|
| [SIZE-ROUTING.md](./SIZE-ROUTING.md) | **Read first** — track vs milestone vs idea vs backlog decision matrix |
| [INSTALL.md](./INSTALL.md) | Install `mp` + wire **Cursor** and **OpenCode** |
| [RAUL.md](./RAUL.md) | Human PM entry — `raul` CLI, dashboard, daily commands |
| [raul-tui-walkthrough.md](../raul-tui-walkthrough.md) | TUI key bindings and review loop |
| [AGENT-QUICKSTART.md](./AGENT-QUICKSTART.md) | Fast session-start → plan-ready path |
| [ADOPTION-CHECKLIST.md](./ADOPTION-CHECKLIST.md) | Day one — personal (`full`) vs work (`hybrid`) |
| [ADOPTION-PROFILES.md](./ADOPTION-PROFILES.md) | `full` / `session` / `hybrid` per-project config |
| [WALKTHROUGH.md](./WALKTHROUGH.md) | End-to-end OAuth example (interview → handoff) |
| [BRANDING.md](./BRANDING.md) | Names, `mp`, skill, display IDs |

```bash
make install          # toolkit + OpenCode + Cursor skills
make dev-env          # local dev exports
```

---

## Core specification

| Doc | What |
|-----|------|
| [SPEC.md](./SPEC.md) | Data model, lifecycles, gates G1–G10 |
| [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) | Full `mp` CLI reference (implementation spec) |
| [IDS.md](./IDS.md) | Outline IDs — M3, S3.1, splits |
| [STORAGE.md](./STORAGE.md) | JSON-canonical persistence, json/raw output modes |
| [TEMPLATES.md](./TEMPLATES.md) | Defaults, views, interview mapping |
| [DECISIONS.md](./DECISIONS.md) | Architecture decisions (ADRs) |

Schemas: [`../schemas/`](../schemas/) · Templates: [`../templates/`](../templates/)

---

## Agents & harnesses

| Doc | What |
|-----|------|
| [AGENT-QUICKSTART.md](./AGENT-QUICKSTART.md) | Fast session-start → plan-ready path |
| [AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md) | When to use `mp`, state transitions |
| [AGENT-READINESS.md](./AGENT-READINESS.md) | **What Rust implements today** |
| [BROWNFIELD-HANDOFF.md](./BROWNFIELD-HANDOFF.md) | Handoff mapping flow for existing-code projects |
| [BROWNFIELD.md](./BROWNFIELD.md) | Greenfield vs brownfield, delta specs |
| [EXECUTION-MODES.md](./EXECUTION-MODES.md) | Planning vs autonomous handoff |
| [PLANNING-ONLY-MODE.md](./PLANNING-ONLY-MODE.md) | Planning-vs-execution gate |
| [EMERGENCY.md](./EMERGENCY.md) | Hotfix policy (tracks, no gate bypass) |
| [EDGE-CASES.md](./EDGE-CASES.md) | Failure paths, concurrency |

Project agent contract: [`../templates/AGENTS-TEMPLATE.md`](../templates/AGENTS-TEMPLATE.md)  
Skill: [`../templates/skills/master-planner/SKILL.md`](../templates/skills/master-planner/SKILL.md)

---

## Planning & delivery

| Doc | What |
|-----|------|
| [PM-WORKFLOWS.md](./PM-WORKFLOWS.md) | PM cadences, funnel, daily flows |
| [GROOMING.md](./GROOMING.md) | Decompose, split, challenge, AC coverage |
| [EXECUTION-PATH.md](./EXECUTION-PATH.md) | Suggested work order, pins, focus |
| [PLANNING-STATUS.md](./PLANNING-STATUS.md) | Pipeline snapshot, phases, feature matrix |

---

## Quality & maintenance

| Doc | What |
|-----|------|
| [TESTING.md](./TESTING.md) | Fixture-driven TDD, scenarios |
| [CI.md](./CI.md) | Validate in GitHub Actions |
| [DESIGN-REVIEW.md](./DESIGN-REVIEW.md) | Gaps audit + remediation index |
| [LEGACY.md](./LEGACY.md) | Superseded markdown + Bash workflow |

Fixtures: [`../tests/fixtures/`](../tests/fixtures/) · Scenarios: [`../tests/scenarios/`](../tests/scenarios/)

```bash
make test-fixtures    # mp validate on fixture projects
```

---

## Implementation status

| Track | Status | Doc |
|-------|--------|-----|
| **v1 RC** (M01–M06) | Shipped | [AGENT-READINESS.md](./AGENT-READINESS.md) |
| **v0.2** (M07–M10) | Planned | [PLANNING-STATUS.md §13](./PLANNING-STATUS.md#13-v02-roadmap) |
| **Adoption** | Ready | [ADOPTION-CHECKLIST.md](./ADOPTION-CHECKLIST.md) |

Historical phase labels (P0–P4) remain in [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) for traceability.

---

## Full file list

| File | Topic |
|------|-------|
| [ADOPTION-CHECKLIST.md](./ADOPTION-CHECKLIST.md) | Day-one adoption |
| [ADOPTION-PROFILES.md](./ADOPTION-PROFILES.md) | Workflow profiles |
| [AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md) | Agent workflows |
| [AGENT-READINESS.md](./AGENT-READINESS.md) | Rust vs documented CLI |
| [BRANDING.md](./BRANDING.md) | Product naming |
| [BROWNFIELD.md](./BROWNFIELD.md) | Existing codebases |
| [CI.md](./CI.md) | CI validate |
| [DECISIONS.md](./DECISIONS.md) | ADRs |
| [DESIGN-REVIEW.md](./DESIGN-REVIEW.md) | Design audit |
| [EDGE-CASES.md](./EDGE-CASES.md) | Edge cases |
| [EMERGENCY.md](./EMERGENCY.md) | Hotfixes |
| [EXECUTION-MODES.md](./EXECUTION-MODES.md) | Autonomous mode |
| [EXECUTION-PATH.md](./EXECUTION-PATH.md) | Work queue |
| [GROOMING.md](./GROOMING.md) | Spec grooming |
| [IDS.md](./IDS.md) | ID rules |
| [INSTALL.md](./INSTALL.md) | Cursor + OpenCode install |
| [RAUL.md](./RAUL.md) | Human PM entry (raul CLI & TUI) |
| [LEGACY.md](./LEGACY.md) | Old workflow |
| [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) | CLI reference |
| [PLANNING-STATUS.md](./PLANNING-STATUS.md) | Status snapshot |
| [PM-WORKFLOWS.md](./PM-WORKFLOWS.md) | PM cadences |
| [SPEC.md](./SPEC.md) | Canonical spec |
| [STORAGE.md](./STORAGE.md) | Storage model |
| [TEMPLATES.md](./TEMPLATES.md) | Templates |
| [TESTING.md](./TESTING.md) | Test strategy |
| [WALKTHROUGH.md](./WALKTHROUGH.md) | OAuth walkthrough |
