# Agent guide — driving the `mp` CLI

You are an agent operating on a Master Plan project. This file is your
orientation: read it once at session start. Everything else in this folder is a
**detail doc you load on demand** — do not read them all up front. Each section
below points you to the right detail file when you need it.

## What `mp` is, in one paragraph

`mp` owns a directory of JSON files (the **plan**) that describe a software
project's work — milestones, tracks, ideas, backlog, decisions. Your job is to
plan and execute well; `mp` owns format, storage, validation, and the audit
trail. You **never** edit plan files by hand. Every read and every write goes
through an `mp` command.

## The non-negotiables (memorize these)

1. **Plan zone is mediated.** Never hand-edit any file under the plan directory.
   Read and write only through `mp`.
2. **Reads are JSON by default.** Omit `--format json`. Use `--fields` and
   `--summary` instead of piping to `jq`.
3. **Fragment-first.** Edit one AC / step / WP at a time
   (`mp milestone ac|step|wp …`). Do **not** rebuild `acceptance_criteria` or
   `steps` arrays via `mp milestone update --json` — that path is rejected by
   default.
4. **Evidence is test output, not prose.** `--evidence` records what ran and its
   exit code (`cargo nextest run … exit 0`). "Test X verifies Y" is a claim, not
   evidence.
5. **Never complete on red.** `--force` and `--skip-verify` are recorded debt,
   not shortcuts. Prefer `mp milestone block` + escalation.
6. **Validate after every write.** `mp validate`. If a write returns success but
   the next read shows no change, stop after 2 attempts and escalate.

Full rules and rationale: [`core-principles.md`](./core-principles.md).

## The lifecycle at a glance

```
draft → groomed → approved → in-progress → done → (review) → complete
                       │          │           │         │
                       │          │           │         └─ self-reviewed → reviewed
                       │          │           └─ blocked / deferred (overlays)
                       └──────────┴─ spec gates: ACs (G3), out-of-scope (G4), deps done (G8)
```

- `complete` is **terminal**. The executor marks work complete after
  self-verifying; an independent `mp reviews pass` (a different session) is
  what makes it trustworthy before it's considered shipped.
- `done` means "finished, awaiting review" — it is not shipped.
- An open external finding auto-enters `remediation`; resolving the last one
  exits it.

## Orient yourself first

Before doing anything, read state — cheaply:

```bash
mp status                  # headline metrics + suggested next path
mp next                    # head of the default (execution) lane
mp path                    # full work queue across lanes
mp inbox                   # items needing a decision
```

When to read which doc: [`reading-state.md`](./reading-state.md).

## Which workflow are you in?

Pick the detail doc that matches your task. **Read only the one you need.**

| You are… | Read | The essence |
|----------|------|-------------|
| **Authoring a spec** (planning) | [`planning-specs.md`](./planning-specs.md) | `interview checklist` → `milestone create` (spec only) → resolve questions → `approve`. Gates G3/G4. |
| **Breaking work into steps** (phase 2) | [`decomposing.md`](./decomposing.md) | Only after approval. `decompose` → `wp add` → `step add` with `files`/`tests`/`done_when`/`covers_ac`. |
| **Executing steps** | [`executing.md`](./executing.md) | `execution check` → `set-status in-progress` → per-step loop → `criterion pass` with evidence → `complete`. |
| **Reviewing** (independent) | [`reviewing.md`](./reviewing.md) | `reviews pending` → `claim` → verify claims against diff+tests → `pass --verdict ok`, or file `finding` (→ remediation). |
| **Picking config / profiles** | [`config-and-profiles.md`](./config-and-profiles.md) | `full` vs `hybrid`, global flags, `--fields`/`--summary`, `config set`. |

## The five most common mistakes (avoid these)

1. **Loading the whole milestone when you need one field.** Use
   `mp show milestone <id> --fields 'milestone.lifecycle'` or
   `mp milestone ac show <id> <ac>`.
2. **Rebuilding arrays.** `ac`/`step`/`wp` fragment commands exist so you don't.
   `mp milestone update --json` with an `acceptance_criteria` array is rejected.
3. **Prose evidence.** "Tests pass" proves nothing. Name the command and the
   exit code.
4. **`--force` to skip a gate.** It records debt and blocks `complete`. Block +
   escalate instead.
5. **Grepping the plan directory.** Use `mp search <query> --include object` —
   it returns hits plus a `suggested_action` that points to the fragment command
   to edit them.

## Reference

- Full command surface: [`../mp/commands.md`](../mp/commands.md)
- Lifecycle detail: [`../milestone-lifecycle/`](../milestone-lifecycle/)
- Milestone anatomy: [`../milestone-details/`](../milestone-details/)
- When in doubt: `mp <command> --help`.
