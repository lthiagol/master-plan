# Project config reference

`mp` reads a project config that lives at the root of the plan directory
(`config.json` on disk, mirrored as `config.toml`-style sections in `mp config
show`). It is created by `mp init` from a profile template and edited with
`mp config set`.

```bash
mp config show                 # full config as JSON
mp config get <dotted.key>     # one value
mp config set <dotted.key> <value>
mp config validate             # { ok, errors[], warnings[] }
```

## Profiles

The profile is chosen at `mp init --profile <full|hybrid|session>` and stored as
`workflow.profile`. It determines the plan location, gate strictness, default
artifacts, and the "next" preference.

| | `full` | `hybrid` / `session` |
|---|---|---|
| Plan location | `master-plan/` (committed) | `.mp/` (gitignored) |
| Gates | `full` (min 2 out-of-scope, min 1 AC) | `relaxed` (min 1 out-of-scope, min 1 AC) |
| Milestone mode | `true` (file-per-milestone) | `session` |
| Artifacts seeded | brief, backlog, ideas, decisions, annotations | ideas, decisions, annotations |
| `next.prefer` | `milestone` | `track` |
| Session branch binding | — | `auto_bind_branch`, `archive_on_merge` |

## Sections

### `[workflow]`

| Key | Type | Effect |
|-----|------|--------|
| `workflow.profile` | `full` \| `hybrid` \| `session` | Plan shape (above) |
| `workflow.artifacts.<x>` | bool | Enable/disable artifact kinds: `brief`, `backlog`, `tracks`, `ideas`, `decisions`, `milestones` (`true`/`false`/`"session"`) |
| `workflow.plan.in_repo` | bool | Is the plan committed to the repo? |
| `workflow.plan.location` | string | Plan directory name (`master-plan` / `.mp`) |
| `workflow.gates.strictness` | `full` \| `relaxed` | Gate strictness level |
| `workflow.session.auto_bind_branch` | bool | Bind sessions to the current git branch |
| `workflow.session.archive_on_merge` | bool | Archive sessions on branch merge |
| `workflow.session.focus` | string | Pinned session focus |
| `workflow.steps.code_review` | bool | Require code review step |

### `[ui]` — raul preferences (see also [`../raul/settings.md`](../raul/settings.md))

| Key | Values | Default |
|-----|--------|---------|
| `ui.color` | `true` / `false` | `true` |
| `ui.icons` | `unicode` / `ascii` / `none` | `unicode` |
| `ui.theme` | `latte`/`frappe`/`macchiato`/`mocha`/`dracula`/`monochrome` | `mocha` |
| `ui.hide_done` | `true` / `false` | `false` |

### `[keybinds]` — raul bindings

Each action maps to a combo string or list of combo strings. See
[`../raul/keybinds.md`](../raul/keybinds.md).

```bash
mp config set keybinds.quit "q"
mp config set keybinds.next_lane '["Right", "l", "Tab"]'
```

### `[archive]`

| Key | Default | Effect |
|-----|---------|--------|
| `archive.auto_purge_days` | `0` (off) | Purge archived items older than N days |
| `archive.archive_on_milestone_delete` | `true` | Archive before deleting a milestone |
| `archive.archive_on_track_cancel` | `true` | Archive before cancelling a track |

### `[git]`

| Key | Default | Effect |
|-----|---------|--------|
| `git.auto_commit` | `false` | Auto-commit plan changes |
| `git.auto_push` | `false` | Auto-push after commit |
| `git.commit_on_milestone_complete` | `false` | Commit when a milestone completes |
| `git.commit_message_template` | string | Template for commit messages |

### `[next]`

| Key | Default | Effect |
|-----|---------|--------|
| `next.prefer` | `milestone` (`full`) / `track` (`hybrid`) | What `mp next` surfaces first |

### `[planning]` — gate minimums

| Key | `full` | `hybrid` | Effect |
|-----|--------|----------|--------|
| `planning.require_min_out_of_scope` | `2` | `1` | Min `scope.out_of_scope` items before review |
| `planning.require_min_acceptance_criteria` | `1` | `1` | Min ACs before review |

### `[display]`

| Key | Effect |
|-----|--------|
| `display.milestone_prefix` | Prefix shown before milestone ids (e.g. `M`) |

### `[sort]` — per-lane sort preferences (raul)

Written by raul's sort-rebind menu (`Shift+S`) on confirm; `mp` itself never
reads it (the TUI surface is the only consumer). One key per lane.

| Key | Effect |
|-----|--------|
| `sort.<lane>` | The sort key bound to a lane, e.g. `sort.milestones = "lifecycle"` |

```bash
mp config set sort.milestones lifecycle
```

### `[agent]` — `mp watch` and automation

| Key | Default | Effect |
|-----|---------|--------|
| `agent.runner.{…}` | — | Per-role agent config consumed by `mp watch` |
| `agent.coordinator.{…}` | — | Per-role agent config consumed by `mp watch` |
| `agent.automation.commit_after_execute` | `false` | Runner commits after `milestone complete` |
| `agent.automation.push_after_review` | `false` | Coordinator pushes the runner's branch at review |
| `agent.automation.branch_strategy` | `current` | `per-milestone` \| `current` \| `none` |
| `agent.automation.auto_remediate` | `none` | Remediation policy at review boundaries |

The `[agent]` section is optional end-to-end — `mp` and `raul` build and run
without it. Only `mp watch` requires it.

### `[review]` — hunk export

| Key | Default | Effect |
|-----|---------|--------|
| `review.hunk` | `false` | Enable `mp reviews hunk <id>` export |
| `review.hunk_author` | `mp` | Author string in exported annotations |

## Validation

```bash
mp config validate
```

Reports `{ ok, errors[{field,message}], warnings[{field,message}] }`. A project
with an invalid config still loads (lenient read), but the offending keys fall
back to defaults — run `validate` after editing to catch typos.
