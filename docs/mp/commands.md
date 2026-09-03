# `mp` command reference

Every command emits JSON by default and accepts the global flags (`--project-root`,
`--plan-dir`, `--format`, `--quiet`, `--verbose`, `--fields`). This reference
groups commands by purpose. Use `mp <command> --help` for the exhaustive flag
list of any single command.

---

## Plan setup & maintenance

| Command | What it does |
|---------|--------------|
| `mp init [--profile full\|hybrid\|session]` | Create a plan in the current project |
| `mp install [--harness …] [--skills …]` | Install the toolkit + skills into harness homes |
| `mp uninstall` | Remove installed toolkit/skills |
| `mp doctor [--project]` | Health check (config, plan, skills, harnesses) |
| `mp migrate --kinds [--dry-run]` | One-time migration: collapse tracks/ideas into backlog |
| `mp migrate manual-prefix-backfill [--yes]` | Prefix prose AC verifications with `manual:` |
| `mp sync` | Re-derive the plan index from on-disk files |
| `mp validate [--summary]` | Structural validation of the whole plan |
| `mp hygiene [--stale-days N]` | Report stale items (old, untouched) |

---

## Milestones — `mp milestone …`

The richest surface. A milestone is a full spec + implementation plan + review
record.

### Lifecycle & status

| Command | Effect |
|---------|--------|
| `milestone create [--title \| --file \| --json \| --example]` | Create a milestone (spec fields only) |
| `milestone update <id> [--json \| --file] [--verification …]` | Mutate spec fields. Arrays rejected by default (see `--replace-arrays`) |
| `milestone approve <id> [--dry-run]` | Mark the spec approved (lifecycle → approved) |
| `milestone set-spec-status <id> <status>` | Set legacy `spec_status` (drives `lifecycle`) |
| `milestone set-lifecycle <id> <status> [--dry-run]` | Set the canonical `lifecycle` directly |
| `milestone set-status <id> <status>` | Set `execution_status` |
| `milestone set-priority <id> urgent\|high\|normal\|low` | Set priority |
| `milestone set-target-version <id> <ver>` | Target release version |
| `milestone complete <id> [--evidence … \| --evidence-file …] [--force\|--skip-verify]` | Verify ACs + steps, then complete |
| `milestone verify <id>` | Run AC + step verification without completing |
| `milestone block <id> --reason "…" [--by …]` / `unblock <id>` | Block/unblock overlay |
| `milestone defer <id> --reason "…"` / `reopen <id>` | Defer/reopen overlay |
| `milestone split <id> --into N` | Split into N milestones |
| `milestone decompose <id>` / `plan <id>` | Scaffold work packages + steps |
| `milestone groom <id>` | Run grooming checks |
| `milestone trace <id>` | Show spec-to-implementation traceability |
| `milestone log <id>` | Read-only history (created/updated + `git log`) |
| `milestone dependents <id>` / `deps <id>` / `impact <id>` | Dependency graph queries |
| `milestone delete <id> [--force]` / `archive <id>` / `restore <id>` / `purge <id>` | Removal + archive lifecycle |

### Acceptance criteria — `mp milestone criterion …` (alias `ac`)

| Command | Effect |
|---------|--------|
| `ac add <id> --description "…" --verification "…"` | Add an AC |
| `ac show <id> <ac-id>` / `ac list <id>` | Read one/all ACs as small fragments |
| `ac update <id> <ac-id> [--description] [--verification] [--evidence]` | Mutate one AC |
| `ac bulk <id> --bulk FILE` | Apply many AC fragment updates from a JSON array |
| `ac pass <id> <ac-id> --evidence "…"` / `ac fail <id> <ac-id> --reason "…"` | Record verification |
| `ac remove <id> <ac-id>` | Remove (fails if a step covers it) |

### Steps — `mp milestone step …`

| Command | Effect |
|---------|--------|
| `step add <m> --wp WP1 [--id] [--after] --action "…" --files a.rs,b.rs --tests "…" --done-when "…" --covers-ac AC-01` | Add a step |
| `step show <m> <step>` | Read one step as a fragment |
| `step update <m> <step> [--action] [--files] [--tests] [--done-when] [--covers-ac] [--evidence]` | Mutate a step |
| `step set-status <m> <step> <status>` | `pending` / `in-progress` / `done` / `failed` |
| `step done <m> <step>` / `step fail <m> <step>` | Convenience transitions |
| `step claim <m> <step> --by … [--lease …]` / `step release <m> <step>` | Concurrency lease |
| `step split <m> <step>` / `step remove <m> <step>` | Restructure |

### Work packages — `mp milestone wp …`

| Command | Effect |
|---------|--------|
| `wp add <m> --name "…" [--goal] [--rollback] [--id]` | Add a WP |
| `wp update <m> <wp> [--name] [--goal] [--rollback]` | Mutate a WP |
| `wp remove <m> <wp>` | Remove (fails if a step references it) |

### Design decisions & open questions

| Command | Effect |
|---------|--------|
| `milestone design-decision add <id> --area … --decision … --rationale …` | Add a decision |
| `milestone design-decision update/remove <id> [--index\|--area] …` | Mutate/remove |
| `milestone question add <id> --text "…"` / `question resolve <id> <qid> --resolution "…"` | Open questions (must be resolved before approval) |

### Challenge (plan stress-test) — `mp milestone challenge …`

`start [--scope plan\|spec\|full]` · `audit` · `list` · `add` · `resolve` ·
`dismiss` · `done` — record findings against a spec or implementation plan and
track their resolution.

### Bulk operations — `mp milestone bulk …`

Multi-id metadata edits that resolve targets via `--ids a,b,c` and/or
`--where 'field==value'` (same filter syntax as `list milestones`).
`--dry-run` previews.

- `bulk set-priority` / `bulk set-spec-status` / `bulk set-lifecycle`
- `bulk depends-on add` / `bulk depends-on remove` (cycle-checked)

---

## Lightweight intake

### Tracks — `mp track …` (small bugfixes/tweaks)

| Command | Effect |
|---------|--------|
| `track list [--items]` | List track kinds + items |
| `track show <kind>` | Show one kind (e.g. `bugfix`, `tweak`) |
| `track add <kind> --title "…" --problem "…" --verification "…" [--step …]` | Add a track item |
| `track start <kind> <id>` / `track done <kind> <id> --evidence "…"` / `track cancel <kind> <id>` | Lifecycle |
| `track promote <kind> <id> --to-milestone` | Grow a track into a milestone |
| `track archive …` / `track restore …` / `track purge …` | Archive lifecycle |

### Ideas — `mp idea …`

`create` · `list` · `show` · `update` · `dismiss` · `archive` · `remove` ·
`promote --to-milestone|--to-backlog|--to-track` — vague "someday" notes with no
commitment.

### Backlog — `mp backlog …`

`add --desc "…" --priority low\|medium\|high` · `list` · `show` · `resolve` ·
`promote` — concrete but deferred work.

### Decisions — `mp decision …`

`add` · `list` · `remove` — project-level decision log (ADRs-lite).

---

## Discovery & reads

| Command | Effect |
|---------|--------|
| `mp status [--summary]` | Headline metrics + suggested next path |
| `mp overview [--summary]` | Consolidated project-health snapshot (status strip + bounded path/inbox/activity previews) |
| `mp activity [--limit N]` | Bounded read of the project activity journal (newest first) |
| `mp next [--lane blocked\|execution\|review\|grooming\|backlog] [--summary]` | The head item of a lane |
| `mp path [options] [pin\|unpin\|list-pins\|focus\|clear-focus\|suggest]` | The work queue + path controls |
| `mp list milestones\|tracks\|steps\|backlog\|decisions\|archived` | Typed lists (with `--filter`, `--where`, `--preset`, `--take`) |
| `mp show milestone <id>` / `show archived …` | Full document (project with `--fields`) |
| `mp search <query> [--type …] [--include object] [--limit N] [--group-by …]` | Full-text search across the plan; hits carry a `suggested_action` |
| `mp inbox [--filter actionable\|all\|spec-review\|execution-review\|review]` | Items needing a decision |
| `mp graph [--milestone …] [--with-steps] [--with-ac]` | Dependency graph (JSON; `--format raw` → DOT) |

`mp search` is the discovery primitive: prefer it over grepping plan files. Each
hit includes a `suggested_action` that maps to the matching fragment command, so
you can go from "found it" to "edit it" in two calls.

---

## Execution & review

### Execution — `mp execution …`

| Command | Effect |
|---------|--------|
| `execution check` | Are milestones execution-ready? Report blockers |
| `execution handoff` / `handoff-show` / `pause` / `status` / `report` | Execution handoff + status surface |

The `check` and `status` JSON both carry a `watch_readiness` block
(structured mirror of the autopilot precondition report — `herdr_on_path`,
`herdr_cli_shape`, `runner_config_present`, `coordinator_config_present`,
`log_path_writable`, `harness_auto_set`) and a top-level `can_handoff`
boolean. `can_handoff` is true only when **all** of: validate is clean,
at least one execution-ready milestone or track-pending work exists,
**and** `watch_readiness.ok` is true. The two surfaces (`execution check`,
`execution status`, `mp autopilot start`, `mp milestone handoff`) answer the
same go/no-go question so the readiness signal cannot drift between
them. The `ui.show_watch_tab` value is a separate
TUI preference and is reported under a distinct key.

### Reviews — `mp reviews …`

| Command | Effect |
|---------|--------|
| `reviews status` | Unified review discovery |
| `reviews pending [--summary] [--filter …] [--group-by …]` | The review queue |
| `reviews pass <id> --verdict ok\|changes-needed --reviewer … [--notes]` | Record a review verdict |
| `reviews pass --all --verdict ok --reviewer …` | Batch-resolve the queue |
| `reviews list` / `reviews show <id>` | Review records |
| `reviews sweep` (alias `triage`) | Classify the queue into risk buckets |
| `reviews lifecycle [--summary]` | Cross-project rollup by review state |
| `reviews finding add\|resolve\|list <id> …` | Structured findings (severity/category/anchor) |
| `reviews comment add\|list <id> …` | Threaded review comments |
| `reviews handoff <id> …` | Record a coordinator↔runner hand-off |
| `reviews l5-check <id>` | Evidence audit on hand-off records |
| `reviews hunk <id> [--file …] [--apply]` | Export findings/comments as hunk-compatible JSON |

A finding with an open *external* phase auto-enters the milestone into
`remediation`; resolving the last open finding auto-exits it. See
[`../milestone-lifecycle/review.md`](../milestone-lifecycle/review.md).

---

## Spec-authoring aids

| Command | Effect |
|---------|--------|
| `mp interview checklist --checklist-type milestone\|track-item\|charter\|brief [--id …]` | Suggested question rounds for a spec |
| `mp interview gaps [--id …]` | What's still missing from a spec |
| `mp plan show\|set\|goals\|nongoals\|principles\|gaps\|coverage\|infer-deps\|relocate\|diff\|metrics` | Project-level plan (charter) |
| `mp plan verify-ac <id>` / `verify-lint` | AC verification integrity pre-flight |
| `mp specs list\|show\|init\|delta` | Long-lived domain specs (brownfield) |
| `mp spec <sub>` | Condensed spec-review projection + since-last-approval diff |
| `mp brief todo\|list\|show\|edit\|add\|rm\|skip\|done\|reopen\|promote\|import` | Project brief (first-session context) |
| `mp brownfield scan` | Assist: scan existing code for a delta milestone |

---

## Cross-cutting

| Command | Effect |
|---------|--------|
| `mp config show\|get\|set\|validate` | Project config (`config.toml`) |
| `mp note add …` | Free-form plan note |
| `mp annotation create\|list\|show\|update\|resolve\|reopen\|remove\|addressed` | Annotations (incl. approval requests) |
| `mp changelog show\|add\|init\|generate` | Release changelog |
| `mp release list\|map\|show\|ship` | Release mapping |
| `mp git status\|suggest-message\|commit` | Git helpers (status-aware commit messages) |
| `mp scratch path` / `scratch new <label>` | In-repo scratch workspace (for big JSON payloads) |
| `mp digest [--since …] [--days N] [--markdown] [--out …]` | Activity digest |
| `mp watch <id> [<id>…] [--dry-run] [--resume\|--force]` | Drive milestones through their lifecycle by spawning runner/coordinator agents |
| `mp watch-control status\|stop\|output\|result` | Structured watch control-plane (machine-client read surface for a live or last run) |
| `mp review sidecar <id> --output <path> [--finding F-XX]` | Write a hunk-compatible agent-context sidecar of a milestone's findings + comments |
| `mp agent role` / `agent harness list\|start-command` | Agent role + harness command registry |
| `mp skill context` | Compact project context for an agent |
| `mp session start\|show\|list\|focus\|unfocus\|archive\|export\|promote` | Session lifecycle (hybrid profile) |

---

## Common flag/option patterns

- **Preview before write:** most mutating milestone commands take `--dry-run`
  (`approve`, `set-status`, `complete`, `set-lifecycle`, all `bulk …`, challenge
  `resolve`).
- **Evidence from a file:** `--evidence-file <path>` alongside `--evidence` for
  long values (`milestone complete`, `milestone update --verification-file`).
- **`--where` filters:** `list milestones`, `backlog list`, and `milestone bulk`
  share the `<field>==<value>` filter grammar.
- **Fragment reads:** `ac show`, `step show`, `ac list` return small fragments
  instead of the whole document — cheaper to load into agent context.
