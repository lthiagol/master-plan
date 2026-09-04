# Autopilot migration guide

`mp autopilot` is the canonical command tree for driving milestones
through their lifecycle. The legacy `mp watch` spelling is preserved
as a deprecation alias and will be removed in a future release.

This guide covers the move from `mp watch` to `mp autopilot` and
the deprecation timeline.

## Background

`mp autopilot` is the umbrella command for everything that used to
live under the `mp watch` and `mp watch-control` surfaces:

| Legacy spelling       | Canonical spelling                              |
|-----------------------|-------------------------------------------------|
| `mp watch <ids...>`   | `mp autopilot start [IDS]...`                   |
| `mp watch-control status` | `mp autopilot status [--summary]`           |
| `mp watch-control stop`   | `mp autopilot stop [--pid N]`              |
| `mp watch-control output` | `mp autopilot output [--max-bytes N] ...` |
| `mp watch-control result` | `mp autopilot result [--force]`            |

The two trees accept identical arguments, return identical exit
codes, and emit byte-identical JSON on stdout.

## Deprecation timeline

1. **Now (deprecation notice active).** Every `mp watch` invocation
   prints one deprecation line on stderr:

   ```
   mp watch is deprecated; use 'mp autopilot' instead.
   ```

   The warning fires once per invocation, regardless of plan state.
   Canonical `mp autopilot` invocations never print this line.

2. **Future release (removal).** The `mp watch` command will be
   removed entirely. Scripts and muscle memory should switch to the
   `mp autopilot` spelling before that release ships.

## Migration steps

1. Replace `mp watch` with `mp autopilot start` in scripts, CI
   pipelines, aliases, and documentation. The argument shape is
   identical.
2. Replace `mp watch-control status|stop|output|result` with the
   matching `mp autopilot status|stop|output|result` command. The
   flag set is identical.
3. Update any tooling that scrapes the deprecation line on stderr
   to expect the new wording:

   - Old: `` `mp watch` is deprecated; use `mp autopilot start` instead ``
   - New: `mp watch is deprecated; use 'mp autopilot' instead.`

4. If your environment splits stderr from stdout (e.g. CI log
   scrapers, alerting, structured log collectors), the deprecation
   line is the only output `mp autopilot start` and the legacy
   `mp watch` write to stderr for the dry-run path. Stdout remains
   clean JSON.

## What's unchanged

- Exit codes for every command in both trees.
- JSON output on stdout (identical bytes for the same arguments).
- The `--dry-run`, `--log-file`, `--stall-timeout-ms`,
  `--poll-interval-ms`, `--resume`, `--force`, and `--detach` flag
  set on `start`.
- The legacy `.mp/watch.state.json` migration path. The
  `mp autopilot migrate` command is idempotent and safe to run on
  projects that have already migrated.

## See also

- `docs/mp/commands.md` — full command reference.
- `docs/autopilot/session-format.md` — autopilot session schema.
