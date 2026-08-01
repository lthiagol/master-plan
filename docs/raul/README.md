# raul — the human-facing terminal UI

`raul` is the read-only, human view of a Master Plan project. It renders the
same JSON that `mp` owns into a keyboard-driven dashboard. `raul` **never writes
plan files** — every preference you change inside it is persisted through
`mp config set`, and every status change is a hint that you should run the
equivalent `mp` command (or hand it to an agent).

> If you remember one thing: `raul` shows you the plan; `mp` changes it.

## Launching

```bash
raul
```

That's it — `raul` takes no subcommands. It opens on the **Overview** lane of
the current project's plan. Run it from the root of a project that has a plan
(`master-plan/` or `.mp/`).

### Flags

| Flag | Effect |
|------|--------|
| `--color on\|off` | Force color on or off for this run (overrides `ui.color` config) |
| `--project-root <path>` | Project root to operate on (forwarded to `mp`) |
| `--plan-dir <path>` | Plan directory to operate on (forwarded to `mp`) |

Any other argument that looks like an old subcommand (for example `raul status`)
prints a short migration reminder and exits. Launch the TUI instead.

## The seven lanes

`raul` is organized as seven tabs you move between left/right. Each lane is a
different cut of the same plan.

| Lane | Shows |
|------|-------|
| **Overview** | Headline counts (milestones by lifecycle), the inbox, recent activity, and the suggested next path |
| **Milestones** | The milestone list — drill into one to see its full detail. Sort with `Shift+S`, filter lifecycle with `Shift+F` |
| **Path** | The work queue across planning lanes (blocked, execution, review, grooming, backlog) |
| **Backlog** | Deferred/parked items, promotable into milestones or tracks |
| **Ideas** | Vague "someday" ideas — promotable into milestones, backlog, or tracks |
| **Watch** | The `mp watch` driver surface: browse drivable milestones and watch the live queue, lifecycle graph, and agent output of an `mp watch` run |
| **Settings** | raul UI preferences (color, icons, theme, hide-done) |

Navigate lanes with `←`/`→` (or `h`/`l`, or `Tab`/`Shift+Tab`), or jump directly
with the number keys `1`–`7`.

## What you can do

`raul` is read-only with respect to the plan, but it is interactive:

- **Browse & drill in.** Move up/down a list, press `Enter` to open an item's
  detail, `Esc` to go back. Inside a milestone detail, `]`/`[` jump between
  sections and `n`/`p` jump between list items.
- **Refresh.** `r` re-reads the plan from disk. On the Overview lane, `w`
  toggles auto-refresh (watch) so the dashboard polls on an interval.
- **Filter, sort & search.** `f` toggles a filter; `Shift+F` opens the
  lifecycle filter; `Shift+S` opens the sort-rebind menu (the chosen sort key
  persists per lane through `mp config set sort.<lane> <key>`); `o` cycles the
  sort key inline; `/` opens search; `h` hides completed items.
- **Annotations.** `Shift+A` creates an annotation, `r` resolves one, `Shift+R`
  reopens one.
- **Approval & review menu.** `p` toggles an approval request; `m` opens the
  review menu (which runs a pre-flight check before approving).
- **Watch lane.** Shows the `mp watch` picker, lifecycle graph, queue, and
  live agent output. Start and control runs through the `mp watch` /
  `mp watch-control` CLI (see [`../mp/commands.md`](../mp/commands.md)).
- **Help.** `?` opens the on-screen legend, generated from the live key
  bindings so it never drifts from reality.

Full bindings: [`keybinds.md`](./keybinds.md). Preferences and the Settings lane:
[`settings.md`](./settings.md).

## How raul gets its data

`raul` shells out to `mp` for every read (status, lists, milestone detail, inbox,
path, search). Because `mp` emits JSON, `raul` simply renders it. Consequences:

- **raul is always consistent with `mp`.** There is no separate data path.
- **raul needs `mp` on `PATH`.** If `mp` is missing, raul falls back to defaults
  and shows an empty plan.
- **UI preferences live in the project config** (`config.toml`), not in a raul
  file. `raul` reads them via `mp config show` and writes them via
  `mp config set` from the Settings lane.
