# `mp watch`

**M149: drive one or more milestones through their lifecycle automatically by spawning runner/coordinator agents via herdr. Processes milestones sequentially; same runner and coordinator panes are reused across milestones. Use `--dry-run` to preview the execution plan without spawning agents or modifying `plan.json`**

**Usage:**

```text
Usage: watch [OPTIONS] [IDS]...
```

**Options:**

| Flag | Description |
|------|-------------|
|  | One or more milestone IDs to process (e.g. `135` or `M135`). Processed sequentially in the order given |
| `--dry-run` | Print the execution plan (milestone states, next actions, herdr commands) without modifying `plan.json` or spawning any agents |
| `--log-file` | Override the structured-log path (default: `<plan_dir>/.mp/watch.log`) |
| `--stall-timeout-ms` | Max milliseconds the lifecycle poll waits before flagging the agent as hung. Default: 1_800_000 (30 min). Lower in tests to bail fast when a fake agent can't advance the milestone |
| `--poll-interval-ms` | Lifecycle poll interval in milliseconds. Default: 1000 |
| `--resume` | M152 / AC-02: re-attach to any herdr role panes that already exist for the active milestones. The crash / SIGINT recovery path: a previous `mp watch` was interrupted, the panes are still alive in herdr, this resume run picks them up instead of double-spawning |
| `--force` | M152 / AC-03: bypass the double-spawn guard. The default (`mp watch` without `--resume` or `--force`) refuses to run when role panes already exist for the active milestones; `--force` opts in to ignoring that check. Once past the gate, `--force` behaves identically to `--resume` — the existing panes are reused, not killed and re-spawned. To start with fresh panes, kill them manually first (e.g. via the herdr CLI) and re-run without `--resume` / `--force` |

