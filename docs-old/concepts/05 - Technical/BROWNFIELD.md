# Greenfield & Brownfield

How Master Plan distinguishes **new capability** from **changes to existing behavior**,
what to use before P4 ships, and the planned **delta spec** model.

See also [SPEC.md](./SPEC.md), [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md), [PLANNING-STATUS.md](./PLANNING-STATUS.md).

**Status:** Implemented (v1 RC) — `specs *`, `brownfield scan`, delta merge on `milestone complete`, `delta rebase`.

---

## 1. Definitions

| Term | Meaning | Example |
|------|---------|---------|
| **Greenfield** | Net-new capability or subsystem. No prior “system truth” to diff against. | “Add OAuth login”, “Ship v1 API” |
| **Brownfield** | Change, extend, fix, or remove **existing** behavior. | “Rate-limit API”, “Fix symlink scanner” |
| **Delta spec** | Brownfield milestone that documents only **what changes** (ADDED / MODIFIED / REMOVED). | P4 model |
| **Domain spec** | Long-lived truth file under `master-plan/specs/` for one bounded area. | `specs/api.json` |
| **Track item** | Small brownfield work without a full milestone. | `BF-01` bugfix |

Most real projects are brownfield after week one. Greenfield is the exception (new
product areas, greenfield repos, major new subsystems).

**Adoption profile** ([ADOPTION-PROFILES.md](./ADOPTION-PROFILES.md)) changes bootstrap depth:
`mp init --profile hybrid --from-repo` proposes minimal plan + tracks; `full` drafts charter
from README. Profile is per-project in `config.json`, not global.

---

## 2. Routing: what to use when

```text
                    User intent
                         │
         ┌───────────────┼───────────────┐
         ▼               ▼               ▼
    Tiny fix        Medium change     Large / epic
    polish          behavior change   new subsystem
         │               │               │
         ▼               ▼               ▼
    Track item      Delta milestone   Greenfield OR
    bugfix/tweak    (P4) OR           delta milestone
                    greenfield        (if extending
                    milestone         existing domain)
                    (today)
```

| Situation | Route | `change_kind` |
|-----------|-------|---------------|
| One-file bug, typo, config tweak | `mp track add bugfix` / `tweak` | — (tracks) |
| Small behavior change, few ACs | Track, or greenfield milestone today | `greenfield` (default) |
| Changes existing API/auth/billing rules | **Delta milestone** (P4) | `delta` |
| New feature with no prior domain spec | Greenfield milestone | `greenfield` |
| New feature that extends documented domain | Delta milestone (P4) | `delta` |
| “We might build this someday” | Idea or backlog | — |

**Promotion:** `mp track promote bugfix BF-03 --to-milestone` when a track item outgrows
the track lane.

**Doctor hint:** `mp doctor` may set `detected.brownfield_likely: true` when the project
root looks like an existing app (`src/`, tests, manifests). That suggests delta routing
for behavior changes — not a hard rule.

---

## 3. Today (pre-P4) vs P4

### What works today

| Mechanism | Brownfield support |
|-----------|-------------------|
| **Tracks** | Primary lane for small fixes and tweaks |
| **Greenfield milestones** | Full spec works; agent uses **code zone** search to learn current behavior during interview |
| **`context.references`** | Link to source files, docs, prior milestones |
| **`change_kind`** | Field exists in schema (default `greenfield`); no delta merge yet |

### What P4 adds

| Mechanism | Purpose |
|-----------|---------|
| `master-plan/specs/*.json` | Domain truth — requirements, scenarios, interfaces |
| `change_kind: delta` | Milestone carries ADDED / MODIFIED / REMOVED sections |
| `mp specs show/list` | Read domain truth |
| `mp brownfield scan` | Assist interview — codebase signals, not auto-spec |
| **Merge on complete** | Delta applied to domain file; milestone archived as history |

Until P4: treat brownfield milestones like greenfield for **workflow**, but write specs
that explicitly describe **before → after** in `problem`, `behavior`, and ACs. Link
evidence in `context.references`.

---

## 4. Two zones (plan vs code)

Agents work in two zones. This split applies to **all** change kinds.

```text
┌─────────────────────────────┐     ┌─────────────────────────────┐
│  PLAN ZONE                  │     │  CODE ZONE                  │
│  master-plan/               │     │  Application repo           │
│                             │     │  src/, tests/, configs, …   │
│  mp show, list, path, …     │     │  ripgrep, read file, LSP,   │
│  mp milestone create, …     │     │  optional harness search    │
│  Never hand-edit JSON       │     │  (fff MCP, etc.)            │
└─────────────────────────────┘     └─────────────────────────────┘
         │                                       │
         │         findings feed interview        │
         └───────────────────┬───────────────────┘
                             ▼
                   mp milestone create --json @-
                   (structured fields only)
```

| Zone | Allowed | Forbidden |
|------|---------|-----------|
| **Plan** | All `mp *` commands | Grep/edit `master-plan/` directly |
| **Code** | Read/search/implement app source | Writing plan state without `mp` |

Code zone discoveries become **spec input** (references, scenarios, AC verification),
not ad-hoc plan file edits.

---

## 5. Domain specs (`master-plan/specs/`)

**P4.** One file per bounded domain — not one file per milestone.

```text
master-plan/
├── specs/
│   ├── api.json          # HTTP API behavior
│   ├── auth.json         # Authentication & sessions
│   └── billing.json      # Payments, plans
├── milestones/
│   └── 04-rate-limit.json   # delta → merges into api.json
```

Template: [templates/defaults/spec-domain.json](../templates/defaults/spec-domain.json)  
Schema: [schemas/spec-domain.schema.json](../schemas/spec-domain.schema.json)

Domain files hold **current truth**:

- Requirements (`REQ-XX`)
- Scenarios (`SC-XX`)
- Interface (endpoints, config keys, CLI)
- Version counter (incremented on each merge)

Milestones are **events**; domain specs are **state**.

---

## 6. Delta milestones (P4)

### `change_kind` on milestone

```json
[milestone]
id = "04"
title = "API rate limiting"
change_kind = "delta"    # greenfield | delta (default: greenfield)

[delta]
domain = "api"           # specs/api.json
base_version = 2         # domain version this delta applies to
```

### Delta sections (OpenSpec-style)

| Section | Meaning |
|---------|---------|
| **ADDED** | New requirements, scenarios, endpoints |
| **MODIFIED** | Behavior change — `before` and `after` required |
| **REMOVED** | Deprecated requirements with `reason` and optional `replacement` |

Example (conceptual JSON):

```json
[[delta.added]]
id = "REQ-12"
statement = "Per-IP rate limit on /api/*"
scenarios = ["SC-10"]

[[delta.modified]]
target = "REQ-03"
before = "No request throttling"
after = "tower::limit middleware returns 429 after 100 req/min"

[[delta.removed]]
target = "REQ-99"
reason = "Unauthenticated bulk export removed for abuse prevention"
replacement = "REQ-12"
```

### Lifecycle

```text
1. mp specs show api     # read current truth
2. mp brownfield scan --domain api     # optional assist (P4)
3. Interview user — before/after, scope
4. mp milestone create (change_kind=delta, delta sections)
5. Approve → decompose → implement (same as greenfield)
6. mp milestone complete 04
   → merge delta into specs/api.json
   → bump domain version
   → archive milestone (history preserved)
```

### Gates (delta-specific, planned)

| Gate | Rule |
|------|------|
| **G11** | `change_kind: delta` requires `delta.domain` and existing domain file |
| **G12** | Every MODIFIED/REMOVED `target` must exist in domain at `base_version` |
| **G13** | Merge conflicts (domain changed since `base_version`) block complete until rebase |

Greenfield milestones use the same G1–G10 gates; no domain file required.

---

## 7. `mp brownfield scan` (P4)

**Assist only** — output is suggestions for the interview, not written to disk.

```bash
mp brownfield scan --domain api
mp brownfield scan --query "rate limit middleware"
```

**Signals (illustrative):**

| Signal | Example |
|--------|---------|
| Entry points | `src/api/router.rs`, middleware chain |
| Related tests | `tests/api_*.rs` or gap: none |
| Config | `RATE_LIMIT` in `.env.example` |
| Docs drift | README claims limits; code does not |
| Dependencies | Existing `tower` / limit crates |

Implementation note: start with **targeted ripgrep/git queries**, not a persistent
search index. Optional harness tools (fff, LSP) may run in the agent session; `mp`
returns structured JSON when the command ships.

---

## 8. Interview: brownfield today

Before P4 delta merge, use this flow for behavior changes:

```text
1. mp doctor              # brownfield_likely?
2. Code zone: find current behavior      # ripgrep, read files
3. mp interview checklist --type milestone
4. Capture explicitly:
   - What exists today (problem.context)
   - What changes (intent.outcome, behavior.scenarios)
   - What stays the same (scope.out_of_scope)
   - Evidence paths (context.references)
5. mp milestone create --json @-
6. Approve → decompose → implement
```

**Interview prompts (brownfield):**

- What is the current behavior? How do we know? (file/test reference)
- What must change? What must **not** change?
- Who relies on old behavior? Migration or flag?
- How do we verify before and after?

---

## 9. `mp doctor` and project shape

Doctor helps bootstrap and routing. Full spec: [MP-COMMANDS.md § doctor](../06 - Reference/MP-COMMANDS.md#mp-doctor).

| Detection | Suggests |
|-----------|----------|
| No `master-plan/` | `mp init` |
| `src/` + tests + manifest | `brownfield_likely: true` |
| `Cargo.toml` / `package.json` | Prefill charter `stack` |
| Validate errors | Run `mp validate` before planning |

Doctor does **not** scan the whole repo for semantics — only lightweight heuristics.

---

## 10. Examples

### Greenfield milestone

New subsystem, no `specs/` entry yet:

> “Add user notifications (email + in-app) — we have no notification system today.”

- `change_kind: greenfield` (default)
- Full intent, scenarios, ACs
- On complete (P4 optional): may **seed** new `specs/notifications.json`

### Brownfield via track (today)

> “Fix vault scanner skipping symlinks”

- `mp track add bugfix --title "..." --problem "..." --verification "cargo test ..."`
- No delta spec needed

### Brownfield via delta milestone (P4)

> “Add rate limiting to existing API”

- `change_kind: delta`, `delta.domain: api`
- MODIFIED middleware behavior, ADDED 429 responses
- Merge updates `specs/api.json`

---

## 11. Implementation phase

| Phase | Deliverable |
|-------|-------------|
| **Now** | This doc, skill + AGENTS routing, doctor spec, templates/schemas |
| **P4** | `specs/` CRUD, delta fields, merge on complete, `brownfield scan`, G11–G13 |
| **P4+** | `mp specs diff`, rebase delta on domain conflict |

---

## 12. References

- [SPEC.md §17](./SPEC.md#17-brownfield-and-delta-specs-future--p4) — summary in core spec
- [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) — `doctor`, `brownfield scan`, `specs *`
- [templates/AGENTS-TEMPLATE.md](../templates/AGENTS-TEMPLATE.md) — agent workflows
- [templates/skills/master-planner/SKILL.md](../templates/skills/master-planner/SKILL.md) — harness skill
