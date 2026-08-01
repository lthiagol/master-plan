# Design Review — Gaps, Risks, and Enhancements

> **Historical audit (2026-06-17).** Most findings below were resolved in v1 (M01–M06) and v0.2 (M07–M10).  
> **Current Rust truth:** [AGENT-READINESS.md](./AGENT-READINESS.md) · **Active work:** v0.3 M11–M15 in [PLANNING-STATUS §14](./PLANNING-STATUS.md#14-v03-roadmap).

Living audit of documentation, requirements, and model consistency.  
**Date:** 2026-06-17 · **Last addressed:** 2026-06-18 (v1 RC)

Use with [PLANNING-STATUS.md](./PLANNING-STATUS.md). Findings below; **remediation** in linked docs.

---

## 0. Remediation index (addressed in docs)

| Finding | Addressed in |
|---------|----------------|
| Steps flat vs nested | [DECISIONS.md ADR-001](./DECISIONS.md#adr-001-steps-on-disk), [IDS.md](./IDS.md) |
| planning_status vs phase | [DECISIONS.md ADR-002](./DECISIONS.md), [SPEC.md §4.7](./SPEC.md#47-planning_status-vs-planning_phase-matrix) |
| `[execution]` split | [DECISIONS.md ADR-003](./DECISIONS.md#adr-003-execution-config-split) |
| init vs brief.json | [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) init section |
| Validate gaps | [SPEC.md §5.1](./SPEC.md#51-gate-enforcement-matrix), [AGENT-READINESS.md](./AGENT-READINESS.md) |
| next-step / handoff risk | [DECISIONS.md ADR-006](./DECISIONS.md), [EXECUTION-MODES.md](./EXECUTION-MODES.md) |
| Legacy confusion | [LEGACY.md](./LEGACY.md), [instructions.md](../instructions.md) banner |
| plan.schema.json | [schemas/plan.schema.json](../schemas/plan.schema.json) |
| track promote | MP-COMMANDS marked P1 |
| Agent unimplemented cmds | [AGENT-READINESS.md](./AGENT-READINESS.md) |
| Uncovered situations | [EDGE-CASES.md](./EDGE-CASES.md) |
| Hotfix policy | [EMERGENCY.md](./EMERGENCY.md) |
| CI | [CI.md](./CI.md) |
| Adoption profiles / workflow config | [ADOPTION-PROFILES.md](./ADOPTION-PROFILES.md), [ADOPTION-CHECKLIST.md](./ADOPTION-CHECKLIST.md), [DECISIONS.md ADR-008](./DECISIONS.md#adr-008-adoption-profiles-in-project-config) |
| v1 harness install (Cursor, OpenCode) | [INSTALL.md](./INSTALL.md), [DECISIONS.md ADR-009](./DECISIONS.md#adr-009-v1-harness-targets-cursor--opencode) |
| criterion fail, step fail, reopen | [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) |

**Open / backlog (not in v0.3 milestones):** see [PLANNING-STATUS §14](./PLANNING-STATUS.md#14-v03-roadmap) backlog groups B-17–B-28 in `master-plan/backlog.json`. Use [AGENT-READINESS.md](./AGENT-READINESS.md) before calling CLI commands.

---

## 1. Executive summary (historical — pre-v1 audit)

**At audit time (2026-06-17):** Clear product vision (spec-before-code, `mp`-only I/O), rich
command spec, recent agent/PM docs (playbook, walkthrough, execution modes), fixture/scenario
scaffold.

**Risks identified then (mostly resolved in v1 RC, M01–M06):**

1. **Docs ahead of Rust** — agents could hit unimplemented commands → **fixed:** [AGENT-READINESS.md](./AGENT-READINESS.md)
2. **Model drift** — steps location, `planning_phase`, `[execution]` prefs → **fixed:** ADR-001–003, Rust model aligned
3. **Legacy surface** — `instructions.md` / Bash CLI → **fixed:** [LEGACY.md](./LEGACY.md) banner
4. **Path engine** — naive `next-step` → **fixed:** P1.8 path engine ships in v1 RC

**Verdict at audit:** Planning quality was **high**; Rust P1 alignment was the blocker.
**Today:** v1 + v0.2 shipped (M01–M10 done) — active work is v0.3 (M11–M15). Remaining gaps are P4/P5 backlog items (B-17–B-28).

---

## 2. What’s working well

| Area | Why |
|------|-----|
| **Agent contract** | AGENTS-TEMPLATE + skill + AGENT-PLAYBOOK — clear zones, state updates |
| **PM story** | Funnel (idea/track/backlog/milestone), cadences, handoff |
| **Gates G1–G10** | Spec-before-code is enforceable and documented |
| **IDS / outline notation** | S3.1 splits, no renumbering — good for agents |
| **Brownfield (P4)** | Delta + domain specs — forward-looking, optional until needed |
| **Testing strategy** | Fixtures + scenarios + walkthrough-oauth |
| **Storage decision** | JSON-canonical on disk and at CLI — documented in STORAGE.md |

---

## 3. Critical design issues (historical — pre-v1 audit)

> **Historical (pre-v1).** Most items below were resolved in M01–M06 (v1 RC). Preserved for audit trail.

### 3.1 Steps: flat `[[steps]]` vs `work_packages[].steps`

| Source | Says |
|--------|------|
| [IDS.md](./IDS.md), `milestone.schema.json` | Top-level `[[steps]]` canonical |
| `crates/mp` `MilestoneFile` | Steps only under `work_packages[].steps` |
| Fixtures | Use `[[work_packages.steps]]` for Rust |

**Risk:** P1 implements wrong shape; migration pain later.

**Recommendation:** Pick one canonical on-disk shape in IDS.md. Either:

- **A)** Migrate Rust to top-level `steps` + optional `work_package` field on step (preferred per docs), or  
- **B)** Document nested steps as canonical until v2 and update IDS/schema.

Do **not** ship P1 without resolving this.

### 3.2 `planning_status` vs `planning_phase`

Both on `plan.json`; overlapping meaning for agents.

| Field | Values | Purpose |
|-------|--------|---------|
| `planning_status` | planning, ready-for-execution, in-execution, release-candidate | Delivery maturity |
| `planning_phase` | brief, charter, milestones, execution | Pipeline stage |

**Risk:** Agents confuse “are we in execution?” — status says `in-execution`, phase says `milestones`.

**Recommendation:** Add a **matrix doc** (or SPEC table): which command updates which field.
Consider merging into one enum + `pipeline_stage` later (P5).

**Rust gap:** `ProjectMeta` has `planning_status` only — no `planning_phase`.

### 3.3 `[execution]` block location

`plan.json` template has `[execution]` (mode, strategy, pins).  
`PlanFile` in Rust ignores it (serde drops unknown keys).  
`config.json` has `[next] prefer`.

**Recommendation:** Document single owner:

- `plan.json [execution]` — strategy, mode, handoff, adoption_order  
- `config.json` — output, archive, git, display prefs  

Parse `[execution]` in Rust when implementing P1.8 / P1.9.

### 3.4 `mp init` ≠ MP-COMMANDS spec

| MP-COMMANDS says init creates | Rust init creates |
|-------------------------------|-------------------|
| `brief.json`, `ideas.json` | **No** — only plan, backlog, decisions, tracks, AGENTS |

**Risk:** Agents run `mp brief todo` on fresh init → file missing.

**Recommendation:** P0.5 = add `brief.json` to init **or** downgrade MP-COMMANDS until implemented.

### 3.5 Validate enforcement gaps

| Gate | Documented | Rust |
|------|------------|------|
| G1 | ✓ | ✓ |
| G2–G4 | ✓ | partial |
| G5 | ✓ | partial (work_packages only) |
| G6–G7 | ✓ | ✗ |
| G8 deps exist + done | ✓ | **deps existence not checked** |
| G9 step deps | ✓ | ✗ |
| G10 AC coverage | ✓ | ✗ |
| G11–G13 delta | ✓ | ✗ |

**Risk:** `execution_ready` and handoff trust validate — false confidence today.

### 3.6 `next-step` ≠ execution path spec

Current Rust: first pending step in **filesystem iteration order** over milestones.

Missing vs [EXECUTION-PATH.md](./EXECUTION-PATH.md):

- Topological sort on `depends_on`
- `priority`, `adoption_order`, `focus`
- Prefer `in-progress` milestone (resume)
- Step `depends_on_steps`
- AC coverage prioritization

**Risk:** Autonomous loop runs wrong step — **block handoff in Rust until P1.8 minimum**.

---

## 4. Documentation gaps (historical — pre-v1 audit)

> Remediation index in §0 tracks what shipped; [AGENT-READINESS.md](./AGENT-READINESS.md) is current runtime truth.

### 4.1 Legacy confusion

| File | Issue |
|------|-------|
| [instructions.md](../instructions.md) | Markdown milestones, STATUS.md, `master-plan` bash — **contradicts** current model |
| [README.md](../README.md) §2 | Large legacy CLI section — easy to misread |
| [PLANNING-STATUS.md](./PLANNING-STATUS.md) | Lists `track promote` as implemented — **CLI has no promote** |

**Recommendation:**

- Add banner to `instructions.md`: superseded, link AGENTS-TEMPLATE  
- Move legacy README to `docs/LEGACY.md` or collapse to one paragraph  
- Fix PLANNING-STATUS command list

### 4.2 Missing schemas / broken references

| Referenced | Exists? |
|------------|---------|
| `schemas/plan.schema.json` in SPEC §2 | **No** |
| JSON Schema validation on CLI write | **Not implemented** |
| `mp sync` (rebuild index) | **Implemented** (v1 RC) |

### 4.3 SPEC still “Draft”

SPEC.md header says draft — understates maturity. Update status or add “normative sections” list.

### 4.4 Open decisions undertreated

SPEC §18 has 3 rows. Missing recorded decisions:

- Steps flat vs nested  
- `planning_phase` vs `planning_status`  
- Emergency/hotfix bypass policy  
- Schema validate on write: required or optional?

**Recommendation:** Expand §18 or add `docs/DECISIONS.md` (ADRs).

### 4.5 Agent doc ↔ command parity

[AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md) references:

- `mp milestone criterion fail` — **not in MP-COMMANDS**
- `mp milestone defer` — alias of block, partial spec

Mark unimplemented commands in playbook with phase tags (like AGENTS-TEMPLATE does for ideas).

---

## 5. Situations not yet covered (historical + v0.2 deferrals)

> **Resolved in v1 + v0.2:** reopen, step fail, skip, track promote, brief reopen, delta rebase, export/digest, git commit, JSON schema on write, session path, concurrency, `mp note`.  
> **Active:** v0.3 milestones M11–M15 · **Parked:** backlog B-17–B-28 in [PLANNING-STATUS §14](./PLANNING-STATUS.md#14-v03-roadmap).

### 5.1 Execution & delivery

| Situation | Gap |
|-----------|-----|
| **Reopen milestone** after complete | No `milestone reopen` — archive/restore only |
| **Partial ship** (M3 phase 1 of 2) | Milestone split documented; no “phase” field |
| **Step failed** (tests red) | No `step fail` / rollback workflow |
| **Skip step** with approval | `skipped` status in schema; command thin |
| **Hotfix bypass** (prod down, skip spec) | Undocumented — need explicit emergency lane or “never” |
| **Parallel agents** two sessions | No lock, no `updated_by` — race on JSON writes |
| **Wrong next-step** taken | No `step claim` / WIP owner |

### 5.2 Planning & grooming

| Situation | Gap |
|-----------|-----|
| **Duplicate intake** | Same idea twice — `idea dup-check` proposed P1.6 only |
| **Milestone ID collision** | No validate for duplicate `03-*.json` |
| **Orphan milestone file** | Not in plan index — W01 warning only partial |
| **Charter drift** | Goals in plan.json vs milestone specs — no `plan gaps` at project level |
| **Promote track → milestone** | Documented, **not in CLI** |
| **Brief reopen** | Mentioned in MP-COMMANDS, not spec’d in workflows |

### 5.3 Brownfield & domain (P4)

| Situation | Gap |
|-----------|-----|
| **Delta rebase** after domain bumped | G13 documented; no `mp delta rebase` flow |
| **Seed domain from code** | `brownfield scan` assist only — no import |
| **Cross-milestone domain** | One milestone touches api + auth — multi-domain delta? |

### 5.4 Ops & integration

| Situation | Gap |
|-----------|-----|
| **CI: validate on PR** | Mentioned in testing; no `docs/CI.md` recipe |
| **Plan-only git commits** | P3 `mp git commit` — not implemented |
| **Multi-package monorepo** | One plan vs plan per crate — undecided |
| **Secrets in evidence** | AC evidence free text — no redaction guidance |
| **Toolkit version mismatch** | doctor doesn’t compare schema version to CLI |

### 5.5 Human / PM

| Situation | Gap |
|-----------|-----|
| **Stakeholder read-only** | export/digest P3 — not built |
| **Capacity / sprint** | P5 iteration — not planned in detail |
| **External blocker** | `blocks_external` P5 — vendor wait |
| **Meeting notes → plan** | P3.3 `mp note` — not spec’d |

---

## 6. Minor design nits

| Topic | Note |
|-------|------|
| `implemented` vs `verified` spec_status | Thin line — document when to use `implemented` (all steps done, ACs pending) |
| `execution_status: done` vs `spec_status: verified` | `complete` sets both — avoid manual mismatch |
| Challenge `F-01` vs open `Q-01` | Overlap — when to use which |
| Many intake lanes | Consider decision flowchart in AGENTS (single page) |
| `behavior.scenarios` shape | `[[behavior.scenarios]]` vs nested — fixture used flat array key; template nested — verified both parse the same via the `Behavior` struct (field since dropped in M82; on-disk format is now JSON) |
| Track `steps` as `Vec<String>` | Not structured Step objects — inconsistent with milestones |

---

## 7. Recommended enhancements (prioritized)

### P0 — Before agents rely on plan in production

1. Resolve **steps storage model** (§3.1)  
2. Fix **init vs brief.json** (§3.4)  
3. **Tag unimplemented commands** in playbook + AGENTS (phase column)  
4. Deprecate **instructions.md** visibly  
5. Fix **PLANNING-STATUS** rust command list (remove `track promote` until built)

### P1 — With milestone lifecycle Rust

6. Implement **G8** (dep exists), **G2**, **G5** fully  
7. **Milestone CRUD** + step lifecycle per AGENT-PLAYBOOK  
8. **`planning_phase`** in model + updates from `brief done`  
9. Parse **`[execution].mode`** for handoff gate

### P1.8 — Before autonomous handoff in production

10. **Path engine** replaces naive `next-step`  
11. **G10** AC coverage in validate  
12. **`execution check`** uses real `execution_ready`

### P1.9 — PM surface

13. `inbox`, `block`/`unblock`, `handoff`/`pause`  
14. **`track promote`** or remove from docs

### P3+ — Maturity

15. `plan.schema.json` + validate JSON on write  
16. `mp sync`, CI doc, `docs/DECISIONS.md`  
17. Emergency/hotfix policy (track-only vs explicit waiver gate)  
18. Multi-agent: `updated_at` + optimistic concurrency or lock file

---

## 8. Suggested new docs (optional)

| Doc | Purpose |
|-----|---------|
| `docs/DECISIONS.md` | ADRs for resolved tensions (steps shape, phase fields) |
| `docs/CI.md` | `mp validate` in GitHub Actions, fixture tests |
| `docs/LEGACY.md` | Move bash + markdown workflow from README |
| `docs/EMERGENCY.md` | Hotfix policy — or “tracks only, never skip gates on milestones” |

---

## 9. Agent instructions — covered?

| Topic | Covered? | Where |
|-------|----------|-------|
| When to use `mp` | ✓ | AGENT-PLAYBOOK §2 |
| Start / finish step | ✓ | AGENT-PLAYBOOK §4 |
| Block / resume | ✓ | AGENT-PLAYBOOK §4.3–4.4 |
| Complete milestone | ✓ | AGENT-PLAYBOOK §4.2 |
| Track lifecycle | ✓ | AGENTS §3.7 |
| Autonomous loop | ✓ | EXECUTION-MODES, WALKTHROUGH |
| **Runtime command matrix** | ✓ | [AGENT-READINESS.md](./AGENT-READINESS.md) — sole source for “call or not” |
| **Deferred v0.2 items** | ✓ | M07–M10 done; v0.3 in [PLANNING-STATUS §14](./PLANNING-STATUS.md#14-v03-roadmap) |

**Verdict (2026-06-18):** Agent instructions are **adequate for v1 RC**. Use AGENT-READINESS
before calling commands; treat sections below as historical audit notes unless marked open.

---

## 10. References

- [PLANNING-STATUS.md](./PLANNING-STATUS.md)
- [SPEC.md §18 Open Decisions](./SPEC.md#18-open-decisions)
- [TESTING.md](./TESTING.md)
- [WALKTHROUGH.md](./WALKTHROUGH.md)
