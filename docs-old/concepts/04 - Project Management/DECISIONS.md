# Architecture Decisions (ADRs)

Recorded decisions for Master Plan. Supersedes informal notes in [SPEC.md §18](./SPEC.md#18-open-decisions).

| ID | Decision | Status |
|----|----------|--------|
| [ADR-001](#adr-001-steps-on-disk) | Steps: top-level `[[steps]]` | **Accepted** |
| [ADR-002](#adr-002-planning_status-vs-planning_phase) | Two fields, distinct roles | **Accepted** |
| [ADR-003](#adr-003-execution-config-split) | `plan.json` vs `config.json` | **Accepted** |
| [ADR-004](#adr-004-json-schema-on-write) | Validate writes in P3 | **Accepted** |
| [ADR-005](#adr-005-emergency--hotfix-policy) | Tracks only, no gate bypass | **Accepted** |
| [ADR-006](#adr-006-autonomous-handoff-gate) | Handoff requires P1.8 path | **Accepted** |
| [ADR-007](#adr-007-concurrency) | Single writer; optimistic `updated_at` later | **Accepted** |
| [ADR-008](#adr-008-adoption-profiles-in-project-config) | Workflow profile in `config.json` | **Accepted** |
| [ADR-010](#adr-010-session-on-disk-layout) | `sessions/<id>/` tree | **Accepted** |
| [ADR-011](#adr-011-one-session-per-branch) | One active session per branch | **Accepted** |
| [ADR-012](#adr-012-monorepo-plan-layout) | Single root `master-plan/` | **Accepted** |
| [ADR-013](#adr-013-storage-backend--files-not-a-database) | Files over embedded DB | **Accepted** |
| [ADR-014](#adr-014-json-canonical-plan-persistence) | JSON on disk (supersedes TOML-on-disk) | **Accepted** |

---

## ADR-001: Steps on disk

**Context:** Docs said top-level `[[steps]]`; Rust read `work_packages[].steps` only.

**Decision:** Canonical on-disk shape is **top-level `[[steps]]`** with optional
`steps[].work_package = "WP1"`. Work packages remain grouping metadata only.

**Transitional (until P1 Rust migrates):**

- `mp` **reads** both shapes (merge into one step list).
- `mp` **writes** top-level `[[steps]]` only.
- Test fixtures may use `[[work_packages.steps]]` until updated.

**Consequences:** P1 Rust must update `MilestoneFile` and `next-step` builder.

See [IDS.md §3](./IDS.md#3-steps-s).

---

## ADR-002: planning_status vs planning_phase

**Context:** Two fields on `plan.json` looked redundant.

**Decision:** Keep **both** — different concerns:

| Field | Question it answers |
|-------|---------------------|
| `planning_status` | How mature is delivery? (`planning` → `release-candidate`) |
| `planning_phase` | Which bootstrap pipeline stage? (`brief` → `execution`) |

**Matrix:** [SPEC.md §4.7](./SPEC.md#47-planning_status-vs-planning_phase-matrix)

**Consequences:** P1 adds `planning_phase` to Rust `ProjectMeta`; commands update it
(`brief done` → `charter`, first `milestone approve` → `milestones`, `handoff` → `execution`).

---

## ADR-003: Execution config split

**Decision:**

| File | Contents |
|------|----------|
| `plan.json [execution]` | `mode`, `handoff_*`, `strategy`, `interleave`, `focus_*`, `adoption_order` |
| `config.json` | `[output]`, `[archive]`, `[next].prefer`, `[git]`, `[planning]` gate strictness |

**Consequences:** P1.8 parses `[execution]` from `plan.json` into Rust model.

---

## ADR-004: JSON Schema on write

**Decision:** CLI writes validated against schemas **starting P3** (optional warn in P1).

Until then: serde + gate validation only.

---

## ADR-005: Emergency / hotfix policy

**Decision:** **No milestone gate bypass.** Production emergencies use **tracks** (`bugfix`)
or explicit human waiver documented in conversation — not `spec_status` shortcuts.

See [EMERGENCY.md](./EMERGENCY.md).

---

## ADR-006: Autonomous handoff gate

**Decision:** `mp execution handoff` **must refuse** (or warn loudly) when `next-step` does
not use P1.8 path engine (G8 deps + ordering). Documented in [EXECUTION-MODES.md](./EXECUTION-MODES.md).

---

## ADR-007: Concurrency

**Decision:** One planning writer per project (single agent session). P5: `updated_at` +
conflict detection on milestone files. No file locks in P1.

See [EDGE-CASES.md](./EDGE-CASES.md#multi-agent--concurrency).

---

## ADR-008: Adoption profiles in project config

**Context:** Users adopt Master Plan at different intensities — full backlog on personal
projects, scoped branch work on work repos, tracks/ideas for quick fixes. This is not
the same as `planning_phase` (pipeline state) or `[execution].mode` (implement now?).

**Decision:**

| File | Contents |
|------|----------|
| `config.json [workflow]` | `profile` (`full` \| `session` \| `hybrid`), artifact toggles, plan location/git policy, gate strictness, session prefs |
| `plan.json` | Unchanged — `planning_phase`, `planning_status`, `[execution]` strategy/mode |
| Global `~/.agents/config.json` | Toolkit defaults only; optional `[init].default_profile` for new projects |

**Profiles:**

- **`full`** — brief, backlog, milestones, tracks, ideas; plan in repo by default.
- **`session`** — one scoped unit (branch/PR); disposable plan; gitignored by default.
- **`hybrid`** — tracks + ideas always on; milestones session-scoped; work-repo default.

**Consequences:** P3.1 implements `[workflow]` parsing, `mp init --profile`, `mp session *`.
Agents read `mp config show` at session start. See [ADOPTION-PROFILES.md](./ADOPTION-PROFILES.md).

---

## ADR-009: v1 harness targets (Cursor + OpenCode)

**Context:** Master Plan needs a first-class install story for agent harnesses. The toolkit
already uses `~/.agents/` (skill + `MP_HOME`), which aligns with OpenCode’s native skill
discovery. Cursor uses `~/.cursor/skills/` separately.

**Decision:**

| Harness | v1 support | Skill location |
|---------|------------|----------------|
| **OpenCode** | Yes | `~/.agents/skills/master-planner/` (no extra config) |
| **Cursor** | Yes | Mirror to `~/.cursor/skills/master-planner/` |
| Others | Document only | Use manual copy of skill + `mp` on PATH |

**Installer (`mp install`, P0.9):**

- Default: install both harnesses (`--harness all`).
- Canonical skill source: `templates/skills/master-planner/SKILL.md`.
- Optional project symlinks: `.cursor/skills/`, `.opencode/skills/` via `mp init --with-*-skill` (P3.1).

**Consequences:** `mp doctor` gains harness checks; [INSTALL.md](./INSTALL.md) is the user guide.
Slash `/mp` is **not** required for v1 — skill `description` triggers discovery.

---

## ADR-010: Session on-disk layout

**Context:** Session commands shipped without a canonical on-disk layout ([D-002](../master-plan/decisions.json)).

**Decision:** **`master-plan/sessions/<id>/`** tree with `session.json` and session-scoped milestone file(s). Reject `scope=session` on flat milestone files for v0.2.

**Consequences:** M08 implements directory creation in `mp session start`, archive moves to `archive/sessions/`.

---

## ADR-011: One session per branch

**Context:** Multiple concurrent sessions vs branch binding ([D-003](../master-plan/decisions.json)).

**Decision:** **At most one active session per git branch** when `workflow.session.auto_bind_branch = true` (default). No `session focus` command in v0.2.

**Consequences:** `mp session start` on a branch with an active session resumes or errors with clear message.

---

## ADR-012: Monorepo plan layout

**Context:** One plan vs per-crate plans ([D-004](../master-plan/decisions.json)).

**Decision:** **Single root `master-plan/`** per repository. Per-crate plan dirs deferred.

**Consequences:** M09 documents monorepo guidance; no multi-plan discovery in v0.2.

---

## ADR-013: Storage backend — files, not a database

**Context:** Evaluated migrating the plan store from on-disk files to an embedded SQL
database — SQLite (via `rusqlite`) or Turso — for performance and reduced coding burden.
Two real pain points motivated the review: (1) writes were not crash-atomic (`store.rs`
used bare `fs::write`), and (2) a known multi-agent write race exists
([ADR-007](#adr-007-concurrency), DESIGN-REVIEW §5.1).

> **Note (M92):** this ADR decided *files, not a database*. The on-disk *serialization
> format* was subsequently changed from TOML to JSON by [ADR-014](#adr-014-json-canonical-plan-persistence).
> Read "files" below as JSON-on-disk files.

**Decision:** **Keep on-disk files as the canonical store.** Reject SQLite and Turso as the
source of truth.

Rationale:

- **Scale is far below the crossover.** ~244 KB / 22 milestones per repo; the hottest paths
  (`mp path`, `validate`, `graph`) load everything into RAM in <10 ms. A DB buys no
  measurable performance here and adds a schema/migration layer — more code, not less.
- **Files are the product's identity.** JSON-on-disk is git-diffable, PR-reviewable, and
  hand-readable ([STORAGE.md](./STORAGE.md), [ADR-012](#adr-012-monorepo-plan-layout)).
  A binary DB breaks this loop — the value prop is *"your master plan, structured for
  agents, readable for humans."*
- **Dependency posture.** M21/M22 just shipped a dependency diet (≤80 transitive crates).
  An embedded DB (libsqlite3, or Turso's crate tree) swims against an explicit
  architectural goal.
- **Turso-specific risk.** The `tursodatabase/turso` repo is an in-process Rust rewrite of
  SQLite, still **BETA** (v0.6.x; vendor FAQ says "not production ready") and **async-only**,
  which would force a rewrite of the entire synchronous `store.rs` surface. Turso *Cloud*
  (libSQL, networked) is a separate, larger departure that breaks the local-first assumption.

**Alternatives considered:**

| Option | Verdict |
|--------|---------|
| SQLite (`rusqlite`, embedded) | Battle-tested; would solve atomicity/concurrency, but breaks git/file reviewability and conflicts with the dep diet. Justified only at ≥1000s of records or multi-project aggregation. |
| Turso (embedded repo) | Same drawbacks as SQLite, plus BETA maturity and an async refactor of all storage code. Weakest of the three. |
| Derived DB cache alongside canonical files | Premature at current scale; revisit only if `mp sync`-style queries get slow. |

**Consequences:**

- The **crash-atomicity** gap is closed without a DB: `store::atomic_write`
  (temp-file-then-rename) now guards every write; a crash leaves the previous file intact.
- The **multi-agent concurrency** gap remains open ([ADR-007](#adr-007-concurrency)'s
  deferred P5). Cheapest fix is extending `--if-updated` to all writes or a `.lock` file —
  not a DB.
- **Revisit when:** (a) a plan grows to thousands of milestones, or (b) a multi-project
  aggregation feature is wanted (currently forbidden by ADR-012). Until then, files win on
  every axis that matters.

See [STORAGE.md](./STORAGE.md), [ADR-007](#adr-007-concurrency),
[ADR-012](#adr-012-monorepo-plan-layout), [DESIGN-REVIEW.md §5.1](./DESIGN-REVIEW.md).

---

## ADR-014: JSON-canonical plan persistence (supersedes TOML-on-disk)

**Status:** Accepted — milestone M92.

**Context:** The prior decision (implicit in [STORAGE.md](./STORAGE.md) and the original
on-disk format row of [SPEC.md §13.7](./SPEC.md)) was **TOML on disk, JSON at the CLI
boundary**: `mp::store` deserialized `*.toml` files to Rust structs via the `toml` crate,
then re-serialized them to JSON for agent I/O. This created two serialization surfaces
(TOML at rest, JSON at the boundary) that had to be kept in sync, plus a re-encoding hop on
every read. The CLI `--format toml` debug mode dumped the raw TOML.

The product is now **agent-first**: humans interact through **raul** (TUI) or a prose
summary, never by hand-editing plan files, and hand-editing was already forbidden by rule.
JSON Schema already validates agent I/O at the boundary, so the same schema could validate
the at-rest bytes if disk and boundary used the same format.

**Decision:** Drop TOML on disk. **JSON is now canonical at rest and at the CLI boundary —
one `serde_json` path through `mp::store`.** Plan artifacts are `.json` on disk:

`milestones/{id}-{slug}.json`, `plan.json`, `config.json`, `brief.json`, `ideas.json`,
`backlog.json`, `decisions.json`, `annotations.json`, `reviews.json`, `tracks/{kind}.json`,
`archive/meta.json`, `specs/{domain}.json`, `reviews/challenges/*.json`,
`sessions/{id}/session.json` + `sessions/{id}/milestone.json`.

Rationale:

- **Agent-first product.** Agents read and write the exact bytes `mp` persists — no
  re-encoding, no second format to drift. JSON is the native interchange for every harness.
- **Humans use raul.** Hand-editing was already forbidden; the TOML human-readability
  advantage (comments, multiline blocks) is unused. Humans get readable views from raul,
  not raw files.
- **One validation surface.** The JSON Schema that validates CLI I/O now also validates
  at-rest content. Persist what you serve.
- **Simpler store layer.** `mp::store` reads/writes JSON via `serde_json`; the `toml`
  crate is no longer on the plan-artifact path (it remains a dependency only for
  `Cargo.toml` parsing in `mp::install` and the one-time migrate module).

**Consequences:**

- **`--format`** values change: `json` is default; `toml` is **gone**; the new debug mode is
  **`--format raw`** = verbatim on-disk JSON passthrough for `show milestone`/`track show`,
  and GraphViz DOT for `graph`.
- **`mp milestone create/update --file`** now requires `.json` (TOML input dropped).
- **`mp show milestone <id>`** default JSON now serializes the loaded `MilestoneFile` struct
  directly (same path as write), not a separate hand-built "lean" view.
- **`mp init`** scaffolds `.json` plan artifacts.
- **One-time conversion:** `mp::migrate` (M92) reads every `*.toml` under a plan dir,
  converts each to equivalent `*.json`, and removes the original. Because the on-disk
  structs no longer carry M82's dropped ceremony fields (`behavior`, `context`,
  `requirements`, `interface`, `technical_context`, `success_criteria`, `assumptions`,
  `risks`, `follow_ups`), deserializing legacy TOML and re-serializing as JSON drops those
  fields for free — they are not reintroduced.
- **Frozen rollback reference:** a pre-M92 TOML snapshot is preserved at the repo-root
  `legacy-toml/` directory, outside the plan dirs. It is **never loaded by `mp`** — it
  exists solely as a rollback/diff reference.
- **Supersedes** the prior "TOML on disk" format decision (the format aspect of the
  original STORAGE.md / SPEC §13.7 on-disk format row). It does **not** supersede
  [ADR-013](#adr-013-storage-backend--files-not-a-database): the *files-over-database*
  verdict still holds; only the per-file serialization changed (TOML → JSON).

See [STORAGE.md](./STORAGE.md).

---

## Open (not yet decided)

| Topic | Options | Meta plan |
|-------|---------|-----------|
| Monorepo | One `master-plan/` per repo vs per crate | **D-004** ACCEPTED → M09 |
| Export in git | Default gitignore `exports/` | backlog |
| `mp milestone plan` | Template-only vs AI-assisted fill | **Decided:** template; agent fills |
| Session on disk | `sessions/` tree vs `scope = session` on milestone files | **D-002** ACCEPTED → M08 |
| Multiple active sessions | One per branch vs explicit `session focus` | **D-003** ACCEPTED → M08 |
