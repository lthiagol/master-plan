# Planning Status — What We Have So Far

Snapshot of the **Master Plan** design: documented, planned, and implemented in Rust.
Updated as the model evolves. **No single source replaces the linked docs** — this is
the map.

**Tagline:** *Your master plan — structured for agents, readable for humans.*

---

## 1. What this is

A **spec-driven planning toolkit** for agentic development:

- **Artifact:** `master-plan/` per project (JSON on disk and at the CLI boundary)
- **CLI:** `mp` — owns all reads/writes; agents never hand-edit plan files
- **Skill:** `master-planner` — harness integration (Cursor + OpenCode; see [INSTALL.md](./INSTALL.md))
- **Voice:** [BRANDING.md](./BRANDING.md)

---

## 2. Planning pipeline (end-to-end)

```text
mp init
  │
  ▼
brief (brainstorm placeholders)     Implemented
  │ mp brief todo → edit → done
  ▼
charter (plan.json)                 Implemented
  │ interview checklist --type charter
  ▼
milestones — phase 1 spec           Implemented
  │ interview → create → approve
  ▼
decompose — phase 2 steps           Implemented
  │ milestone decompose, wp/step add, covers_ac
  ▼
challenge (optional)                Implemented
  │ challenge start → audit → resolve → done
  ▼
execution path                      Implemented
  │ mp path → next-step → implement
  ▼
complete                            Implemented
  │ criterion pass → milestone complete
```

**Parallel lanes:** tracks (bugfix/tweak), ideas (park), backlog (defer).

---

## 3. Document map

| Doc | Contents |
|-----|----------|
| [BRANDING.md](./BRANDING.md) | Product name, `mp`, skill, display IDs |
| [SPEC.md](./SPEC.md) | Data model, lifecycles, gates G1–G10, workflows |
| [STORAGE.md](./STORAGE.md) | JSON on disk and at CLI, json/raw output modes |
| [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) | Full CLI reference + implementation phases |
| [IDS.md](./IDS.md) | Outline IDs: M3, S3, S3.1 splits |
| [GROOMING.md](./GROOMING.md) | List filters, decompose, split, challenge, AC coverage |
| [BROWNFIELD.md](./BROWNFIELD.md) | Greenfield vs brownfield, delta specs, plan/code zones |
| [EXECUTION-PATH.md](./EXECUTION-PATH.md) | Suggested order, `mp path`, automatic vs manual |
| [TEMPLATES.md](./TEMPLATES.md) | Defaults, views, interview mapping |
| [TESTING.md](./TESTING.md) | Fixture-driven TDD, scenarios, goldens |
| [PM-WORKFLOWS.md](./PM-WORKFLOWS.md) | PM cadences, daily flows, backlog gaps |
| [AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md) | Agent state updates, when to use mp |
| [EXECUTION-MODES.md](./EXECUTION-MODES.md) | Planning vs autonomous, execution_ready |
| [AGENT-READINESS.md](./AGENT-READINESS.md) | Rust vs documented CLI matrix |
| [DECISIONS.md](./DECISIONS.md) | ADRs — resolved design choices |
| [EDGE-CASES.md](./EDGE-CASES.md) | Failure paths, concurrency |
| [EMERGENCY.md](./EMERGENCY.md) | Hotfix policy |
| [CI.md](./CI.md) | Validate in GitHub Actions |
| [LEGACY.md](./LEGACY.md) | Markdown + Bash superseded |
| [DESIGN-REVIEW.md](./DESIGN-REVIEW.md) | Gaps audit + remediation index |
| [INSTALL.md](./INSTALL.md) | v1 install — Cursor + OpenCode |
| [README.md](./README.md) | Documentation index (all docs) |
| [ADOPTION-PROFILES.md](./ADOPTION-PROFILES.md) | full / session / hybrid; per-project workflow config |
| [ADOPTION-CHECKLIST.md](./ADOPTION-CHECKLIST.md) | Day-one adoption (personal vs work) |
| [PLANNING-STATUS.md](./PLANNING-STATUS.md) | This file |

**Agent contract:** [templates/AGENTS-TEMPLATE.md](../templates/AGENTS-TEMPLATE.md)  
**Skill:** [templates/skills/master-planner/SKILL.md](../templates/skills/master-planner/SKILL.md)

---

## 4. Core concepts (cheat sheet)

| Concept | ID / file | Purpose |
|---------|-----------|---------|
| **Brief** | `T01`… `brief.json` | Day-zero brainstorm before charter |
| **Charter** | `plan.json` | Product goals, stack, execution prefs |
| **Milestone** | `03`, `03.1` / M3 | Feature or phase; two-phase spec + plan |
| **Step** | `S1`, `S3.1` | Atomic implementation action |
| **Work package** | `WP1` | Merge/rollback grouping (not in step ID) |
| **AC** | `AC-01` | Acceptance criterion — what “done” means |
| **AC coverage** | `steps[].covers_ac` | Each AC mapped to ≥1 step |
| **Track item** | `BF-01` | Small perpetual bugfix/tweak |
| **Idea** | `ID-01` | Park for later |
| **Challenge** | `F-01` in `reviews/challenges/` | Structured plan audit |
| **Path** | computed | Suggested work queue |

---

## 5. Ordering: automatic vs manual

Full detail: [EXECUTION-PATH.md §3.1](./EXECUTION-PATH.md#31-automatic-vs-manual).

| | Automatic | Manual |
|--|-----------|--------|
| **What** | Recompute queue when facts change | Set deps, pins, priority, focus |
| **Commands** | `mp path`, `next-step`, `graph explain`, `validate` | `path pin`, `focus`, `milestone update` |
| **Persists?** | No (derived each read) | Yes (`plan.json`, milestone files) |
| **Silent file edits?** | Never | Only via explicit `mp` writes |

**Planned:** ~~`mp path suggest`~~ — **Implemented** (M06); propose pins; user confirms with `path pin`.

---

## 6. Feature status

### Legend

| State | Meaning |
|-------|---------|
| **Implemented** | In Rust `crates/mp` today |
| **Documented** | Spec + MP-COMMANDS; not in Rust yet |
| **Partial** | Some commands work; gaps per phase table |

### By area

| Area | Doc phase | Rust | Notes |
|------|-----------|------|-------|
| Init, profiles, doctor, validate | P0 | **Implemented** | G1–G13, brief on init |
| **Install + harness** | P0.9 | **Implemented** | `mp install`, project skills |
| Status, list, show, next/path | P0–P1.8 | **Implemented** | Path engine, filters, `execution_ready` |
| Interview checklist / gaps | P0 | **Implemented** | Multiple checklist types |
| Tracks (bugfix/tweak) | P1.5 | **Implemented** | Includes `track promote` |
| Archive / restore / purge | P2 | **Implemented** | |
| **Brief** | P0.5 | **Implemented** | `brief promote`, `brief reopen` |
| Milestone spec lifecycle | P1 | **Implemented** | create → complete + split |
| Steps, wp, decompose, plan | P1 / P1.7 | **Implemented** | `milestone plan` scaffolds closure WP |
| Ideas, backlog promote | P1.6 / P3 | **Implemented** | Full promote ladder |
| Groom, challenge | P1.7 | **Implemented** | |
| **Path, pin, focus, graph** | P1.8 | **Implemented** | `path suggest` (M06) |
| **PM surface (inbox, handoff)** | P1.9 | **Implemented** | hygiene, digest, execution |
| Charter, backlog, config, export, git | P3 | **Implemented** | `git.auto_commit` on complete; `git.auto_push` on commit |
| **Adoption profiles, session** | P3.1 | **Implemented** | `--profile`, `--from-repo`, `session *` |
| Brownfield / delta specs | P4 | **Implemented** | scan, merge, rebase |
| Fixture + scenario tests | — | **Implemented** | 55 integration + scenario goldens (`make test-scenarios`) |

### Rust commands today (`mp`)

See [AGENT-READINESS.md](./AGENT-READINESS.md) for the full matrix. Summary:

```text
init · install · uninstall · doctor · validate · sync · status · export · git *
brief * · idea * · session * · backlog * · track * · milestone * · step * · wp *
path · path suggest · next · next-step · graph · inbox · hygiene · groom · digest · challenge *
plan * · specs * · brownfield · delta · execution * · archive · restore · purge
config * · decision * · metrics * · interview *
```

**Deferred (backlog):** see [v1.1 roadmap §14](#14-v11-roadmap) — grouped in `master-plan/backlog.json`

---

## 13. v0.2 roadmap

**Status:** **Complete** — M07–M10 verified/done (2026-06-18). Shipped as part of v1.0.0. Next target: **1.1** ([§14](#14-v11-roadmap)).

### Shipped (v1) — milestones 01–06

| ID | Title | Status |
|----|-------|--------|
| 01 | CLI foundation | verified / done |
| 02 | Adoption & PM surface | verified / done |
| 03 | Brownfield & execution | verified / done |
| 04 | Promote ladder & publish | verified / done |
| 05 | Meta master-plan | verified / done |
| 06 | Remaining polish | verified / done |

### Shipped (v0.2) — milestones 07–10

| Batch | Milestone | Status | Theme |
|-------|-----------|--------|-------|
| **A — Quality** | **07** Quality & CI | done | JSON schema on write (ADR-004), GHA `mp validate` |
| **B — Session** | **08** Session model depth | done | `sessions/` tree; one session per branch |
| **C — Scale** | **09** Scale & PM maturity | done | Optimistic concurrency, dup-check, `mp note` |
| **D — Docs** | **10** Documentation alignment | done | Sync headers; MP-COMMANDS/PM-WORKFLOWS truth |

### Backlog groups (resolved)

| Group | Items | Resolution |
|-------|-------|------------|
| `v0.2-quality` | B-04 – B-06 | M07 |
| `v0.2-docs` | B-13 – B-15 | M10 |
| `v0.2-session` | B-07 – B-08 | M08 |
| `v0.2-scale` | B-09 – B-11 | M09 |

---

## 14. v1.0 roadmap (shipped)

**Status:** **Complete** — M11–M15 verified/done (2026-06-18). v1.0.0 tag + Gitea release published; B-16 resolved as shipped.

### Shipped (v1.0) — milestones 11–15

| Batch | Milestone | Status | Theme |
|-------|-----------|--------|-------|
| **Release** | **11** v1.0 release & adoption | done | v1.0.0 tag, Gitea release, CHANGELOG |
| **Docs** | **12** Post-v0.2 doc truth sync | done | AGENT-READINESS, PLANNING-STATUS, DESIGN-REVIEW |
| **Session** | **13** Session UX depth | done | `mp session focus` (config.json persistence) |
| **Dist** | **14** Homebrew distribution | done | Formula in `lthiagol/homebrew-tap` |
| **Docs** | **15** MP-COMMANDS completeness | done | Full per-command doc sync |

### Backlog — release & distribution (resolved)

| ID | Item | Resolution |
|----|------|------------|
| B-16 | v1.0 tag + release notes | shipped (M11) |
| B-12 | Homebrew formula | shipped (M14) |

---

## 15. v1.1 roadmap (in-execution)

**Status:** In-execution — M16 in-progress; M17–M22 spec-approved (`ready`), execution gated on M16 (plan hygiene).

### Active milestones

| ID | Title | Effort | Risk | Theme |
|----|-------|--------|------|-------|
| **16** | Plan hygiene & v1.0 closure | S | low | Resolve B-16, sync plan index, target_version=1.1, docs closure |
| **17** | Code health & refactoring | L | med | Refactor `crates/mp` modules, reduce duplication |
| **18** | Doc-code drift closure | M | low | Sync docs to current CLI behavior |
| **19** | AGENTS.md simplification | S | low | Trim master-plan/AGENTS.md workflow verbosity |
| **20** | Architecture map (ARCHITECTURE.md) | S | low | New ARCHITECTURE.md describing crate layout |
| **21** | Dependency diet | S | low | Trim unused/implied deps in Cargo.toml |
| **22** | Remove tera dependency | S | low | Drop tera from renderer path |

**Recommended order:** M16 → M17 → M18 → M19 → M20 → M21 → M22

### Backlog — P5 / future (no milestone yet)

| ID | Item |
|----|------|
| B-17 | Portfolio / multi-session dashboard |
| B-18 | Capacity / sprint iterations |
| B-19 | `blocks_external` vendor wait field |
| B-20 | Multi-writer file locks |
| B-21 | P4.1 multi-domain delta |
| B-22 | Doctor schema/version mismatch check |
| B-23 | Milestone `blocks` field |
| B-24 | Merge `planning_status` + `planning_phase` |
| B-25 | Per-crate monorepo plan dirs |
| B-26 | Step claim / WIP owner |
| B-27 | brownfield-likely fixture |
| B-28 | `mp doctor --harness` checks |
| B-29 | CLI/schema version compatibility stamp |

### Decisions (v1.1)

| ID | Topic | Status |
|----|-------|--------|
| D-006 – D-010 | Release semver, session focus, Homebrew tap | **ACCEPTED** (2026-06-18) |

Recorded in `master-plan/decisions.json`; normative ADRs in [DECISIONS.md](./DECISIONS.md).

---

## 7. Implementation phases (roadmap)

| Phase | Focus | Status |
|-------|-------|--------|
| **P0** | Query, init, validate, interview | **Implemented** |
| **P0.9** | `mp install`, harness doctor | **Implemented** |
| **P0.5** | Brief bootstrap + promote | **Implemented** |
| **P1** | Milestone spec + steps + complete | **Implemented** |
| **P1.5** | Tracks + promote | **Implemented** |
| **P1.6** | Ideas + promote | **Implemented** |
| **P1.7** | Grooming, challenge, decompose, plan, split | **Implemented** |
| **P1.8** | Path, pins, graph, coverage | **Implemented** |
| **P1.9** | Inbox, block, execution handoff, hygiene, digest | **Implemented** |
| **P1.10** | `milestone reopen`, `delta rebase` | **Implemented** |
| **Walkthrough** | [WALKTHROUGH.md](./WALKTHROUGH.md) + fixtures | **Implemented** (9 scenarios) |
| **P2** | Archive | **Implemented** |
| **P3** | Charter, backlog, config, export, git, sync | **Implemented** |
| **P3.1** | Adoption profiles, `[workflow]`, `session *` | **Implemented** |
| **P4** | Brownfield delta specs | **Implemented** |

---

## 8. Schemas & templates

| Asset | Path |
|-------|------|
| Milestone schema | `schemas/milestone.schema.json` |
| Brief schema | `schemas/brief.schema.json` |
| Challenge schema | `schemas/challenge.schema.json` |
| Idea schema | `schemas/idea.schema.json` |
| Track schema | `schemas/track.schema.json` |
| Domain spec schema | `schemas/spec-domain.schema.json` (P4) |
| Plan schema | `schemas/plan.schema.json` |
| Interview checklist | `schemas/interview-checklist.json` |
| Defaults | `templates/defaults/*.json` |
| Human views | `templates/views/*.md` |

---

## 9. Per-project layout (target)

```text
<project-root>/
├── AGENTS.md
└── master-plan/
    ├── AGENTS.md
    ├── plan.json              # charter + [execution] prefs
    ├── brief.json             # P0.5
    ├── config.json
    ├── backlog.json
    ├── ideas.json             # P1.6
    ├── decisions.json
    ├── milestones/*.json      # spec + [[steps]] + [[work_packages]]
    ├── specs/*.json           # P4 domain truth (brownfield)
    ├── tracks/bugfix.json
    ├── tracks/tweak.json
    ├── reviews/challenges/    # P1.7
    └── archive/
```

---

## 10. Gates (validation)

| Gate | Rule |
|------|------|
| G1 | No code before spec `ready` |
| G5 | No impl plan before spec `ready` |
| G8 | Deps done before milestone `in-progress` |
| G9 | Step deps before step `in-progress` (P1.8) |
| G10 | AC coverage before `in-progress` (warn/strict) |
| G11–G13 | Delta domain merge (P4 brownfield) |
| B1–B3 | Brief completion |

Full list: [SPEC.md §5](./SPEC.md#5-gates-enforced-by-mp-validate).

---

## 11. Typical agent flows

**New project**

```text
mp init → mp brief * → mp brief done → charter interview → milestone spec → approve
→ mp milestone decompose → mp challenge audit → mp path → mp next
```

**“What’s next?”**

```text
mp status → mp path  → mp next
```

**“Do M4 before M3”**

```text
mp path pin 04 --before 03
```

**“Challenge this plan”**

```text
mp challenge start 03 --scope plan → audit → list → resolve → done
```

---

## 12. References

- Repository README: [../README.md](../README.md)
- Implementation crate: [../crates/mp](../crates/mp)
