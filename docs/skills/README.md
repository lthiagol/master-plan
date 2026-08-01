# Skills

Master Plan ships **skills** — markdown instruction sets that teach an agent how
to behave in a particular role or for a particular technique. Skills are
deployed into agent harness homes (OpenCode, Cursor, Pi) by `mp install` and
discovered by the harness at session start.

## What a skill is

A skill is a directory under `templates/skills/<id>/` containing at least a
`SKILL.md` (the instructions) and a `manifest.json` (the registry entry). Some
skills ship extra files alongside `SKILL.md` (stage tables, scripts, supporting
reference docs) — the whole directory is deployed, so relative links inside
`SKILL.md` resolve.

Each manifest declares:

| Field | Meaning |
|-------|---------|
| `id` | The skill id (used in `mp install --skills …`) |
| `display` | Human label |
| `category` | `core` (ships, deploys by default), `catalog` (opt-in), or `internal` (repo-only; never installable) |
| `consumes[]` | Other skills this one expects to also be loaded |
| `min_mp_version` | Minimum `mp` version |
| `source` / `source_url` / `upstream_version` | For vendored catalog skills |

## Categories

| Category | Deploys by default? | Skills |
|----------|---------------------|--------|
| **`core`** | Yes — the three base CPD skills deploy on a bare `mp install` | `mp-flow`, `mp-runner`, `mp-coordinator` |
| **`catalog`** | No — opt in with `mp install --skills …` | `spec-grill`, `codebase-design`, `diagnosing-bugs` |
| **`internal`** | Never — excluded from the install registry | `mp-code-review` (repo maintainers only) |

## Managing skills

```bash
mp install --list-skills                 # registry + per-harness deployment state
mp install --check                       # validate registry consistency (no deploy)
mp install                               # deploy the 3 base skills to every harness
mp install --skills spec-grill           # add spec-grill (alone)
mp install --skills mp-flow,spec-grill   # explicit set
mp install --harness pi                  # deploy to one harness only
mp install --toolkit-only                # skip skills entirely
```

A freshly installed project gets `mp-flow`, `mp-runner`, and `mp-coordinator`
everywhere. Add `spec-grill` for adversarial spec co-design, and the two
vendored engineering skills on demand.

## The core CPD skills (deploy by default)

These three define the two-agent, four-role architecture that drives a milestone
through its lifecycle. They are the operational layer on top of the `mp` CLI.

### `mp-flow` — cross-role orchestration
The meta-skill. Loads the **12-stage timeline** of a milestone's life and binds
each stage to the role that owns it. Any agent (coordinator or runner) loads
this first; it is the map both roles share.

| Stages 1–4 | Spec authoring (coordinator): Draft, Groom, Specify, Approve |
|------------|--------------------------------------------------------------|
| Stages 5–7 | Execution (runner): Claim & execute, Self-review, Complete |
| Stage 8 | External review (coordinator) |
| Stage 9 | Remediate (runner) |
| Stage 10 | Re-review (coordinator) |
| Stages 11–12 | Document, Hand off (coordinator) |

### `mp-runner` — executing + fixing (runner role)
Owns the execution domain (stages 5–7) and remediation (stage 9). When a session
loads `mp-flow` + `mp-runner`, it is the **runner**: it claims steps, implements,
self-reviews, completes, and fixes findings. *Consumes `mp-flow`.*

### `mp-coordinator` — planning + reviewing (coordinator role)
Owns spec authoring (stages 1–4), the review domain (stages 8, 10), and the
closing stages (11–12). When a session loads `mp-flow` + `mp-coordinator`, it is
the **coordinator**: it drafts and grooms specs, runs spec-grill, performs
external review, and re-reviews remediation. *Consumes `mp-flow` and
`spec-grill`.*

> The split is deliberate: the agent that **executes** a milestone must not be
> the agent that **reviews** it. That independence is what makes the review
> verdict trustworthy.

## The catalog skills (opt-in)

### `spec-grill` — adversarial spec co-design
Wraps `mp interview checklist` + `mp interview gaps` in a structured
multi-round questioning loop. Activate when a milestone idea is vague or the
agent detects weak intent/scope/ACs. Output is a validated milestone spec ready
for `mp milestone create`.

### `codebase-design` — deep-module vocabulary
A shared language for designing **deep modules**: a lot of behavior behind a
small interface, at a clean seam, testable through that interface. Use when
designing or restructuring a module's interface, finding deepening
opportunities, or making code more testable. *Vendored from
`mattpocock/skills` (MIT).*

### `diagnosing-bugs` — disciplined diagnosis loop
A phase-by-phase discipline for hard bugs and performance regressions. The core
insight: build a tight feedback loop first (a pass/fail signal that goes red on
*this* bug); everything else (bisection, hypothesis testing, instrumentation)
consumes it. *Vendored from `mattpocock/skills` (MIT).*

## Repository-internal skills (not deployed)

Some skills in `templates/skills/` are **not part of the consumer surface**:
they are coupled to master-plan's own fixtures and the archived lessons
catalog, so they have no value to adopters. They live in the skill tree
for repo-internal use (the dogfood loop, milestone reviews of master-plan
itself) but are **not** in the public catalog above and are **not**
deployed by `mp install`.

| Skill | Audience | Notes |
|-------|----------|-------|
| `mp-code-review` | Master-plan maintainers only | Manifest `category: internal` — excluded from `mp install` / `--list-skills`. Lesson-pattern pre-screen + runnable fixtures in `crates/mp/tests/code_review_patterns.rs`; lessons catalog at `crates/mp/tests/fixtures/code-review-lessons.md`. Load from the repo tree, not via install. |

The consumer-surface de-internalization rules (no `M\d+` IDs, no `L\d+`
codes, no dead `docs/` pointers) do **not** apply to repository-internal
skills — they are not shipped to adopters. They still get the same
documentation and code-review discipline as anything else in the repo.

## How harnesses discover skills

Skills are deployed into the conventional per-harness skills directory
(`~/.agents/skills`, `~/.cursor/skills`, the Pi skills tree). The harness reads
each skill's `description` frontmatter to decide when to load it, then surfaces
the full `SKILL.md` to the agent when the trigger fires. `mp install --check`
verifies that the deployed set matches the registry and reports drift.

## Authoring rules — keep the consumer surface self-contained

Skills ship to adopters who have **no access to this repo's plan or history**.
Anything that only makes sense with that history is a leak. Two rules cover the
cases seen in the wild.

### 1. Name the capability, not the milestone that introduced it

Internal milestone IDs (`M\d+`) and lesson codes (`L\d+`) are
*provenance* — they record when something was born, not what to do. An
adopter's agent cannot run `mp show milestone <id>`, so the reference
is unactionable, and it rots the moment that milestone's behavior
evolves. Rewrite to the capability:

| Leaky | Self-contained |
|-------|----------------|
| "**M\d+ consult FIRST:** `mp config get agent.automation.branch_strategy`" | "Before the first plan write, consult `mp config get agent.automation.branch_strategy`" |
| "the M\d+ advisory lock and the M\d+ killpg timeout" | "the advisory flock lock and the per-AC killpg timeout" |
| "per the M\d+ `SeverityRank` contract" | "using the `low\|medium\|high` severity order `mp` parses" |
| "L\d+ (author ≠ reviewer)" | inline the prose: "the author should not be the only reviewer" |

### 2. No pointers to repo-internal files

A skill that says "read `docs/code-review-lessons.md`" hands the consumer a
broken link — that path is not part of the shipped docs tree. Either inline
the content the agent needs, or link to a file that *does* ship (another skill
in the same bundle, resolved by relative path). Treat any `docs/…` or
`crates/…` path in a skill as suspect unless it is part of the deployed bundle.

### Where milestone IDs *are* allowed

Inside `master-plan/` (the plan zone) and repo-internal dogfood notes
(`mp-dogfood-log.md`, plan JSON, test fixtures), milestone IDs are the native
vocabulary and stay. The ban is specifically the **consumer surface**:
`templates/skills/**`, `docs/**`, and any README an adopter reads.

### Vendored third-party skills

`codebase-design` and `diagnosing-bugs` are vendored from `mattpocock/skills`.
Keep the upstream attribution the consumer needs (source URL + license); drop
the internal `M\d+:` prefix that records *which milestone vendored it* — that
is provenance, not license info.

### Interactive forks — prefer the asking tool

When a skill contains an interview step or any decision fork that needs the
human (`mp init`, `mp interview checklist|gaps`, spec-grill rounds, milestone
create/edit, or a "how do you want to proceed?" moment), write it to use the
harness's structured asking/question tool — not a prose prompt. Offer concrete
options, mark at most one `(recommended)` (only with evidence from
context/conventions/risk; never fake it on a values call), and always leave a
"type your own" path. Open-ended clarifications stay open-ended. If the harness
has no asking tool, fall back to numbered text options with the recommended one
marked `(recommended)`. Canonical statement: `master-plan/AGENTS.md` §3.1.

### Preventing recurrence

A ripgrep guard (`\bM\d{2,4}\b` and `\bL\d{1,3}\b`, plus the known dead-link
paths `docs/code-review-lessons.md` and `docs/dogfood/…`) over `templates/skills/`
and `docs/` catches new leaks at review time. Legitimate synthetic IDs in CLI
examples (e.g. `mp path pin 42 --before 17` in `docs/agent-guide/`) go in an
allowlist. The guard runs as part of `make lint` — see
[`scripts/check-consumer-surface.sh`](../../scripts/check-consumer-surface.sh)
and the `make consumer-surface-lint` target.

## See also

- The two CLIs the skills drive: [`../mp/`](../mp/) and [`../raul/`](../raul/).
- The lifecycle the core skills orchestrate: [`../milestone-lifecycle/`](../milestone-lifecycle/).
- An agent-oriented walkthrough: [`../agent-guide/`](../agent-guide/).
