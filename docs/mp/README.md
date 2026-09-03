# mp — the Master Plan CLI

`mp` is the single tool that creates, mutates, and reads a Master Plan project.
It is designed first for **agents and scripts** (its default output is JSON), but
humans use it too — typically piped into `jq` or driven by the `raul` UI.

## The 30-second model

```
mp init   →   plan files (JSON)   →   mp <read|write>   →   raul (humans look)
```

- A **plan** is a directory of JSON files owned by `mp` (`master-plan/` for the
  `full` profile, `.mp/` for `hybrid`/`session`).
- **You never edit those files by hand.** Every read and every write goes through
  `mp`, which keeps them valid and leaves an audit trail.
- Read commands emit **JSON by default** so they are trivial to parse and project
  with `--fields`.

## Global flags (every command)

| Flag | Effect |
|------|--------|
| `--project-root <path>` | Operate on this project root instead of the cwd |
| `--plan-dir <path>` | Operate on this plan directory instead of the discovered one |
| `--format json\|raw` | Output format. `json` is the default. `raw` is a debug escape hatch: verbatim on-disk JSON for `show`, GraphViz DOT for `graph` |
| `--fields a.b,c[].d` | Project specific dotted JSON paths. Unknown paths are a hard error. Applies to read commands: `show`, `list`, `status`, `validate`, `reviews` |
| `--quiet` | Suppress non-essential output |
| `--verbose` | More detail |

## Output conventions

- **Reads are JSON by default.** You almost never need `--format json`.
- **Project, don't `jq`.** Prefer `--fields` and `--summary` over piping to
  external tools — `mp` validates the projection server-side.
  - `mp show milestone 42 --fields 'milestone.lifecycle,steps[].status'`
  - `mp show milestone 42 --summary` (health rollup)
  - `mp validate --summary` (ok/error counts + warnings grouped by code)
- **`--format raw`** is for debugging on-disk shape, not for regular use.

## Finding your way

| You want to… | Go to |
|---|---|
| Install and start a project | [`getting-started.md`](./getting-started.md) |
| See every command group and what it does | [`commands.md`](./commands.md) |
| Understand project config and profiles | [`config.md`](./config.md) |
| Know how milestones move through their lifecycle | [`../milestone-lifecycle/`](../milestone-lifecycle/) |
| Know what a milestone document contains | [`../milestone-details/`](../milestone-details/) |

## The commands at a glance

`mp` is organized into top-level command groups. The full surface is in
[`commands.md`](./commands.md); the high-level shape:

- **Plan setup:** `init`, `install`, `uninstall`, `doctor`, `migrate`, `sync`
- **Milestones:** `milestone …` (create, approve, update, complete, steps, ACs, WPs, design decisions, challenge, bulk…)
- **Lightweight intake:** `track …`, `idea …`, `backlog …`
- **Discovery & reads:** `status`, `next`, `path`, `list …`, `show …`, `search`, `inbox`, `graph`
- **Lifecycle & review:** `execution …`, `reviews …`
- **Spec authoring aids:** `interview …`, `plan …`, `specs …`, `brief …`
- **Cross-cutting:** `config …`, `note …`, `decision …`, `annotation …`, `changelog …`, `release …`, `git …`, `scratch …`, `digest`, `autopilot`

## Plan zone vs. code zone

Two zones, one rule:

| Zone | What | Rule |
|------|------|------|
| **Plan** | The plan directory (all JSON files) | All reads and writes via `mp`. Never hand-edit. |
| **Code** | The project's own source/config | Normal editing. |

Touching a plan JSON file with a text editor is the one thing you must not do —
`mp validate` will not always catch the resulting drift, and the audit trail
loses the change.
