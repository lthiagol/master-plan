# Grooming, Challenge & Decomposition

Workflows for reviewing plans, finding gaps, splitting work, and keeping milestones
right-sized. **Status:** Implemented (v1 RC).

---

## §0. Grooming-depth rubric — "Is this groomed enough?"

A groomed milestone passes both the **machine-checkable** gates (validate warnings W40–W43)
and the **human-judgment** criteria below. If all of these are true, the milestone is
groomed enough to start execution.

### Human-judgment criteria

| # | Criterion | Check |
|---|-----------|-------|
| 1 | **Capability-sized steps** | Each step delivers a meaningful capability (not micro "write one line", not bundled "build entire feature"). A step should be hours, not days. |
| 2 | **Non-empty done_when** | Every step has a non-empty `done_when` that says what "done" means. (W40 catches this mechanically.) |
| 3 | **AC ↔ step coverage** | Every acceptance criterion is covered by at least one step's `covers_ac`. No AC is orphaned. (G10 catches gaps under strictness=full.) |
| 4 | **No bundled concerns** | The milestone addresses exactly one concern. If two unrelated concerns are present, split into separate milestones. |
| 5 | **Real (non-hypothetical) AC examples** | AC verification values reference actual test commands, scripts, or observable outcomes — not vague placeholders like "works" or "test it". **Quantitative perf ACs** must use `mp_measure!` (or an explicit `manual:` prefix with a measured dogfood-log entry) — see [PERF-ACS.md](./PERF-ACS.md). |
| 6 | **Design notes on ambiguous / high-risk work** | Milestones with `risk=medium` or `risk=high`, or those requiring cross-system integration, have at least one `design_decisions` entry covering the risky seam. (W42 catches empty design_decisions on medium/high risk.) |
| 7 | **No stale references** | Step actions and AC descriptions do not reference milestone or step IDs that no longer exist in the plan. (W43 catches stale refs mechanically.) |

### Design-note convention

When a design note is required (criterion 6):

- **Where it lives:** the milestone's `design_decisions` field (a `Vec<DesignDecision>` in the milestone JSON). For complex topics, the `rationale` field may link to an external doc.
- **What it contains:** interactions with other systems, alternatives considered, risky seams, and the reasoning behind the chosen approach.
- **How to write it:** `mp milestone design-decision add <id> --decision "<choice>" --rationale "<why>"`.

### Quick checklist

Before marking a milestone `spec_status: ready`, run through:

- [ ] Steps are capability-sized (hours, not days or weeks)
- [ ] Every step has `done_when` (W40)
- [ ] Every AC is covered by a step (G10)
- [ ] Milestone has one concern, not multiple (split if needed)
- [ ] ACs reference real verification commands, not placeholders
- [ ] If risk=medium/high, design_decisions are populated (W42)
- [ ] No stale M/S references in actions/ACs (W43)

For a deep-dive review, run `mp challenge start <id> --scope plan`.

This doc is the design reference for:

- Rich milestone listing (`list milestones --filter`)
- Step listing (`list steps`)
- **Groom** — entry-point routing (“what does this milestone need?”)
- **Decompose** — break a milestone into work packages and steps
- **Split** — break one step into smaller steps (or a milestone into two)
- **Challenge** — structured audit with findings and resolutions

See also [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) for command signatures and
[SPEC.md](./SPEC.md) for data model and gates.

---

## 1. Design principles

1. **Flows, not one-offs.** Common grooming actions get a predictable CLI sequence
   (like `mp brief todo → edit → done`).
2. **Agent reads JSON, user sees tables.** Use `raul milestones` / `raul show` for human scanability.
3. **Challenge produces findings, not silent edits.** Gaps are recorded (`F-01`),
   then resolved explicitly — so challenge sessions are reviewable.
4. **Decompose after spec approval.** Implementation plans (WPs/steps) require
   `spec_status >= ready` (gate G5).
5. **Natural language maps to one canonical command.** “List milestone 3” →
   `mp show milestone 03`, not `list`.

---

## 2. Milestone listing

### Command

```bash
mp list milestones
mp list milestones --filter all
mp list milestones --filter pending
mp list milestones --filter in-progress
mp list milestones --filter partial
mp list milestones --filter done
mp list milestones --filter grooming
```

Legacy fine-grained filters remain: `--status`, `--spec-status`, `--blocked`.

### Filter presets

| Filter | Includes milestones where… |
|--------|----------------------------|
| `all` | Default. All non-archived milestones. |
| `pending` | Not started: `execution_status = planned` and no step is `in-progress` or `done`. |
| `in-progress` | `execution_status = in-progress` **or** any step is `in-progress`. |
| `partial` | Started but not finished: at least one step `done` or `in-progress`, but milestone not `done`. **Or** `spec_status = ready` with zero steps (approved spec, no impl plan yet). |
| `done` | `execution_status = done` (typically `spec_status = verified`). |
| `grooming` | Needs planning attention (see §2.1). |
| `blocked` | `execution_status = blocked` or dependency gate G8 blocks start. |

### §2.1 `grooming` filter logic

A milestone appears in `--filter grooming` when **any** of:

- `spec_status` ∈ `draft`, `interview`, `review`
- `spec_status = ready` and no work packages or zero steps
- Open findings on an active challenge for this milestone
- `mp interview gaps` or `mp plan gaps` reports blockers (when implemented)

### Human output (compact headers)

`raul milestones` renders a table — no full spec body:

```text
 ID  Display                      Spec          Exec         Steps   Progress
───  ───────────────────────────  ────────────  ───────────  ──────  ────────
 01  M01 — Environment Setup      verified      done         4/4     100%
 02  M02 — Config & Vault         ready         planned      0/0     —
 03  M03 — OAuth Login            ready         in-progress  2/5     40%
 04  M04 — Search                 review        planned      —       spec
```

- **Steps** = `done_count / total_count` (empty `—` when spec not ready for steps).
- **Progress** = percent of steps `done`, or `spec` when still in spec phase.

### JSON shape

```json
{
  "filter": "partial",
  "count": 2,
  "milestones": [
    {
      "id": "03",
      "display": "M03 — OAuth Login",
      "title": "OAuth Login",
      "spec_status": "ready",
      "execution_status": "in-progress",
      "steps": { "done": 2, "total": 5, "in_progress": 1 },
      "progress_percent": 40,
      "blocked": false,
      "needs_grooming": false
    }
  ]
}
```

### “List milestone X”

There is no `mp list milestone <id>`. Canonical:

```bash
raul show 03
mp show milestone 03    # agent
```

---

## 3. Step listing

```bash
mp list steps
mp list steps --milestone 03
mp list steps --milestone 03 --status pending,in-progress
mp list steps
```

### Human output

```text
M03 — OAuth Login

 ID    Status       Action
─────  ───────────  ─────────────────────────────────────
 S1    done         Add OAuth config schema
 S2    in-progress  Implement callback handler
 S3    pending      Wire session middleware
```

### JSON shape

```json
{
  "milestone": { "id": "03", "display": "M03 — OAuth Login" },
  "steps": [
    {
      "id": "S2",
      "work_package": "WP1",
      "status": "in-progress",
      "action": "Implement callback handler",
      "files": ["src/auth/callback.rs"],
      "tests": "cargo test oauth_callback",
      "done_when": "Tests pass"
    }
  ]
}
```

---

## 4. Groom — entry point

`mp milestone groom <id>` answers: **what should we do with this milestone next?**

```bash
mp milestone groom 03
mp milestone groom 03
```

### Response (JSON)

```json
{
  "milestone": { "id": "03", "display": "M03 — OAuth Login" },
  "phase": "implementation",
  "spec_status": "ready",
  "execution_status": "in-progress",
  "steps": { "done": 2, "total": 5 },
  "recommended": [
    { "command": "mp list steps --milestone 03", "reason": "See remaining work" },
    { "command": "mp challenge start 03 --scope plan", "reason": "Audit plan before continuing" }
  ],
  "blockers": []
}
```

### Phase detection

| Phase | Condition | Typical next command |
|-------|-----------|----------------------|
| `spec` | `spec_status < ready` | `mp interview checklist --type milestone --id <id>` |
| `decompose` | `spec_status = ready`, no steps | `mp milestone decompose <id>` |
| `implementation` | Has steps, not all done | `mp next` or `mp list steps` |
| `challenge` | User asked to review | `mp challenge start <id>` |
| `done` | `execution_status = done` | `mp show milestone <id>` |

---

## 5. Decompose — break milestone into steps

Triggered by: *“break M03 into steps”*, *“decompose milestone P”*, *“plan implementation”*.

### Command

```bash
mp milestone decompose 03
mp milestone decompose 03 --work-packages 3
mp milestone decompose 03
```

### Flow

```text
1. mp show milestone <id>          # spec + ACs
2. Gate: spec_status >= ready (else error + suggest spec interview)
3. mp milestone plan <id>                      # scaffold WPs if empty
4. mp plan gaps <id>             # machine-readable gaps
5. Agent proposes WPs/steps; user confirms
6. mp wp add / mp step add ...                 # writes
7. mp validate
8. mp milestone decompose <id>   # returns summary when complete
```

`mp plan gaps` (alias: `mp interview checklist --type implementation-plan --id <id>`)
checks:

| Check | Severity |
|-------|----------|
| At least one work package | blocker |
| Each WP has ≥1 step (via `steps[].work_package`) | blocker |
| Each step has `action`, `done_when` | major |
| Each step has `files` or explicit `files = []` | minor |
| Each step has `tests` or `verification = manual` | major |
| Every AC mapped to ≥1 step (coverage) | major |
| No step references files outside repo | minor |
| Closure WP present (verify, lint) | recommended |

### End state

Milestone has a complete implementation plan; safe to `mp next` after user confirms.

---

## 5.1 AC coverage (acceptance criteria ↔ steps)

**AC coverage** answers: *for each acceptance criterion (AC-01, AC-02, …), is there at
least one implementation step that gets us there?*

### Two layers

| Layer | What | When |
|-------|------|------|
| **Spec** | `[[acceptance_criteria]]` — what “done” means | Phase 1 (spec interview) |
| **Plan** | `steps[].covers_ac` — which steps implement/verify each AC | Phase 2 (decompose) |

**AC** = acceptance criterion (`AC-01`). **Coverage** = every AC appears in at least one
step’s `covers_ac` list before the implementation plan is complete.

### Example

```json
[[acceptance_criteria]]
id = "AC-01"
description = "OAuth flow completes and returns a valid session"
verification = "cargo test oauth_flow_end_to_end"

[[acceptance_criteria]]
id = "AC-02"
description = "Invalid callback shows error without panic"
verification = "cargo test oauth_callback_error"

[[steps]]
id = "S1"
work_package = "WP1"
action = "Add OAuth routes"
covers_ac = ["AC-01"]

[[steps]]
id = "S2"
work_package = "WP1"
action = "Implement callback + error handling"
covers_ac = ["AC-01", "AC-02"]

[[steps]]
id = "S3"
work_package = "WP2"
action = "Run integration suite and attach evidence"
covers_ac = ["AC-01", "AC-02"]
```

| AC | Covered by | Status |
|----|------------|--------|
| AC-01 | S1, S2, S3 | covered |
| AC-02 | S2, S3 | covered |

### Gap types (`mp plan gaps` / `mp path --coverage`)

| Gap | Meaning |
|-----|---------|
| **uncovered_ac** | AC has no step referencing it |
| **orphan_step** | Step has empty `covers_ac` (warning — allowed but discouraged) |
| **over_covered** | Fine — multiple steps may cover one AC |
| **premature_verify** | All covering steps `done` but AC still `pending` → run verification |

### Commands

```bash
mp plan gaps 03              # includes coverage section
mp plan coverage 03                        # implemented
mp path                      # per-milestone coverage summary
mp milestone criterion pass 03 AC-01 ...   # after covering steps done
```

**`mp plan gaps` coverage output:**

```json
{
  "milestone": "03",
  "coverage": {
    "ok": false,
    "acceptance_criteria": [
      {
        "id": "AC-01",
        "covered_by": ["S1", "S2"],
        "status": "covered"
      },
      {
        "id": "AC-03",
        "covered_by": [],
        "status": "uncovered"
      }
    ],
    "orphan_steps": ["S4"],
    "errors": [
      { "code": "COV-01", "ac": "AC-03", "message": "no step covers this acceptance criterion" }
    ]
  }
}
```

### Gates

| Gate | Rule |
|------|------|
| **G10 — AC coverage** | Before `execution_status: in-progress`, every AC must be covered by ≥1 step (warn at decompose, error at validate if strict) |
| **G6 / G7** | At finish, each AC must be `passed` with evidence (unchanged) |

### Agent rules

1. On decompose, map each AC to at least one step when writing `mp step add`.
2. On challenge (`scope plan`), flag uncovered ACs as findings (`category: coverage`).
3. Do not `mp milestone complete` until all ACs are `passed` — coverage is about the
   **plan**, not replacing verification.

---

## 6. Split — smaller steps or milestones

### 6.1 Split a step

Outline IDs: parent keeps its id; children get decimal suffixes. See [IDS.md](./IDS.md).

```bash
mp step split 03 S3
mp step split 03 S3
mp step split 03 S3 --json @-    # agent provides bodies for S3, S3.1, S3.2
```

**Example:** `S1`, `S2`, `S3` → `S3` too big → `S1`, `S2`, `S3`, `S3.1`, `S3.2`.

**Behavior:**

1. Load step `S3` (must be `pending` or `in-progress`; warn if `done`).
2. Add decimal children `S3.1`, `S3.2`, … alongside parent `S3`.
3. **Do not renumber** `S4`, `S5`, …
4. Distribute `action` / `files` / `tests` / `done_when` via JSON payload or agent edits after split.
5. Further split of `S3.1` → `S3.1`, `S3.1.1`, `S3.1.2` (arbitrary depth).

**Sort order:** outline / natural numeric (`S3 < S3.1 < S3.10 < S4`).

### 6.2 Split a milestone

When a milestone is too large (multiple disconnected components, > ~3 days):

```bash
mp milestone split 03
mp milestone split 03 --into 2 --titles "OAuth core,OAuth UI"
```

**Behavior:**

1. Parent **M3** (`03`) keeps first/reduced scope.
2. Children **M3.1**, **M3.2** (`03.1`, `03.2`) are new milestone files.
3. Set `depends_on` so slices run after parent (or in chain).
4. Step ids in each file start fresh (`S1`, `S2`, …) — scoped per milestone.

> Use next integer **M4** (`04`) only for **new** work, not a slice of M3.

---

## 7. Challenge — structured plan review

For when you **stress-test** a spec or implementation plan — a flow you run often.

### Session model

**File:** `master-plan/reviews/challenges/<milestone>-<nn>.json`  
**Example:** `reviews/challenges/03-plan-01.json`

```json
[challenge]
id = "CH-03-01"
milestone_id = "03"
scope = "plan"              # spec | plan | full | sequence
status = "open"             # open | closed
created = "2026-06-17"
closed = ""
summary = ""

[[findings]]
id = "F-01"
severity = "major"          # minor | major | blocker
category = "gap"            # gap | risk | sequencing | scope | step-quality | coverage
title = "Step S2 has no tests"
description = "Callback handler step lacks test command."
target = "step:S2"          # step:S3.1 | ac:AC-01 | wp:WP1 | milestone
status = "open"             # open | resolved | dismissed
resolution = ""
action = ""                 # see §7.3
action_ref = ""
```

### Commands

```bash
# Start
mp challenge start 03 --scope plan
mp challenge start 03 --scope spec      # before approval
mp challenge start 03 --scope full      # spec + plan

# Audit (auto-detect gaps → findings)
mp challenge audit 03
mp challenge audit 03 --scope plan

# Findings
mp challenge list                       # open challenges
mp challenge list 03                    # findings for milestone
mp challenge add 03 --title "..." --severity major --target step:S2
mp challenge resolve 03 F-01 --action update-step --payload @-

# Close
mp challenge done 03
mp challenge dismiss 03 F-02            # won't fix
```

### §7.1 Challenge scopes

| Scope | Audits | Typical when |
|-------|--------|--------------|
| `spec` | ACs, scope, scenarios, open questions | Before `mp milestone approve` |
| `plan` | WPs, steps, AC coverage, testability | After decompose, before/during execution |
| `full` | Spec + plan | Major milestone review |
| `sequence` | Dependencies, ordering across milestones | Roadmap review (no single milestone) |

`sequence` scope uses `milestone_id = ""` and lives in `reviews/challenges/roadmap-01.json`.

### §7.2 Audit flow (agent + user)

```text
1. mp challenge start <id> --scope plan
2. mp challenge audit <id>              # auto findings from plan gaps
3. mp challenge list <id>
4. Discuss with user; add manual findings if needed
5. For each finding:
     mp challenge resolve <id> F-01 --action <type> --payload ...
   or mp challenge dismiss <id> F-01 --reason "..."
6. mp validate
7. mp challenge done <id>
```

### §7.3 Resolution actions

| Action | CLI effect |
|--------|------------|
| `update-step` | `mp step update` with payload fields |
| `add-step` | `mp step add` |
| `split-step` | `mp step split` |
| `split-milestone` | `mp milestone split` |
| `update-spec` | `mp milestone update` (spec fields) |
| `defer-backlog` | `mp backlog add` + link in finding |
| `no-change` | Record verdict only |
| `resequence` | `mp milestone update` deps / order |

`mp challenge resolve` may apply the action immediately or stage it for confirmation
(`--dry-run` shows diff).

### §7.4 Challenge vs interview gaps

| Tool | Phase | Purpose |
|------|-------|---------|
| `mp interview gaps` | Spec (phase 1) | Missing required spec fields |
| `mp plan gaps` | Implementation (phase 2) | Missing/incomplete steps |
| `mp challenge audit` | Either | Findings with severity, history, resolutions |

---

## 8. User phrase → command map

| You say | Do |
|---------|-----|
| List all milestones | `mp list milestones` |
| List done / pending / in progress | `mp list milestones --filter <preset>` |
| What needs grooming? | `mp list milestones --filter grooming` |
| List milestone 3 | `mp show milestone 03` |
| List steps of M03 | `mp list steps --milestone 03` |
| Challenge the plan for M03 | `mp challenge start 03 --scope plan` → `audit` → `list` → resolve → `done` |
| Break M03 into steps | `mp milestone decompose 03` |
| Break step S3 into smaller steps | `mp step split 03 S3` |
| Refine step S2 | `mp step update 03 S2 --action "..." ...` |
| What should we do with M03? | `mp milestone groom 03` |

---

## 9. Agent workflows (AGENTS.md summary)

### Challenge a plan (common)

```text
mp show milestone <id>
mp challenge start <id> --scope plan
mp challenge audit <id>
mp challenge list <id>
# discuss findings with user
mp challenge resolve <id> F-01 --action update-step --payload ...
mp validate
mp challenge done <id>
```

### Decompose (common)

```text
mp milestone groom <id>
mp milestone decompose <id>
mp plan gaps <id>
mp wp add / mp step add ...
mp validate
# present plan to user for confirmation
```

### Refine / split step

```text
mp list steps --milestone <id>
mp step update <id> <step> ...     # refine in place
# or
mp step split <id> <step> --json @-  # smaller steps
mp validate
```

---

## 10. Implementation phase

| Phase | Commands | Notes |
|-------|----------|-------|
| **P1** | `step add/update`, `milestone plan`, `list steps` | Core writes |
| **P1.7** | `list milestones --filter`, `groom`, `decompose`, `plan gaps`, `challenge *`, `step split`, `milestone split` | This doc |

---

## 11. References

- [IDS.md](./IDS.md) — hierarchical outline IDs
- [SPEC.md](./SPEC.md) — gates, lifecycles, reviews directory
- [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) — full command reference
- [../schemas/challenge.schema.json](../schemas/challenge.schema.json)
- [../schemas/interview-checklist.json](../schemas/interview-checklist.json) — `implementation-plan` checklist
