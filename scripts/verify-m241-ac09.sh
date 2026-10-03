#!/usr/bin/env bash
# verify-m241-ac09.sh — setup choices write through mp; the new
# ui.autopilot.* keys validate; no raul-owned state file.
#
# The original verification chained three checks with `&&` and prefixed
# the last with `!`, neither of which mp's argv-only parser accepts.
# Wrapped into one argv-clean command.
#
# Three distinct claims, three distinct proofs:
#   1. autopilot_persistence  — raul's side: the exact `mp` argv, and
#      that per-run choices (milestone selection, run mode) are not
#      written. Includes its own source-level check for the absence of a
#      raul-owned state file, so the `! rg` precondition below is a
#      second, independent guard over the whole crate.
#   2. suite_config/ui_autopilot — mp's side: set/get round-trips at both
#      clamp bounds, out-of-range / bad-enum / non-boolean rejection, and
#      the "reading a default writes nothing" additive contract.
#   3. the grep — a second, independent guard over raul's *source*. The
#      scope is `src/` rather than all of `crates/` on purpose: the
#      claim is that raul does not *write* such a file, so the check
#      belongs on the code that would write it. Widening it to
#      `crates/` would make the guard match the test that searches for
#      the same filename — a tautology that passes for the wrong
#      reason.
set -euo pipefail
cargo nextest run -p raul --no-fail-fast --test autopilot_persistence
cargo nextest run -p mp --test suite_config --no-fail-fast -E 'test(/ui_autopilot/)'
! rg -q 'autopilot-state\.json' crates/raul/src
