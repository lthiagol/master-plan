#!/usr/bin/env bash
# verify-m232-step-s1.sh — M232 S1 (split lib_api.rs into ctx/mutation/io/capture).
#
# Same shape as verify-m232-ac01.sh: line-count < 100 plus the four
# submodule files. Kept distinct from AC-01's verifier because the
# step definition predates the AC and a step wrapper is required by
# mp's argv-only parser regardless of the AC wrapper.
set -euo pipefail
test "$(wc -l < crates/mp/tests/common/lib_api.rs)" -lt 100
test -f crates/mp/tests/common/lib_api/ctx.rs