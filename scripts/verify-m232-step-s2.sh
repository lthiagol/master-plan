#!/usr/bin/env bash
# verify-m232-step-s2.sh — M232 S2 (seed_handoff_gate + init_git single-source).
#
# Two `&&`-chained ripgrep counts wrapped for mp's argv-only parser.
# Each helper must be defined exactly once under crates/mp/tests/.
set -euo pipefail
test "$(rg 'fn seed_handoff_gate\b' crates/mp/tests | wc -l)" -eq 1
test "$(rg 'fn init_git\b' crates/mp/tests | wc -l)" -eq 1