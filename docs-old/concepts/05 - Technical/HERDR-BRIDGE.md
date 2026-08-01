# Herdr stage-done bridge (M150)

`mp watch` detects stage completion by polling
`mp show milestone <id> --fields milestone.lifecycle` once per
second. The poll is cheap, but it's also laggy — a stage that ends
immediately after a poll can take up to 1s (the poll interval) to
register. The opencode herdr bridge
(`~/.config/opencode/plugins/herdr-agent-state.js`) reports only
`idle` / `working` and never `done`, so `herdr agent-status` can't
substitute for the poll.

M150 closes that gap by adding an explicit, push-based completion
signal — a **stage-done sentinel** — that flows from the producer
(`mp milestone complete`, `mp reviews pass`) to the consumer
(`mp watch`'s wait loop). The sentinel is an additive wake-up hint
on top of the lifecycle poll; **the lifecycle field remains the
authoritative source of truth** (F-15).

## Sentinel contract

| Field | Value | Source |
|-------|-------|--------|
| `STAGE_DONE_SENTINEL` | `"mp-stage-done"` | [`crates/mp/src/watch/bridge.rs`](../../../crates/mp/src/watch/bridge.rs) |
| `--source` (herdr) | `"mp"` | same |
| `--agent` (herdr) | `"mp-runner"` | same |
| `--state` (herdr) | `idle` | always; sentinel ≠ agent-status |
| `--message` (herdr) | `<milestone-id>` | the active milestone id (F-05) |

The sentinel is the **`custom_status`** field of a
`herdr pane report-agent` call:

```bash
herdr pane report-agent "$HERDR_PANE_ID" \
    --source mp \
    --agent mp-runner \
    --state idle \
    --custom-status mp-stage-done \
    --message "<milestone-id>"
```

**`--seq` is intentionally omitted.** Verified against herdr 0.7.3:
`--seq N` requires a numeric value (herdr rejects strings with
`invalid value for --seq: <string>`). The producer does not need a
sequence counter because `pane get` only exposes `custom_status` in
the known envelope; the consumer cannot read `--message` or `--seq`
back, so neither adds information beyond `custom_status == "mp-stage-done"`.
The milestone id rides in `--message` (F-05).

`herdr pane get <pane>` then exposes
`result.pane.custom_status == "mp-stage-done"` until the consumer
clears it via `herdr pane report-metadata --clear-custom-status`
(F-10 — consumed best-effort after observation so the next stage
starts clean).

## Producer side

When `mp milestone complete <id>` (and the M145 auto-promote path of
`mp reviews pass --verdict ok`) successfully writes a terminal
`lifecycle=complete`, it inspects the environment:

- If `HERDR_PANE_ID` is set AND `herdr` is on `PATH`, the command
  calls `herdr pane report-agent` with the sentinel and the
  milestone id in `--message`.
- Otherwise: silent no-op. The sentinel emission is **best-effort**.
  A failure to emit must never fail the underlying write — that
  would punish agents running `mp milestone complete` from a
  non-herdr shell with a regression.

The subprocess call is **bounded**: `Command::spawn` + `try_wait`
loop with a 500ms wall-clock deadline (F-13). On timeout the child
is `kill()`ed and reaped. The producer's swallow-Err path returns
`false` (not propagated); the underlying milestone write succeeds
regardless. A hung `herdr` cannot wedge `mp milestone complete` or
`mp reviews pass`.

The producer code lives in
[`crates/mp/src/milestone/complete.rs`](../../../crates/mp/src/milestone/complete.rs)
and [`crates/mp/src/reviews.rs`](../../../crates/mp/src/reviews.rs);
the bridge primitives are in
[`crates/mp/src/watch/bridge.rs`](../../../crates/mp/src/watch/bridge.rs).

## Consumer side

`mp watch`'s state machine (S7) calls
`SystemDriveOps::wait_for_lifecycle(target)`, which runs an
**integrated bridge + lifecycle loop** (F-11):

1. **Lifecycle poll** runs FIRST on each tick — every
   `WaitOptions::poll_interval_ms` (default 1000ms). Reads
   `mp show milestone <id> --fields milestone.lifecycle`. Returns
   `Reached` / `AdvancedPast` on match. The lifecycle field is the
   source of truth; the bridge can only accelerate this read, not
   block it.
2. **Sentinel poll** runs every ~100ms when a producer pane is
   tracked (the pane that received the most recent `send_prompt` —
   F-12; the ambient `HERDR_PANE_ID` is **not** consulted because
   it points at the watch process's pane, which may differ in the
   multi-pane topology). Each `herdr pane get` call has its own
   wall-clock deadline (≤200ms, ≤½ of `poll_interval_ms`).
3. On sentinel observation: do an immediate lifecycle confirm. If
   the lifecycle field agrees (`Reached` / `AdvancedPast`), return.
   If the lifecycle has NOT advanced, treat the signal as stale
   (F-10): clear it best-effort via
   `herdr pane report-metadata --clear-custom-status` and keep
   polling. A stale sentinel can never advance the state machine.

The loop interleaves both checks on a single thread: the
lifecycle poll runs first on each tick (every
`WaitOptions::poll_interval_ms`), and the sentinel poll only
starts when there is enough wall-clock budget remaining for its
bounded subprocess to finish before the next lifecycle deadline.
A `pane get` that overruns cannot push the lifecycle poll past
its deadline. Sentinel polling on a sub-second cadence can fire
**before** the next lifecycle tick, accelerating detection by up
to `poll_interval_ms` (sub-second in practice). On a silent /
failing / missing bridge the lifecycle poll drives completion on
its normal cadence — no added latency (F-11, AC-03).

The fast-path is owned by `bridge.rs` (sentinel helpers);
`wait_for_lifecycle`'s shape lives in
[`crates/mp/src/watch/herdr.rs`](../../../crates/mp/src/watch/herdr.rs).
The state machine itself is in
[`crates/mp/src/watch/state_machine.rs`](../../../crates/mp/src/watch/state_machine.rs).

## Subprocess timeouts (F-13)

Every `herdr` call from this module runs through
`run_herdr_with_timeout`, which spawns the child with
`Command::spawn` and `try_wait`s every 20ms; on deadline the child
is `kill()`ed and reaped with `wait()`. A wedged `herdr` therefore
cannot wedge `mp milestone complete`, `mp reviews pass`, or the
watch loop.

Defaults:
- Producer (`report_stage_done_bounded`): 500ms.
- Consumer (`read_custom_status_bounded`): 200ms (or `poll_interval_ms / 2`,
  whichever is smaller; floor 20ms).

## What this changes (and doesn't)

| Concern | Before M150 | After M150 |
|---------|-------------|------------|
| Stage-done latency | ≤1s (poll interval) | sub-second sentinel wake-up when the bridge is present; ≤1s fallback when not |
| Failure mode | Poll-based; tolerates bridge absence | Sentinel is additive; lifecycle poll still drives everything if the bridge is absent or broken |
| Source of truth | `plan.json lifecycle` (only) | `plan.json lifecycle` (unchanged) — sentinel is a hint, never authoritative |
| Wire surface | none | `herdr pane report-agent` (producer) + `herdr pane get` (consumer) + `herdr pane report-metadata --clear-custom-status` (consumer cleanup) |
| Stale-signal handling | n/a | Sentinel cleared best-effort on observation; lifecycle confirm required to advance (F-10) |

## Caveats

- **Pane routing** (F-12): the consumer polls the **producer pane**
  (the pane that received the most recent `send_prompt`), not the
  ambient `HERDR_PANE_ID` at watch startup. This matters in the
  multi-pane topology where `mp watch` runs in a different pane
  than the role that emitted the sentinel (`mp milestone complete`
  in the runner/coordinator pane). The pane cache is reused across
  milestones (AC-04); the producer pane is repopulated on every
  milestone switch by `send_prompt_to`.
- **Sentinel monotonicity:** the sentinel does not carry a
  per-stage counter. A second stage completing before `mp watch`
  notices the first will overwrite the sentinel — the fast-path
  still fires (it just fires for the second stage). The lifecycle
  poll is the authoritative ordering; the sentinel is the latency
  optimization. `set_active_milestone` clears the producer pane
  cache so a stale sentinel from a prior milestone cannot drive
  the next milestone's fast-path.

## See also

- [`crates/mp/src/watch/bridge.rs`](../../../crates/mp/src/watch/bridge.rs) — sentinel helpers + producer/consumer primitives (bounded subprocess).
- [`crates/mp/src/watch/herdr.rs`](../../../crates/mp/src/watch/herdr.rs) — `wait_for_lifecycle` (the lifecycle poll, M149 S5).
- [`crates/mp/src/watch/state_machine.rs`](../../../crates/mp/src/watch/state_machine.rs) — `drive_milestone` (M149 S7) that calls into both.
- [master-plan/milestones/M150](../../master-plan/milestones/M150.md) — the milestone spec.
