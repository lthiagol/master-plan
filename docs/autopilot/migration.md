# Autopilot migration guide

`mp autopilot` is the canonical command tree for driving milestones
through their lifecycle. The legacy `mp watch` spelling was
preserved through `1.x` as a deprecation alias and was removed in
the next-major release.

This guide covers the move from `mp watch` to `mp autopilot`.

## Background

`mp autopilot` is the umbrella command for everything that
historically lived under the `mp watch` and `mp watch-control`
surfaces:

| Legacy spelling            | Canonical spelling                    |
|----------------------------|---------------------------------------|
| `mp watch <ids...>`        | `mp autopilot start [IDS]...`         |
| `mp watch-control status`  | `mp autopilot status [--summary]`     |
| `mp watch-control stop`    | `mp autopilot stop [--pid N]`         |
| `mp watch-control output`  | `mp autopilot output [--max-bytes N]` |
| `mp watch-control result`  | `mp autopilot result [--force]`       |

The canonical tree accepts identical arguments, returns identical
exit codes, and emits byte-identical JSON on stdout (compared to
the legacy alias that used to be available).

## Deprecation timeline (closed)

1. **1.x (deprecation notice active, REMOVED).** During the `1.x`
   release line, every `mp watch` invocation printed one deprecation
   line on stderr:

   ```
   mp watch is deprecated; use 'mp autopilot' instead.
   ```

   The warning fired once per invocation, regardless of plan state.
   Canonical `mp autopilot` invocations never printed this line.

2. **Next major release (cut).** The `mp watch` and
   `mp watch-control` commands and the `mp autopilot migrate`
   shim were removed. The breaking-release cleanup
   (`mp breaking-release preflight`) was the gate
   for the cut.

## Migration steps

1. Replace `mp watch` with `mp autopilot start` in scripts, CI
   pipelines, aliases, and documentation. The argument shape is
   identical.
2. Replace `mp watch-control status|stop|output|result` with the
   matching `mp autopilot status|stop|output|result` command.
3. The `mp autopilot migrate` subcommand was removed; the
   `.mp/watch.state.json` migration path was consumed during the
   compatibility window. Autopilot sessions now live exclusively
   under `<plan_dir>/autopilot/<id>/session.json`.
4. The `ui.show_watch_tab` config key was removed; `mp doctor` no
   longer surfaces a `ui_show_watch_tab` row. Update raul config
   files that referenced the legacy key.

## What's unchanged

- Exit codes for every command in the canonical tree.
- JSON output on stdout (same shape as the legacy alias used to
  emit).
- The `--dry-run`, `--log-file`, `--stall-timeout-ms`,
  `--poll-interval-ms`, `--resume`, `--force`, and `--detach`
  flag set on `start`.
- The autopilot session schema under `<plan_dir>/autopilot/<id>/`.
  Existing autopilot sessions are unchanged by the cleanup.

## See also

- `docs/mp/commands.md` — full command reference.
- `docs/autopilot/session-format.md` — autopilot session schema.
- `mp breaking-release preflight` — record the next-major target
  version and migration-window evidence before further
  compatibility removals.
