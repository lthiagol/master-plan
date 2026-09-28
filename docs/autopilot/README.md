# Autopilot

The `mp autopilot` orchestration surface. Each autopilot session is a
self-contained **orchestrator + runner + reviewer** workflow, tracked in
`<plan_dir>/autopilot/<id>/session.json` so it can be archived, diffed, and
recovered in isolation.

## Following a run

`mp autopilot status` is a snapshot — useful for a machine client, awkward for a
human who wants to *block* on a run. Two verbs cover the follow-up case.

### `mp autopilot wait <id> [--timeout <sec>]`

Blocks until the milestone reaches the lifecycle its run is waiting for (or
`complete`), then exits. The JSON body is always emitted first, so a shell
caller can read the outcome without parsing prose:

```bash
mp autopilot wait 246              # default --timeout 1800 (30 min)
mp autopilot wait 246 --timeout 60
```

```json
{ "reached": true,  "lifecycle": "self-reviewed", "target_lifecycle": "self-reviewed" }
{ "reached": false, "lifecycle": "in-progress", "target_lifecycle": "self-reviewed", "reason": "timeout" }
```

Exit code is the contract: **0** when `reached` is true, **1** otherwise.
`reason` is one of:

| reason | meaning |
|--------|---------|
| `timeout` | `--timeout` elapsed first |
| `run-stopped` | the run is gone or no longer driving this milestone — no state file, a dead driver PID, or a run that finished without the milestone advancing |
| `run-failed` | the run recorded a terminal outcome that is not success (failed, skipped, spawn-failed, exhausted) |

For a milestone the run is not actively driving there is no stage target, so
`complete` is the only success condition. A milestone that has already reached
`complete` satisfies the wait even if the recorded target was an earlier stage —
a run will not move it backwards.

### `mp autopilot tail <id> [--follow] [--since <rfc3339>] [--idle <sec>]`

Prints the activity journal for one milestone, oldest first, one compact JSON
object per line — a stream shape you can read line by line while the run is
still writing:

```bash
mp autopilot tail 246                        # print and exit
mp autopilot tail 246 --since 2026-09-28T14:00:00Z
mp autopilot tail 246 --follow --idle 10      # keep watching; stop after 10s of quiet
```

Only events whose `subject` is the requested id are printed. `--follow` keeps
polling after the journal is drained and exits — status 0 — once `--idle`
seconds (default 5) pass with no new event. Reaching the idle timeout is a
normal end to a follow, not a failure.

## Prompt delivery and stall detection

Two knobs decide how patiently a run waits on a slow agent.

**`--prompt-settle-ms <ms>`** (default 5000) — how long a harness must report
`idle` *continuously* before a prompt is delivered. A freshly spawned pane's
harness TUI is still booting when the first prompt arrives, and the status read
can report `idle` during that window; delivering into it drops the prompt.
Any non-idle read resets the window. `0` restores the legacy first-idle
behaviour. The readiness gate's own timeout still bounds the whole wait, settle
window included, so an unreachable harness fails rather than hanging.

**`agent.automation.stall_timeout_minutes`** (1..=240, default 30) — how long
the drive loop waits for a *stalled* runner. The stall timer only accrues while
the runner is **not** working, so a long build or test run is never mistaken for
a hang. A lifecycle change that is not a completion also counts as progress and
resets the accrued time.

A hard ceiling still applies at *any* status: no lifecycle advance for
**4 × the stall timeout** stalls the run. That bounds a genuinely hung runner
without penalising a working one.

Precedence is `--stall-timeout-ms` > `agent.automation.stall_timeout_minutes` >
30 minutes.

```bash
mp config set agent.automation.stall_timeout_minutes 45
mp config get agent.automation.stall_timeout_minutes
```

## Reference

- [`session-format.md`](./session-format.md) — the JSON schema, the topology
  fields, and the runtime state machine
- [`migration.md`](./migration.md) — schema-version upgrades and how to migrate
  older sessions