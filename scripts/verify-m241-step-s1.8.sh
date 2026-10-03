#!/usr/bin/env bash
# verify-m241-step-s1.8.sh — S1.8 "every persisted choice round-trips
# through mp".
#
# The original chained a nextest run and a `! rg` with `&&`. The `!` is
# also rejected by the argv-only parser, so both halves are wrapped.
set -euo pipefail
cargo nextest run -p raul --no-fail-fast --test autopilot_persistence
! rg -q 'autopilot-state\.json' crates/raul/src
