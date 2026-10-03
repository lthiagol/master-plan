#!/usr/bin/env bash
# verify-m232-ac01.sh — M232 AC-01 (lib_api split).
#
# Wraps the original `&&`-chained line-count / file-existence checks into
# one argv-clean command so mp's argv-only verifier can run it at
# `mp milestone complete` time. mp's parser rejects `&&`, so this wrapper
# is the surface that proves the split actually landed:
#
#   1. lib_api.rs is under 100 lines,
#   2. lib_api/ctx.rs exists,
#   3. lib_api/mutation.rs exists,
#   4. lib_api/io.rs exists,
#   5. lib_api/capture.rs exists.
#
# Exit 0 when all five pass; non-zero on the first failure.
set -euo pipefail
test "$(wc -l < crates/mp/tests/common/lib_api.rs)" -lt 100
test -f crates/mp/tests/common/lib_api/ctx.rs
test -f crates/mp/tests/common/lib_api/mutation.rs
test -f crates/mp/tests/common/lib_api/io.rs
test -f crates/mp/tests/common/lib_api/capture.rs