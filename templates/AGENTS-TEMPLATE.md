# Master Plan — Agent Instructions

> **Agent guide:** [`docs/agent-guide/README.md`](~/.agents/master-plan/docs/agent-guide/README.md) — orientation + per-workflow detail.  
> **Repo session start:** [../AGENTS.md](../AGENTS.md) — read meta plan and propose next move.
>
> F-05: `../AGENTS.md` is a project-internal link (sibling file in the project root), not a toolkit doc — the rest of this template uses toolkit-absolute paths because toolkit docs do not live in consumer projects. The exception is intentional.

This project uses **spec-driven development**. The `master-plan/` directory is the
single source of truth for what to build, in what order, and how to verify it.

**You must follow these rules when planning or implementing work in this project.**

---

## 1. Non-Negotiable Rules

1. **Never read or edit files under `master-plan/` directly.** Use the `mp` CLI for
   all reads and writes.
2. **Spec before code.** Do not modify application source code until the relevant
   milestone is approved (`lifecycle: approved`).
3. **Two-phase milestones.** Phase 1 is the spec (what/why). Phase 2 is the
   implementation plan (how), created only after spec approval.
4. **Reads emit JSON by default.** Use `mp <command>` — omit `--format json` on reads (redundant).
5. **User-facing output** — summarize JSON or defer to `raul`.
6. **After every write, validate.** `mp validate`
7. **Plan-only mode.** When asked to plan without implementing, stop after `mp`
   writes. Do not touch application code.
8. **Execution mode.** Check `mp execution status`. In `planning` mode, do not implement
   unless the user directs a specific milestone/step. In `autonomous` mode, run the
   `next` loop until blocked — then escalate. Never change spec without pausing.

---

## 1a. Plan zone vs code zone

| Zone | What | Rules |
|------|------|-------|
| **Plan** | `master-plan/` | All reads/writes via `mp`. Never hand-edit plan files. |
| **Code** | Application source (`src/`, `tests/`, configs) | Search, read, implement. Harness tools OK (ripgrep, LSP). |

Use code zone to learn **current behavior** during interviews. Record findings in spec
fields (`scope`, `problem.description`, `design_decisions`, acceptance criteria) via
`mp milestone create --json @-` — not by editing plan JSON directly.

See [`docs/milestone-details/`](~/.agents/master-plan/docs/milestone-details/) for the brownfield `delta`
descriptor; greenfield milestones omit it.

---

## 1b. Agent output contract (do not drift)

`mp` stdout is **JSON by default** on read commands.

| Need | Use | Avoid |
|------|-----|-------|
| Read plan state | `mp status`, `mp show milestone <id>` | `mp … --format json` (redundant) |
| Few fields only | `mp show milestone <id> --fields 'milestone.lifecycle,steps[].status'` | `mp show … \| jq` |
| Remediation health | `mp show milestone <id> --summary` | jq count aggregates |
| Validate rollup | `mp validate --summary` | jq on validate output |
| Open findings | `mp reviews finding list <id> --open` | `milestone update --json` with findings |
| Batch resolve | `mp reviews finding resolve <id> --all` | hand-edit finding status in plan JSON |
| Write plan | `mp milestone create --json @-`, `mp step done`, etc. | `sed` / editor on `master-plan/` |
| Debug on-disk JSON | `mp show milestone <id> --format raw` | dumping raw JSON to users |
| Human view | launch `raul` (TUI) | `--format human` (removed) |

**Loop guard:** if the same `mp` write returns success but the next read shows no
change, stop after 2 attempts — read `--help`, check
[`docs/agent-guide/`](~/.agents/master-plan/docs/agent-guide/), then `mp milestone block` + escalate.

---

## 2. Tooling

| Tool | Location |
|------|----------|
| CLI | `mp` (Master Plan CLI — `~/.agents/master-plan/bin/mp`) |
| Plan directory | `./master-plan/` |
| Spec reference | `mp plan show` |

If `mp` is not found, tell the user to install the master-plan toolkit. Do not fall
back to editing plan files by hand.

### Intake routing (which lane?)

```text
Too vague / later?     → idea
Small fix / polish?    → track (bugfix / tweak)
Defer scope formally?  → backlog
Feature / behavior?    → milestone (full spec)
Prod emergency?        → track bugfix
```

---

## 3. Workflows

### 3.0 Project brief (first session after init)

Use when `brief.status = in_progress` or `planning_phase = brief`.

```text
1. mp brief todo
2. Ask 1–2 questions per pending topic; user brain-dumps freely
3. mp brief edit T01 --body "..."  (repeat for each topic)
4. mp brief add ...                (optional custom topics)
5. mp brief list     # established context
6. mp brief done
7. → charter interview (§3.1) or milestone planning
```

Optional: `mp interview checklist --checklist-type brief` for suggested question rounds.

Do not start milestone specs until `mp brief done` unless the user explicitly skips the brief.

---

### 3.1a Brownfield change (behavior change on existing code)

Use when changing existing behavior — not a greenfield subsystem.

**Small** → track (`§3.7`). **Large** → milestone with explicit before/after.

```text
1. mp doctor
2. Code zone: locate current behavior (files, tests)
3. mp interview checklist --checklist-type milestone
4. Spec must state: what exists today, what changes, what stays the same
5. mp milestone create --json @-  (change_kind: greenfield is OK until P4)
6. Approve → decompose → implement
```

**P4 (delta milestones):** `change_kind: delta`, `delta.domain`, ADDED/MODIFIED/REMOVED;
`mp specs show <domain>` before create; `mp brownfield scan` optional assist.

---

### 3.1 Plan a new feature or bug (interview mode)

Use when the user asks to plan, groom, or spec work without implementing yet.

```text
1. mp interview checklist --type milestone
2. Ask the user 2–4 questions per round (skip topics already answered)
3. Propose defaults from codebase analysis; user confirms or corrects
4. mp milestone create --json @-   (spec fields only — no WPs/steps yet)
5. mp milestone set-spec-status <id> review
6. Summarize the spec in natural language
7. On user approval → mp milestone approve <id>
8. mp validate
9. Stop. Do not write application code.
```

**Interview topics — Phase 1 (spec — lean 2.0 model):**
1. Intent — `intent.outcome`, `problem.description`
2. Scope — `scope.in_scope`, `scope.out_of_scope` (minimum 2 exclusions)
3. Acceptance — `acceptance_criteria` (AC-XX + verification command)
4. Design — `design_decisions` (area/choice/rationale) when trade-offs exist
5. Gaps — `open_questions` (Q-XX; resolve before approval)

**Do not scaffold dropped ceremony fields:** behavior/scenarios, FR-XX/NC-XX
requirements blocks, success_criteria (SSC-XX), interface, `context.references`,
technical_context, assumptions, risks, follow_ups.

**Interview topics — Phase 2 (implementation plan, after approval):**
1. Decomposition — work packages, steps (files, tests, done-when, rollback)

---

### 3.2 Decompose into implementation plan (phase 2)

Use only after the spec is approved (`lifecycle: approved`).

Triggered by: *"break this into steps"*, *"plan implementation for M03"*, *"decompose M03"*.

```text
1. mp milestone groom <id>
2. mp milestone decompose <id>
3. mp plan gaps <id>
4. mp milestone wp add <id> ... (work package grouping)
5. mp milestone step add <id> --wp WP1 ... (steps S1, S2, … with files, tests, done-when)
6. mp validate
7. Present the implementation plan to the user for confirmation
```

Do not start coding until the user confirms the implementation plan (or explicitly
says to proceed).

#### 3.2.1 Scope discipline for verification strings

Scope each step's `tests` and each acceptance criterion's `verification` to the
**affected code** (a single crate, module, or test file) — not the whole workspace.
The per-AC verification gate runs these commands with a bounded timeout, so a cold
full-suite run is both too slow and too broad to pinpoint the milestone's behavior.
Keep whole-suite commands (`cargo test --workspace`, `make test`, etc.) in CI, not in
the per-AC gate.

A `tests`/`verification` value that is **not** prefixed `manual:` is executed from the
project root during verification. Prefer real commands; use `manual: <note>` only for
genuinely non-automatable checks.

---

### 3.2a Split a step (step too large)

```text
1. mp list steps --milestone <id>
2. mp milestone step split <id> <step> --json @-   # e.g. S3 → S3, S3.1, S3.2
3. mp validate
```

---

### 3.2b Challenge a plan (stress-test)

Use when the user wants to review, challenge, or find gaps in a spec or implementation plan.

```text
1. mp show milestone <id>
2. mp milestone challenge start <id> --scope plan    # or spec | full
3. mp milestone challenge audit <id>
4. mp milestone challenge list <id>
5. Discuss findings with user
6. mp milestone challenge resolve <id> F-01 --action update-step --payload ...
7. mp validate
8. mp milestone challenge done <id>
```

---

### 3.3 Execute work

Use when implementing approved, decomposed milestones.

```text
1. mp execution status
2. mp next
3. mp milestone set-status <id> in-progress     # once, on this milestone
4. mp milestone step set-status <id> <step> in-progress   # BEFORE code changes
5. Implement application code (outside master-plan/)
6. mp milestone step done <id> <step>
7. mp validate
8. Repeat 2–7 until all steps done
9. → §3.3b (verify) — do NOT call complete here
```

**Blocked?** `mp milestone block <id> --reason "..."` → `mp execution pause` → escalate to user.  
**Resume:** `mp milestone unblock <id>`.

### 3.3a Execution handoff (autonomous mode)

When the user says “go execute”, “work through the plan”, or similar:

```text
1. mp execution check
2. Present execution_ready milestones and blockers
3. User confirms → mp execution handoff
4. Loop: next → step in-progress → code → step done → validate
5. On ambiguity, validate fail, or new scope → mp execution pause + escalate
```

See [`docs/milestone-lifecycle/execution.md`](~/.agents/master-plan/docs/milestone-lifecycle/execution.md)
and the milestone-loop cheat sheet in
[`docs/mp/getting-started.md`](~/.agents/master-plan/docs/mp/getting-started.md).

---

### 3.3b Verify (self-step, executor)

After all steps are `done`, the executor **verifies** their own work before handing off
to review. This is not a code review — it is proof that the work is honest.

```text
1. Re-run each step's tests value from the project root; confirm green
2. For each AC: run its verification target, then
   mp milestone criterion pass <id> <ac-id> --evidence "<test-name> exit 0"
3. If an AC cannot be honestly passed:
   mp milestone block <id> --reason "AC-05 blocked: <why>"
   → escalate. Do NOT --force.
4. mp milestone complete <id>   # → lifecycle: executed; enters the review queue (NOT shipped)
5. mp validate
```

**Evidence is test output, not prose.** Record the test name and exit code
(`cargo test -p mp exit 0`, `crates/x.rs pass`). Never write prose claims like
*"Test X verifies Y"* — that is an assertion, not evidence. If you did not run it,
do not claim it.

**Drift happens.** If the implementation diverged from the spec (a step was skipped,
an AC was relaxed, a test was made source-grep instead of behavioral), record it
explicitly: `mp reviews finding add <id> --severity low --desc "drift: <what>"`
rather than hiding it. A reviewer who finds undeclared drift loses trust in the
whole submission; a declared drift is just a decision they can evaluate.

### 3.3c Independent review (mandatory for milestones)

Autonomous execution is **always** followed by an independent review pass — a
different session/context than the executor — before work is considered shipped.
The terminal `complete` state is reachable **only** through `mp reviews pass`, never
through `complete` alone.

> **Risk tiering — which items need external review?**
>
> | Item kind | Flow | External review? |
> |-----------|------|------------------|
> | **Track / backlog tweak** | execute → verify → done | **No** — low blast radius, one-pass |
> | **Milestone** | execute → verify → independent review → complete | **Yes** — higher risk earns the loop |
>
> If a track grows complex, promote it (`mp track promote --to-milestone`) and it
> inherits the full flow.

```text
1. mp reviews pending                     # find review-ready milestones
2. mp reviews claim <id>                  # → in-review (different agent than executor)
3. mp execution report <id>               # read claims FIRST
4. Verify claims against diff + tests     # drift hides in reports
5. mp reviews pass <id> --verdict ok --reviewer <who>   # → complete (terminal)
   — OR —
   mp reviews finding add <id> ...        # → remediation; findings attached
6. mp validate
```

**Verify, don't trust.** The execution report and per-AC evidence are *claims*.
Read them first, then confirm against the actual diff and test output. A claim
that says *"test X passes"* is verified by running test X, not by trusting the string.

**Remediation loop:** filing an open external finding sets the milestone to
`remediation`. Whoever fixes the findings runs `mp milestone set-status <id>
in-progress`, addresses each finding, re-verifies (§3.3b), and re-completes — the
milestone re-enters the review queue. A finding is closed with
`mp reviews finding resolve <id> <F-XX> --commit <sha>`.

### 3.3d Execution contract

These rules are permanent — not session scratch notes:

1. **Never complete on red tests.** Step `tests` values are a gate, not a
   suggestion — non-`manual:` values are executed from the project root. Red
   tests block the transition.
2. **`--force` is debt, not a shortcut.** It requires a recorded reason and
   creates visible evidence of the bypass. Prefer `mp milestone block <id>
   --reason "..."` + escalation over forcing. A force-bypassed milestone cannot
   reach `complete` until the bypass is resolved or explicitly accepted by a reviewer.
3. **Never hand-edit plan files.** Every status transition, evidence string, and
   finding goes through an `mp` command. Editing `master-plan/*.json` directly is
   a plan-zone violation — `mp validate` will not catch it, but the audit trail
   will show the gap.
4. **Evidence is test output, not prose.** `criterion pass --evidence` records
   what ran and its exit code. *"Test X verifies Y"* is a claim, not evidence.
5. **On unfixable failure: block + report.** `mp milestone block <id> --reason`
   and escalate. Never fake completion, silently defer, or mark a step done that
   was not done.
6. **Review is mandatory for milestones.** `complete` is reachable only via
   `mp reviews pass` (an independent pass). Tracks skip this — see the risk-tiering
   table in §3.3c.

---

### 3.4 Query and report

Use when the user asks for status, summaries, or what's next.

| User intent | Command |
|-------------|---------|
| Overall status | `mp status` → summarize (or launch `raul` for the user) |
| Done / pending / in progress / partial | `mp list milestones --filter <preset>` |
| Needs grooming | `mp list milestones --filter grooming` |
| Pending milestones | `mp list milestones --spec-status ready,interview` |
| What's next? | `mp next` or `mp path` |
| Full work queue | `mp path` |
| Do M4 before M3 | `mp path pin 04 --before 03` |
| What should we do with M03? | `mp milestone groom 03` |
| Steps for a milestone | `mp list steps --milestone <id>` |
| Park idea for later | `mp idea create ...` |
| Small bugfix | `mp track add bugfix ...` |
| Show one milestone | `mp show milestone <id>` (or launch `raul` for the user) |
| Archived items | `mp list archived` |
| Full validation | `mp validate` |

> For grooming, challenge, and step filters, see
> [`docs/mp/commands.md`](~/.agents/master-plan/docs/mp/commands.md).

---

### 3.5 Defer a topic (idea)

Use when the user defers a topic **without** asking for a plan or fix:

- “Let’s handle this later”
- “Park this idea”
- “Remind me about the installer approach”

```text
mp idea create --title "App installer design" --body "..." --tags installer
mp validate
```

Do **not** create a milestone, track item, or backlog entry unless the user wants formal deferred scope or actionable work.

**Later:**
```text
mp idea list
mp idea promote ID-01 --to-milestone    # or --to-backlog | --to-track bugfix
```

---

### 3.6 Defer scope (backlog)

Use during **grooming** when scope is formally deferred from a milestone or charter:

```text
mp backlog add --desc "..." --priority medium --source planning
mp validate
```

---

### 3.7 Track item (lightweight bugfix/tweak)

Use for **small, independent fixes** that do not need a full milestone spec.

**Choose track when:**
- Effort is hours, not days
- No new feature behavior — correctness, polish, or small improvement
- Work can be done and verified in one pass
- Item does not need scenarios, FR-XX, or design decisions

**Choose milestone when:**
- New feature or significant behavior change
- Multiple work packages or cross-cutting design
- Needs interview, spec approval, and acceptance criteria

```text
1. mp interview checklist --checklist-type track-item --kind bugfix
2. Ask 1–3 quick questions if fields missing
3. mp track add bugfix --title "..." --problem "..." --verification "..."
4. mp track start bugfix BF-01              # → in-progress
5. Implement fix
6. mp track done bugfix BF-01 --evidence "..."
7. mp validate
```

**Blocked?** Tell the user; do not call `track done`. **Cancel?** `mp track cancel bugfix BF-01`.

If a track item grows large: `mp track promote bugfix BF-03 --to-milestone`

---

### 3.8 Bootstrap (first time only)

```text
mp init
mp doctor
mp brief todo    # first agent task with user
# ... fill brief (see §3.0) ...
mp brief done
```

If charter is empty after the brief, run a charter interview
(`mp interview checklist --checklist-type charter`) before planning milestones.

Use `mp brief list` as context — do not re-ask what the brief already covers.

---

## 4. Lifecycle Gates

`mp validate` (and the mutation commands) enforce these. The full state machine —
`draft → groomed → approved → in-progress → done → self-reviewed → reviewed → complete`,
plus `remediation` and the blocked/deferred overlays — lives in
[`docs/milestone-lifecycle/`](~/.agents/master-plan/docs/milestone-lifecycle/).

| Gate | Rule |
|------|------|
| No code before approved | `lifecycle` must be `approved` before `in-progress` |
| No open questions at approval | Resolve all `Q-XX` before `approve` |
| Min 2 out-of-scope items | Required before `groomed` (`set-spec-status review`) |
| Min 1 acceptance criterion | Required before `groomed` |
| No impl plan before approval | WPs/steps only after spec approval |
| All steps done before verify | Every step `done` before §3.3b self-verification |
| Evidence is test output | `criterion pass` evidence records test name + exit, not prose |
| Honest ACs to leave the review queue | All `AC-XX` passed with non-prose evidence (or blocked with reason) |
| `complete` requires independent review | Milestones reach `complete` only via `mp reviews pass`, never `complete` alone |
| Tracks skip external review | Track items go `start → done` (low blast radius) |

If `mp validate` fails, fix via `mp` commands. Do not patch files manually.

---

## 5. Output Conventions

| Audience | Command | Notes |
|----------|---------|-------|
| Agent reads | `mp <cmd>` | JSON default — **no** `--format json` |
| Field projection | `mp <cmd> --fields 'a.b,c[].d'` | Prefer over jq |
| Health rollup | `mp show milestone <id> --summary` | Prefer over jq counts |
| User display | launch `raul`, or summarize JSON | raul is TUI-only |
| Debug source | `--format raw` on `show milestone` / `graph` | Agent-only escape hatch (verbatim on-disk JSON or DOT) |

- **Talking to the user:** summarize JSON in clear prose, or defer to `raul`.
- **Never dump raw plan JSON** unless the user explicitly asks.
- **Never pipe `mp` to jq/python** for plan reads when `--fields` or `--summary` exists.

---

## 6. Triggers

Activate this workflow when the user:

- Mentions master plan, milestone, roadmap, spec, backlog, or ideas
- Mentions “later”, “park this”, “remind me about”, or defers a topic without planning it
- Uses `/mp` or CPD skills (`mp-flow` / `mp-runner` / `mp-coordinator`)
- Asks "what's next?", "plan X", "break down X", or "what's the status?"

---

## 7. References

- **Agent guide (orientation + workflows):** [`docs/agent-guide/README.md`](~/.agents/master-plan/docs/agent-guide/README.md)
- **Lifecycle state machine + gates:** [`docs/milestone-lifecycle/`](~/.agents/master-plan/docs/milestone-lifecycle/)
- **Milestone data model & fields:** [`docs/milestone-details/`](~/.agents/master-plan/docs/milestone-details/)
- **Command reference:** [`docs/mp/commands.md`](~/.agents/master-plan/docs/mp/commands.md)
- **Getting started / walkthrough:** [`docs/mp/getting-started.md`](~/.agents/master-plan/docs/mp/getting-started.md)
- Global CPD skills: `~/.agents/skills/mp-flow/`, `mp-runner/`, `mp-coordinator/`
