#!/usr/bin/env bash
# verify-m232-step-s1.sh — S1 "lib_api split: < 100 lines + ctx.rs submodule exists".
#
# S1's claim is line count + at least one submodule's first submodule
# (ctx.rs). The full 5-check verification (all 4 submodules) lives in
# scripts/verify-m232-ac01.sh, which is the AC-01 wrapper. Step S1
# keeps the smallest sufficient check that proves the split happened.
set -euo pipefail
test "$(wc -l < crates/mp/tests/common/lib_api.rs)" -lt 100
test -f crates/mp/tests/common/lib_api/ctx.rs
