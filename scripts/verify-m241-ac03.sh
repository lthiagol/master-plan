#!/usr/bin/env bash
# verify-m241-ac03.sh — sidebar tabs, visibility, and the three new
# rebindable lane bindings.
#
# The original verification chained two nextest invocations with `&&`,
# which mp's argv-only parser rejects. Wrapped so `mp milestone
# complete` can run the full check set as one argv-clean command.
#
# The two halves are deliberately separate: `autopilot_sidebar` is the
# behaviour (tabs switch by click and by key, visibility flips, both
# persist), and the `keybind` filter is the *registry* (the three fields
# exist, are rebindable through keybinds.toml, and collide with no
# existing default). A new field wired into the struct but not the
# loader passes the first and fails the second.
set -euo pipefail
cargo nextest run -p raul --no-fail-fast --test autopilot_sidebar
cargo nextest run -p raul --no-fail-fast -E 'test(/keybind/)'
