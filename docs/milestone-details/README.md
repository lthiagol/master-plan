# The anatomy of a milestone

A milestone is one JSON document with a fixed shape. This is a tour of every
part: what it is, what it's for, and which `mp` commands read and write it. The
shape is enforced by validation; you never edit it by hand — you mutate it
through `mp`, which keeps it valid.

```
milestone document
├── milestone          ← identity + lifecycle + meta
├── intent             ← outcome (one sentence)
├── problem            ← why this is needed
├── scope              ← in_scope / out_of_scope boundaries
├── acceptance_criteria[] ← observable, verifiable proof points
├── design_decisions[] ← trade-offs worth recording
├── open_questions[]   ← unresolved items (must clear before approval)
├── work_packages[]    ← implementation groupings (phase 2)
├── steps[]            ← the implementation plan (phase 2)
├── verification       ← completion evidence (date/branch/evidence)
├── findings[]         ← review feedback + remediation record
└── delta              ← brownfield change descriptor (optional)
```

---

## `milestone` — identity & lifecycle

The envelope. Carries the id, title, lifecycle, and bookkeeping.

| Field | Purpose |
|-------|---------|
| `id`, `title`, `slug` | Identity. `id` is a stable numeric id (e.g. `42`). |
| `lifecycle` | The single canonical state — see [`../milestone-lifecycle/`](../milestone-lifecycle/). One of `draft`, `groomed`, `approved`, `in-progress`, `done`, `self-reviewed`, `reviewed`, `complete`, `remediation`. |
| `lifecycle_at` | RFC3339 timestamp of the last lifecycle transition (for "3d ago" displays). |
| `spec_status`, `execution_status` | Legacy read-only aliases derived from `lifecycle`. Kept for older consumers. |
| `priority` | `urgent` \| `high` \| `normal` \| `low` (default `normal`). |
| `effort` | Size estimate (default `S`). |
| `risk` | `low` \| `medium` \| `high` (default `low`). Medium/high with no design decisions trips a warning. |
| `change_kind` | Greenfield vs. delta/brownfield marker. |
| `depends_on[]` | Milestone ids this one needs done first (gate **G8**). Cycle-checked. |
| `created`, `updated` | ISO dates. |
| `target_version` | Target release version. |
| `executed_by` | Who/what executed it. |

**Overlays** (orthogonal to lifecycle, separate booleans):

| Field | Purpose |
|-------|---------|
| `blocked`, `blocked_at`, `block_reason`, `blocked_by` | `milestone block --reason …` / `unblock` |
| `deferred`, `deferred_reason` | `milestone defer --reason …` / `reopen` |
| `cancelled` | Terminal overlay (`set-status cancelled`). |
| `needs_regrooming` | Flipped by validation when an approved spec drifts. |
| `remediation_pre_state` | The lifecycle value captured when entering `remediation`, so the exit restores it exactly. |

Commands: `milestone create`, `milestone update`, `milestone set-lifecycle`,
`milestone set-status`, `milestone set-priority`, `milestone block/unblock`,
`milestone defer/reopen`, `milestone approve`.

---

## `intent` — the outcome

```json
{ "intent": { "outcome": "What users can do after this ships." } }
```

One sentence describing the user-visible result. The most important field for
keeping a milestone focused — if you can't state the outcome crisply, the spec
isn't ready.

---

## `problem` — the why

```json
{ "problem": { "description": "Why this is needed — the gap it fills." } }
```

The motivation. What's broken, missing, or painful today? A milestone without a
problem is a solution in search of one.

---

## `scope` — the boundary

```json
{
  "scope": {
    "in_scope": ["Specific deliverable"],
    "out_of_scope": ["Explicit non-goal 1", "Explicit non-goal 2"]
  }
}
```

What this milestone will and **won't** deliver. Naming out-of-scope items is
where most scope creep is killed. The default gate (**G4**) requires at least two
out-of-scope items (`full` profile) or one (`hybrid`) before the spec can reach
`review`.

---

## `acceptance_criteria[]` — the proof

```json
{
  "acceptance_criteria": [
    {
      "id": "AC-01",
      "description": "Observable behavior that proves completion",
      "verification": "cargo nextest run -p mp --test config_set",
      "status": "pass",
      "evidence": "cargo nextest run -p mp --test config_set --no-fail-fast  exit 0"
    }
  ]
}
```

The heart of the milestone. An AC is an observable, verifiable behavior with a
**command** that proves it. Completion (`milestone complete`) requires every AC
to be `pass`ed with real evidence (what ran + exit code), or `fail`ed with a
reason.

| Field | Purpose |
|-------|---------|
| `id` | Stable id like `AC-01`. |
| `description` | What the system must do. |
| `verification` | The command/check that proves it. |
| `status` | `pass` / `fail` / (empty). |
| `evidence` | The actual run record (test name + exit code). **Not prose.** |

At least one AC is required before `review` (gate **G3**). Commands:
`milestone ac add|show|list|update|bulk|pass|fail|remove` (alias `criterion`).

---

## `design_decisions[]` — the trade-offs

```json
{
  "design_decisions": [
    { "area": "storage", "choice": "JSON files", "rationale": "grep-able, diff-friendly, no DB" }
  ]
}
```

Record decisions only where there was a real choice. A medium/high-risk
milestone with no design decisions trips a validation warning. Commands:
`milestone design-decision add|update|remove`.

---

## `open_questions[]` — the unresolved

```json
{
  "open_questions": [
    { "id": "Q-01", "question": "Do we need to support legacy fixtures?", "status": "open", "answer": "" }
  ]
}
```

Anything you haven't figured out yet. **All open questions must be resolved
before approval.** Commands: `milestone question add|resolve`.

---

## `work_packages[]` & `steps[]` — the implementation plan (phase 2)

Written **after** spec approval. Don't include these at creation time.

### Work package

```json
{
  "work_packages": [
    { "id": "WP1", "name": "Data layer", "goal": "…", "rollback": "…" }
  ]
}
```

A grouping of related steps with a goal and a rollback note. Commands:
`milestone wp add|update|remove`.

### Step

```json
{
  "steps": [
    {
      "id": "S1",
      "work_package": "WP1",
      "order": 1,
      "action": "Add config struct",
      "files": ["crates/mp/src/config.rs"],
      "tests": "cargo nextest run -p mp --test config_set",
      "done_when": "Config round-trips through mp config set",
      "status": "done",
      "covers_ac": ["AC-01"],
      "depends_on_steps": [],
      "evidence": ""
    }
  ]
}
```

A single unit of implementation work.

| Field | Purpose |
|-------|---------|
| `action` | What to do. |
| `files[]` | Files it touches (bare paths). |
| `tests` | The command that proves it — observable, not prose. |
| `done_when` | Human-readable success condition. |
| `status` | `pending` / `in-progress` / `done` / `failed`. |
| `covers_ac[]` | Which ACs this step advances (drives coverage analysis). |
| `depends_on_steps[]` | Ordering within the milestone. |
| `claimed_by`, `claimed_at`, `lease_expires_at` | Concurrency lease for parallel work. |
| `evidence` | Per-step run evidence. |

`files`/`tests` values prefer observable commands pinned to a crate/test, not
outcomes. Commands: `milestone step add|show|update|set-status|done|fail|claim|release|split|remove`.

---

## `verification` — completion stamp

```json
{ "verification": { "date": "2026-07-16", "branch": "feat/m42", "evidence": "all ACs green" } }
```

Written by `milestone complete`. The `evidence` string can be amended after the
fact with `milestone update --verification …` (useful for clearing a
`[force-bypassed]` marker once a follow-up closes the debt).

---

## `findings[]` — review & remediation

```json
{
  "findings": [
    {
      "id": "F-01",
      "severity": "high",
      "category": "correctness",
      "description": "…",
      "status": "open",            // open | fixed | dismissed
      "phase": "external",          // self | external (empty = self by convention)
      "author": "reviewer",
      "anchor": { "path": "…", "commit": "…", "new_range": { "start_line": 10, "end_line": 20 } },
      "thread": [ { "author": "…", "at": "…", "body": "…" } ],
      "summary": "…", "rationale": "…", "confidence": "high", "tags": []
    }
  ]
}
```

Structured review feedback. An open **external** finding auto-enters
`remediation`; resolving the last open one auto-exits it. An open **self**
finding blocks completion. See [`../milestone-lifecycle/review.md`](../milestone-lifecycle/review.md).
Commands: `reviews finding add|resolve|list`.

---

## `delta` — brownfield change descriptor (optional)

```json
{
  "delta": {
    "domain": "config",
    "added":    [ { "path": "…", "kind": "…" } ],
    "modified": [ { "path": "…", "before": "…", "after": "…" } ],
    "removed":  [ { "path": "…" } ]
  }
}
```

Present only for change-driven milestones on an existing codebase. Describes
what was added, modified, and removed so a reviewer can see the blast radius at
a glance. Omitted from disk entirely when unset.

---

## Reading milestones efficiently

You rarely need the whole document:

- **One field path:** `mp show milestone 42 --fields 'milestone.lifecycle,steps[].status'`
- **Health rollup:** `mp show milestone 42 --summary`
- **One AC:** `mp milestone ac show 42 AC-03`
- **One step:** `mp milestone step show 42 S2`
- **Find something:** `mp search "config validation" --type ac --include object`

Project specific paths instead of loading the whole document — it keeps agent
context small and your scripts fast.
