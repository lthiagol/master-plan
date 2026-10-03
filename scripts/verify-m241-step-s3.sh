#!/usr/bin/env bash
# verify-m241-step-s3.sh — S3 "help overlay reflects rebinds; docs list
# the new keys".
#
# The original chained a nextest run and an `rg` with `&&`. Wrapped for
# the argv-only parser.
set -euo pipefail
cargo nextest run -p raul --no-fail-fast -E 'test(/help_overlay_autopilot/)'
rg -q 'next_sidebar_tab' docs/raul/keybinds.md
