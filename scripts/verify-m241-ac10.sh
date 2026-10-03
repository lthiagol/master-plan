#!/usr/bin/env bash
# verify-m241-ac10.sh — the `?` help overlay's Autopilot section, and
# the docs listing the three new bindings.
#
# The original verification chained a nextest run and an `rg` with `&&`.
# The two halves answer different questions — does the *code* generate
# the section (including reflecting a rebind), and does the *docs*
# mention the keys — so both are kept rather than collapsing to one.
set -euo pipefail
cargo nextest run -p raul --no-fail-fast -E 'test(/help_overlay_autopilot/)'
rg -q 'next_sidebar_tab' docs/raul/keybinds.md
