# Agent Readiness — What Works in Rust Today

**Read this before agents run commands.** Prevents calling unimplemented CLI surface.

**Source of truth:** [AGENTS.md](../../../AGENTS.md) (session start). This doc is the command matrix.

Last aligned: 2026-07-03 (M97 — read ergonomics: `--fields` parity on `path`/`inbox`, `--summary` on `status`/`reviews finding list`/`reviews lifecycle`, `note add --body @file/@-`, `design_decisions` create round-trip).

---

## Agent output contract (read this first)

| Task | Do | Don't |
|------|-----|-------|
| Read plan state | `mp <cmd>` (JSON is default) | `mp <cmd> --format json` (redundant since M76) |
| Slice a few fields | `mp <cmd> --fields '…'` (all read commands) | `mp … \| jq` |
| Project by stable id | `mp show milestone <id> --fields 'acceptance_criteria[AC-03]'` | `mp show … \| jq '.acceptance_criteria[] \| select(.id==…)'` |
| Read one AC | `mp milestone ac show <id> <ac-id>` | `mp show milestone … \| jq '.acceptance_criteria[] \| select(.id==…)'` |
| Read one step | `mp milestone step show <id> <step-id>` | `mp show milestone … \| jq '.steps[] \| select(.id==…)'` |
| Find plan content | `mp search <query> [--type ac\|step\|wp\|…] [--include object]` | `grep master-plan/` or `rg master-plan/` |
| Update one AC | `mp milestone ac update <id> <ac-id> --description …` | `mp milestone update --json '{"acceptance_criteria":[…]}'` |
| Update one step | `mp milestone step update <id> <step-id> --…` | `mp milestone update --json '{"steps":[…]}'` |
| Remove one AC / step / WP | `mp milestone ac/step/wp remove <id> <id>` | `mp milestone update --json` with array rebuild |
| Remediation health | `mp show milestone <id> --summary` | jq count aggregates |
| Headline plan metrics | `mp status --summary` | full `mp status` payload |
| Finding counts only | `mp reviews finding list <id> --summary` | jq on findings array |
| Lifecycle bucket counts | `mp reviews lifecycle --summary` | jq grouping lifecycle |
| Validate rollup | `mp validate --summary` | jq on validate output |
| Note body from file/stdin | `mp note add --title X --body @file.md` or `--body @-` | shell-quoted inline markdown |
| Open findings | `mp reviews finding list <id> --open` | `milestone update --json` with findings |
| Batch resolve | `mp reviews finding resolve <id> --all` | hand-edit finding status in JSON |
| Writes | `mp <cmd> --json @-` or `--file` | `sed` / editor on plan dir |
| Human display | `raul` or summarize JSON | `--format human` (removed v2) |
| Debug on-disk JSON | `mp show milestone <id> --format raw` | dump raw JSON to users |

**Fragment-first rule (M93):** Agents edit by id (`ac/step/wp show|add|update|remove`)
and read small JSON slices. `mp milestone update --json` rejects `acceptance_criteria`
and `steps` arrays unless `--replace-arrays` is passed (migration escape hatch only).
See the [Fragment operations](#fragment-operations-m93) section below.

**Loop guard:** same write succeeds but state unchanged → stop after 2 tries, read `--help`,
check this doc, then `mp milestone block` + escalate.

**Installed copies:** run `make install` after pulling doc changes so `~/.agents/skills/` and
project `mp init` templates match the repo.

---

## Fragment operations (M93)

Agents plan by issuing small, id-targeted verbs. Never assemble a milestone
document; never replace whole arrays via `milestone update`.

### Reads — return one fragment (no full milestone load)

```bash
# Single acceptance criterion (id, description, verification, status, evidence).
mp milestone ac show <id> <AC-id>
# Same call under the legacy namespace:
mp milestone criterion show <id> <AC-id>

# All ACs for a milestone as a fragment-only array.
mp milestone ac list <id>

# Single step (id, action, done_when, tests, covers_ac, work_package, status, …).
mp milestone step show <id> <step-id>
```

### Writes — edit one fragment; returns only the changed fragment

```bash
# Add or update one acceptance criterion.
mp milestone criterion add <id> --description "…" --verification "…"
mp milestone ac update <id> <AC-id> --description "…" --verification "…"

# Remove one AC. Refuses when a step `covers_ac` includes it.
mp milestone ac remove <id> <AC-id>

# Remove one step. Refuses when another step's `depends_on_steps` includes it,
# or when split children (S3.1, S3.2) still exist under it.
mp milestone step remove <id> <step-id>

# Remove one work package. Refuses when any step references it via `work_package`.
mp milestone wp remove <id> <wp-id>
```

### Projection — stable-id selectors in `--fields`

```bash
# Single AC by id (returns {"acceptance_criteria": {"AC-03": {…}}}).
mp show milestone <id> --fields 'acceptance_criteria[AC-03]'

# Single step by outline id (returns {"steps": {"S4": {…}}}).
mp show milestone <id> --fields 'steps[S4]'

# Numeric index still works (backward compat with M79).
mp show milestone <id> --fields 'acceptance_criteria[0]'

# Mix multiple selectors in one query.
mp show milestone <id> --fields 'acceptance_criteria[AC-02],steps[S1],milestone.id'
```

### Anti-pattern: rebuild document arrays via `milestone update`

```bash
# ❌ Default-rejected (M93 AC-08). Error points to fragment commands.
mp milestone update <id> --json '{"acceptance_criteria":[…]}'
# stderr: 'acceptance_criteria' is a guarded document array (M93 AC-08);
#   use mp milestone ac … commands, or pass --replace-arrays to opt into
#   whole-array replacement (migration only)

# ⚠️ Migration escape hatch only — do NOT use in normal grooming/execute flows.
mp milestone update <id> --json '{"acceptance_criteria":[…]}' --replace-arrays
```

### Read ergonomics (M97)

`--fields` is honored by **every** read command (`status`, `path`, `inbox`,
`list milestones`, `reviews finding list`, `reviews lifecycle`, `show milestone`).
An **unknown path is a hard error** everywhere (exit non-zero) — it is never
silently ignored. Use it to slice payloads instead of piping through `jq`.

`--summary` is available on:

| Command | Returns |
|---------|---------|
| `mp status --summary` | headline metrics only (no `suggested_path` / path block) |
| `mp reviews finding list <id> --summary` | `{open, fixed, total}` counts, no findings array |
| `mp reviews lifecycle --summary` | `[{review_state, count}]` buckets only, no milestone detail |
| `mp show milestone <id> --summary` | step/AC/finding health rollup |
| `mp validate --summary` | ok/error counts + warnings grouped by code |

`mp note add` accepts `--body @<path>` (read body from file) and `--body @-`
(read from stdin) to avoid shell-backtick mangling of inline markdown.
`mp milestone create --json @file` round-trips `design_decisions` (mirroring
`acceptance_criteria`); `mp milestone update --json` stays fragment-first and
rejects `design_decisions` with a hint to `mp milestone design-decision add`.

---

## Legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Implemented in `crates/mp` |
| ⚠️ | Partial / simplified behavior |
| 📋 | Documented only — **do not call** until shipped |

---

## Install & bootstrap

| Command | Status | Notes |
|---------|--------|-------|
| `mp install` / `mp uninstall` | ✅ | Mirrors `make install`; `--harness`, `--dev` |
| `mp init` | ✅ | `--profile`, `--from-repo`, `--with-cursor-skill`, `--with-opencode-skill` |
| `mp doctor` | ✅ | Toolkit + `--project` (profile, harness, brownfield detect). v1.3: templates/schemas **embedded** — green with no asset files on disk |
| `mp brief *` | ✅ | todo/list/show/edit/add/rm/skip/done/**promote**/**reopen** |
| `mp brief reopen` | ✅ | Resets `brief.status` + `planning_phase` |

---

## Core query & validation

| Command | Status | Notes |
|---------|--------|-------|
| `mp validate` | ✅ | G1–G13, T1–T2, W01 index drift (after `mp sync`) |
| `mp sync` | ✅ | Rebuilds `[[milestones]]` index in `plan.json` |
| `mp status` | ✅ | `inbox_count`, `blockers`, `suggested_path`, execution |
| `mp digest` | ✅ | `--since 7d` stakeholder summary |
| `mp export` | ✅ | Markdown views → `exports/` |
| `mp git status/suggest-message/commit` | ✅ | Plan dir only; `pushed` when `git.auto_push` |

---

## Milestones & steps

| Command | Status | Notes |
|---------|--------|-------|
| `milestone create/update/approve` | ✅ | JSON input |
| `milestone set-spec-status/set-status` | ✅ | Gates enforced; bulk via `bulk set-spec-status` |
| `migrate lifecycle` | ✅ | M100 — bulk-clear `spec_status` + `execution_status` after `effective_lifecycle()` derives a value. `--dry-run` previews; idempotent. Reads `mp-model::effective_lifecycle`. |
| `milestone set-priority` | ✅ | Bulk via `bulk set-priority` — **never shell-for-loop over single-id `set-priority`** |
| `milestone bulk set-priority/set-spec-status` | ✅ | M94 — `--ids …, …` and/or `--where 'field==value'`; per-id results; `--dry-run` |
| `milestone bulk depends-on add/remove` | ✅ | M94 — cycle detection per-id; **never shell-for-loop over single-id `update` for multi-id dep changes** |
| `milestone block/unblock/defer/reopen` | ✅ | `reopen --reason` |
| `milestone complete` | ✅ | Delta merge G11–G13; **AC verify gate (M30)** refuses `done` on failing runnable ACs; `--force` bypass; optional git commit via config |
| `milestone verify` | ✅ | M30 — re-runs runnable AC verifications; exits non-zero on failure |
| `migrate manual-prefix-backfill` | ✅ | M177 — prefix prose AC verifications with `manual: `; `--dry-run` previews, `--yes` applies; idempotent |
| `edit strip-deferred-reason` | ✅ | M177 — clear stale `deferred_reason` when `deferred: false`; `--dry-run` / `--yes` |
| `milestone delete` | ✅ | `--force` required for in-progress |
| `milestone decompose/plan/split` | ✅ | `plan` scaffolds WPs + closure WP |
| `milestone criterion pass/fail/add` | ✅ | |
| `milestone question add/resolve` | ✅ | `needs_clarification` Q-XX |
| `milestone design-decision add` | ✅ | Suppresses W42 warning |
| `step add/update/set-status/done/fail/split` | ✅ | `step fail --evidence` |
| `wp add/update` | ✅ | |
| `interview checklist` | ✅ | `--checklist-type brief` / `implementation-plan` |
| `purge archived` | ✅ | `--type track-item`, honors `--older-than` |
| `list milestones/steps` | ✅ | `--filter` presets on milestones |
| `show milestone` | ✅ | Includes `execution_ready` |

---

## Reviews & findings

| Command | Status | Notes |
|---------|--------|-------|
| `mp reviews status` | ✅ | Unified spec-review + execution-review queues; `suggested_next` action |
| `mp reviews pending` | ✅ | Queue for independent review; `--summary` for steps/findings rollup; `pending_review_count` in `mp status` |
| `mp reviews pass <id>` | ✅ | Required before milestone is truly shipped; `--verdict`, `--reviewer`, `--notes` |
| `mp reviews list/show` | ✅ | Review history |
| `mp reviews sweep` | ✅ | Triage pending queue by risk bucket |
| `mp reviews lifecycle` | ✅ | Rollup by `review_state` (pending-review, open-findings, remediated, reviewed-clean) |
| `mp reviews finding add` | ✅ | Reviewer adds structured finding (`--severity`, `--category`, `--desc`) |
| `mp reviews finding resolve <id> <F-XX>` | ✅ | Sets status `fixed`, stamps `resolved` date; optional `--commit` |
| `mp reviews finding resolve <id> --all` | ✅ | Resolve every open finding in one command |
| `mp reviews finding list <id>` | ✅ | `--open` filters; includes `summary` counts |
| `show milestone <id> --summary` | ✅ | Health rollup: step/AC/finding counts, review_state, force_bypassed |

Finding statuses: `open` → `fixed`. Do **not** use `mp milestone update` for findings.

Prefer `mp show milestone <id> --summary` over `mp show … | jq` for health rollups
(step/AC/finding counts, review_state, force-bypass flag).

---

## Bulk milestone metadata (M94)

For multi-id plan edits — `set-priority`, `set-spec-status`, `depends-on add`,
`depends-on remove` — use `mp milestone bulk …` instead of shell-for-loops over
single-id commands. Same filter syntax as `list milestones` (`--where
'field==value'`); `--ids` and `--where` are unioned; `--dry-run` previews
without writing.

```bash
# Bump three milestones in one command:
mp milestone bulk set-priority --ids 82,92,93 --priority high

# Bump every milestone currently in spec_status=review:
mp milestone bulk set-priority --where 'spec_status==review' --priority high

# Append the same dependency to several milestones (cycle-checked per id):
mp milestone bulk depends-on add --ids 91,92 --depends-on 87

# Preview without writing:
mp milestone bulk set-spec-status --where 'priority==high' --status review --dry-run
```

Response shape: `{ ok, operation, dry_run, target_count, succeeded, failed,
results: [{ id, ok, before?, after?, error? }] }`. On partial failure, later
ids still process and exit code is `2`.

**Anti-pattern (forbidden):**

```bash
# ✗ Never shell-loop over single-id commands for multi-id writes:
for id in 82 92 93; do mp milestone set-priority "$id" high; done
for id in 91 92;   do mp milestone update --json "{\"id\":\"$id\",...}"; done
```

The single-id path is still available for one-off writes; `bulk` is for
fan-out. See [MP-COMMANDS](../../concepts/06%20-%20Reference/MP-COMMANDS.md#mp-milestone-bulk-) for full syntax.

Re-complete to refresh stale force-bypass evidence: `mp milestone complete <id> --evidence "<cmd> exit 0"`.

---

## Annotations (review / approval)

| Command | Status | Notes |
|---------|--------|-------|
| `annotation create/list/show` | ✅ | `kind`: review-request, approval-request, change-suggestion, break-down, note |
| `annotation update/addressed/resolve/reopen/remove` | ✅ | Lifecycle: open → addressed → resolved → (reopen) |
| Gate G14 | ✅ | Open approval-request blocks `spec_status: ready` |

---

## Path, execution & PM surface

| Command | Status | Notes |
|---------|--------|-------|
| `mp path` (+ pin/focus) | ✅ | `path_engine` with adoption order |
| `mp next` | ✅ | Uses path engine |
| `mp graph` (+ explain) | ✅ | |
| `mp inbox` / `mp hygiene` | ✅ | `--filter` presets: `spec-review`, `execution-review`, `review` |
| `mp groom` | ✅ | |
| `mp plan gaps/coverage` | ✅ | |
| `mp execution check/handoff/pause/status` | ✅ | |
| `mp challenge *` | ✅ | `reviews/challenges/` |
| `mp path suggest` | ✅ | Pin/focus heuristics; user confirms with `path pin` |

---

## Tracks, ideas, backlog, sessions

| Command | Status | Notes |
|---------|--------|-------|
| `track list/show/add/start/done/cancel` | ✅ | |
| `track promote` | ✅ | `--to-milestone`; archives item |
| `track archive milestone/track-item` | ✅ | Soft-delete to `archive/` |
| `track restore archived` | ✅ | `--kind bugfix` for track items |
| `track purge archived` | ⚠️ | Milestone only; `--confirm` required |
| `idea *` (+ promote, dup-check warn) | ✅ | |
| `note add` | ✅ | Meeting capture → ideas (`source=meeting`); `--body @file` / `@-` for markdown; `--to idea` (default) |
| `backlog *` (+ promote) | ✅ | |
| `session *` (+ export, promote, **focus**, **unfocus**) | ✅ | |
| `brief promote` | ✅ | `--to-idea` / `--to-backlog` |

---

## Charter, config, brownfield

| Command | Status | Notes |
|---------|--------|-------|
| `plan show/set/goals/nongoals` | ✅ | |
| `metrics show/set` | ✅ | |
| `backlog add/show/resolve` | ✅ | |
| `decision add/list/remove` | ✅ | |
| `search <query>` | ✅ | M95 — `--type ac\|step\|wp\|title\|milestone\|idea\|backlog\|track\|decision`; `--include object` embeds full fragment; `--group-by milestone`; hits carry `suggested_action` mapping to M93 fragment commands |
| `config show/get/set` | ✅ | `workflow.*`, `git.*` |
| `specs list/show/init` | ✅ | |
| `skill context` | ✅ | Agent-context report for session start |
| `brownfield scan` | ✅ | |
| `delta rebase` | ✅ | |

---

## Adoption profiles

| Feature | Status | Notes |
|---------|--------|-------|
| `mp init --profile full/hybrid/session` | ✅ | |
| `[workflow]` config parse | ✅ | Plan dir auto-resolve |
| `mp init --from-repo` | ✅ | Brownfield bootstrap |
| Session layout | ✅ | `sessions/<id>/` per ADR-010; `mp session *` shipped (M08) |
| Session in path queue | ✅ | Hybrid `next-step` surfaces session milestones when tracks empty (M08) |
| One session per branch | ✅ | D-003 `auto_bind_branch`; resume/reject duplicate branch (M08) |
| `mp session focus` | ✅ | M13 — explicit focus when `auto_bind_branch=false` |

---

## Validate gates

| Gate | Status |
|------|--------|
| G1–G5 | ✅ |
| G6–G7 | ✅ |
| G8–G10 | ✅ (G10 strict when `workflow.gates.strictness = full`) |
| G11–G13 | ✅ (delta / brownfield) |
| T1–T2 | ✅ (tracks) |
| W01 | ✅ (index drift after sync) |

---

## Format support matrix

`mp` is agent-only — stdout is **JSON by default** (omit `--format` on reads).
`--format raw` is a debug escape hatch (verbatim on-disk JSON passthrough).

| Format | Audience | Availability |
|--------|----------|-------------|
| `json` | Agents | Every command (default when `--format` omitted) |
| `raw` | Debugging | `show milestone` / `track show` (verbatim on-disk JSON), `graph` (DOT) |

`mp` no longer supports `--format human`, `--format toml`, `--format markdown`,
or `--format pr-body` (removed in v2.0). Human display is handled by the
`raul` CLI/TUI.

## Git integration config

| Config key | Behavior |
|------------|----------|
| `git.commit_on_milestone_complete` | ✅ Auto `mp git commit` after `milestone complete` |
| `git.auto_commit` | ✅ Same hook (alias) |
| `git.auto_push` | ✅ After `mp git commit` when configured |

---

## raul TUI — Settings lane (M169)

| Key / action | Default | Behavior |
|--------------|---------|----------|
| `keybinds.open_settings` | **Ctrl-O** | Jump to the Settings lane (`Lane::Settings`) and load `mp config show`. No-op when already on Settings. |
| `s` (Settings lane only) | — | `Action::SettingsSave`: dry-run all staged edits, then batch-commit via `mp config set`. |
| `Enter` | — | Open inline edit for the focused key; Enter again stages the value (dry-run validated). |
| `Esc` | — | Cancel active edit only. Esc on the flat list is a no-op (no modal to close). |
| `Tab` / `Shift+Tab` | lane defaults | Cycle lanes like every other tab. During an active edit, Tab commits the edit instead of cycling. |
| Leave Settings lane | Tab / click another tab | Discards unstaged edits; re-entering reloads from `mp config show`. |

Footer on the Settings lane: `[Save (s)] [Cancel (Esc)]`.

---

## AC verification env flags (M177)

The complete/verify gate classifies each AC `verification` string as
`runnable` / `manual` / `empty`. Prose-shaped strings (parenthetical notes,
mid-string `+ rg` clauses, multi-clause `;` prose) auto-classify as manual.
Two env flags force every non-`manual:` string to manual (never shell-executed):

| Env | Behavior |
|-----|----------|
| `MP_VERIFY_NO_SHELL=1` | Strict mode: never shell-exec any non-`manual:` verification. Use when consuming untrusted plans. |
| `MP_VERIFY_DEFAULT_NO_SHELL=1` | Exact alias of `MP_VERIFY_NO_SHELL` (same runtime OR). Migration-era name for legacy plans that have not been rewritten via `mp migrate manual-prefix-backfill`. |

Either flag accepts `1` / `true` / `TRUE` / `yes` / `YES`. Prefer fixing
prose verifications with a `manual: ` prefix (write-time `prose_warning` on
`mp milestone ac update` / `step update` nudges this) over leaving the flag
on permanently.

---

## Safe agent modes today

| Goal | Safe approach |
|------|----------------|
| Read plan | ✅ `show`, `list`, `status`, `validate`, `path`, `graph` |
| Bootstrap brainstorm | ✅ `brief *` → `brief done` → charter |
| Plan milestone | ✅ `milestone create` → interview → `approve` → `plan`/`decompose` |
| Small fix | ✅ `track *` → `track promote` when it grows |
| Execute | ✅ `path` → `next-step` → `step done` → `milestone complete` |
| Review milestone | ✅ `reviews pending` → verify diff/tests → `reviews pass` (independent agent) |
| Remediate findings | ✅ `reviews finding resolve` after code fix + re-complete with evidence |
| Publish plan | ✅ `export`, `git commit` |
| Autonomous handoff | ⚠️ Use `execution check` + human review; see [EXECUTION-MODES.md](./EXECUTION-MODES.md) |

---

## References

- [AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md)
- [EXECUTION-MODES.md](./EXECUTION-MODES.md)
- [AGENTS.md](../AGENTS.md) — session start for this repo
- [PLANNING-STATUS.md](./PLANNING-STATUS.md) — phase map
