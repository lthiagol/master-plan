# Config, profiles & flags

Quick reference for the global surface every `mp` command shares, plus the
project config and profile choices. Full detail in [`../mp/config.md`](../mp/config.md).

## Global flags (every command)

| Flag | Effect |
|------|--------|
| `--project-root <path>` | Operate on this project root instead of cwd |
| `--plan-dir <path>` | Operate on this plan directory instead of the discovered one |
| `--format json\|raw` | `json` (default) or `raw` (debug: verbatim JSON for `show`, DOT for `graph`) |
| `--fields a.b,c[].d` | Project dotted JSON paths on read commands. Unknown paths are a hard error. |
| `--quiet` / `--verbose` | Output volume |

You almost never type `--format json` — it's the default. Reach for `--fields`
and `--summary` before `jq`.

## Profiles

Chosen at `mp init --profile <full|hybrid|session>`, stored as
`workflow.profile`.

| | `full` (default) | `hybrid` / `session` |
|---|---|---|
| Plan location | `master-plan/` (committed) | `.mp/` (gitignored) |
| Gate strictness | `full` (min 2 out-of-scope) | `relaxed` (min 1 out-of-scope) |
| Milestone mode | one file per milestone | `session` |
| `next.prefer` | `milestone` | `track` |
| Brief / backlog | seeded | not seeded |

Rule of thumb: **`full`** for personal/open-source work where the plan lives in
the repo; **`hybrid`** for corporate repos where the plan must not be committed.

## Project config (`config.toml`)

```bash
mp config show                 # full config as JSON
mp config get <dotted.key>     # one value
mp config set <dotted.key> <value>
mp config validate             # { ok, errors[], warnings[] }
```

Dotted keys map to sections: `workflow.profile`, `ui.color`, `ui.theme`,
`planning.require_min_out_of_scope`, `review.hunk`, `keybinds.quit`, etc.

### Keys agents touch most

| Key | Effect |
|-----|--------|
| `planning.require_min_out_of_scope` | Gate G4 minimum (`full`=2, `hybrid`=1) |
| `planning.require_min_acceptance_criteria` | Gate G3 minimum (default 1) |
| `workflow.gates.strictness` | `full` \| `relaxed` |
| `review.hunk` | Enable `mp reviews hunk` export |
| `next.prefer` | `milestone` \| `track` |

### raul UI keys (humans; agents can set them)

`ui.color`, `ui.icons`, `ui.theme`, `ui.hide_done`, `keybinds.*`. See
[`../raul/settings.md`](../raul/settings.md).

## Projecting reads

| Need | Use |
|------|-----|
| One field | `mp show milestone 42 --fields 'milestone.lifecycle'` |
| Multiple | `--fields 'milestone.priority,steps[].status'` |
| Health rollup | `mp show milestone 42 --summary` |
| Validate rollup | `mp validate --summary` |
| List with filter | `mp list milestones --where 'lifecycle==done' --take 10` |

`--fields` validates paths server-side; a typo fails fast. Don't `jq` unless
`--fields` genuinely can't express the projection.

## Local dev vs global install

For hacking on the `mp` binary itself, point your shell at the freshly built
binary (it shadows the global install):

```bash
eval "$(make dev-env)"     # MP_HOME + PATH → target/release/mp
```

Don't leave it on for ordinary planning work — it shadows the installed product.
For normal project work, use the globally installed `mp` (after `make install`).

## Workspace payloads

Commands taking `--json @file` (`milestone create`, `milestone update`, `ac
bulk`) should read from a scratch path, not the repo root:

```bash
SCRATCH=$(mp scratch new m-update)
cat > "$SCRATCH/payload.json" <<'JSON'
{ "intent": { "outcome": "…" } }
JSON
mp milestone update 42 --json @"$SCRATCH/payload.json"
```

`mp scratch path` prints the scratch dir; `mp scratch new <label>` makes a unique
subdir. Avoid bare `/tmp/<random>.json` — cleanup timing has bitten long-running
commands.
