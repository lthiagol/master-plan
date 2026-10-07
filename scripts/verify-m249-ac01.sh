#!/usr/bin/env bash
# verify-m249-ac01.sh — TakeoverRow.cycle reads from session.queue[].cycle.
#
# The original verification chained two checks with `&&` (a ripgrep probe
# + a nextest run) and a `||` for the failure path, all of which mp's
# argv-only parser rejects. Wrapped so `mp milestone complete` can run
# the full check set as one argv-clean command.
#
# Two distinct claims, two distinct proofs:
#   1. cycle_from_history is gone. The function is the AC's *named*
#      target — `cycle_history` is intentionally not in the pattern,
#      because the broader `queue_cycle_history[]` is still read by
#      `DetailPanel` (history rows) and `Telemetry` (attempts-per-stage
#      from the same history's outcomes). Both are separate concerns
#      and out of M249's scope.
#   2. The M241 AC-05 cycle tests still pass against the new source.
#      The fixture changes (cycle moved from `queue_cycle_history` onto
#      the queue item) are pinned by the same test names that were
#      green in M241, so a regression on the read path surfaces here.
set -euo pipefail

# 1. The named function is deleted.
if rg -q 'cycle_from_history' crates/raul/src/tui/autopilot.rs; then
  echo "  FAIL: cycle_from_history still referenced in autopilot.rs" >&2
  exit 1
fi
echo "  OK: cycle_from_history removed"

# 2. The M241 AC-05 takeover tests still pass.
cargo nextest run -p raul --no-fail-fast --test autopilot_while_executing
