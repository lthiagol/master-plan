# Reading state

How to orient quickly and cheaply at the start of a task. Reads are JSON by
default; project with `--fields` / `--summary` instead of dumping whole
documents.

## Headline orientation

```bash
mp status                 # metrics + suggested next path (start here)
mp next                   # head of the default (execution) lane
mp inbox                  # items needing a decision
```

`mp status --summary` gives headline counts only (no path nesting). Use it when
you want a one-glance pulse.

## The work queue — lanes

Work is organized into five lanes. `mp path` shows them all; `mp next --lane X`
returns a specific lane's head.

| Lane | Contains |
|------|----------|
| `blocked` | Milestones with the `blocked` overlay |
| `execution` | In-progress milestones ready for the next step (the default for `mp next`) |
| `review` | Milestones in the review queue (`done`, awaiting review) |
| `grooming` | Specs being authored/refined (not yet approved) |
| `backlog` | Deferred/promotable work |

```bash
mp next --lane review              # what needs reviewing
mp next --lane grooming --summary  # head + counts, no nesting
mp path --lane execution           # full execution lane
mp path --all                      # every lane in one report
```

## Path controls

You can influence ordering:

```bash
mp path pin 42 --before 17         # do M42 before M17
mp path unpin 42
mp path list-pins
mp path focus 42                   # focus on one milestone
mp path clear-focus
mp path suggest                    # mp's suggested next action
```

Useful flags: `--horizon N` (how far ahead to look, default 50),
`--include-grooming`, `--prioritize-coverage`, `--no-ideas`, `--all`.

## Typed lists

```bash
mp list milestones [--filter <preset>] [--status …] [--spec-status …] [--where 'field==value'] [--take N]
mp list steps --milestone <id>
mp list tracks
mp list backlog
mp list decisions
mp list archived
```

Common presets for `list milestones --filter`: `done`, `pending`, `in-progress`,
`partial`, `grooming`, `spec-status ready,interview`. `--where` uses the shared
`<field>==<value>` grammar (same one `milestone bulk` uses).

## Reading one milestone

Load only what you need:

```bash
mp show milestone 42                                    # whole document
mp show milestone 42 --summary                          # health rollup
mp show milestone 42 --fields 'milestone.lifecycle'     # one field
mp show milestone 42 --fields 'milestone.priority,steps[].status'
```

For fragments smaller than the whole document:

```bash
mp milestone ac   list 42                  # all ACs as small fragments
mp milestone ac   show 42 AC-03            # one AC
mp milestone step show 42 S2               # one step
```

## Finding things — `mp search`

The discovery primitive. Prefer it over grepping plan files:

```bash
mp search "config validation"
mp search "rollback" --type step
mp search "AC-02" --include object          # embed the full matched fragment
mp search "auth" --group-by milestone --limit 50
```

`--type` filters by fragment kind (`ac`, `step`, `wp`, etc.). Every hit includes
a `suggested_action` naming the fragment command that edits it, so you go from
"found it" to "edit it" in two calls.

## Review discovery

```bash
mp reviews pending                        # the review queue
mp reviews pending --summary              # steps done/total, open findings per item
mp reviews pending --filter force-bypassed
mp reviews status                         # unified: execution-review + spec-review
mp reviews lifecycle                      # cross-project rollup by review_state
mp reviews lifecycle --summary            # bucket counts only
mp reviews sweep                          # triage the queue into risk buckets
```

## Dependency awareness

Before touching a milestone, know its blast radius:

```bash
mp milestone deps 42          # what it depends on
mp milestone dependents 42    # what depends on it
mp milestone impact 42        # transitive reverse deps + path pins
mp graph --milestone 42 --with-steps --with-ac   # JSON graph
```

## Execution readiness

```bash
mp execution check           # ready? what's blocking?
mp execution status          # current execution state
mp execution report 42       # the executor's claims (read BEFORE reviewing)
```

## Inbox & hygiene

```bash
mp inbox                                   # actionable items (default)
mp inbox --filter all                      # everything
mp hygiene --stale-days 30                 # stale/untouched items
```
