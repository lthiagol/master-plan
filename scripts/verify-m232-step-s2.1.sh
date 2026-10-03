#!/usr/bin/env bash
# verify-m232-step-s2.1.sh — M232 S2.1 (suite green + clippy + fmt).
#
# Wraps the three `&&`-chained gates into one argv-clean command so
# mp's argv-only parser accepts the verifier at `mp milestone
# complete` time. The AC-04 verifier is the same shape; this wrapper
# exists because mp treats step `tests` independently.
set -euo pipefail
cargo nextest run -p mp --no-fail-fast
cargo clippy -p mp --tests --no-deps -- -D warnings
cargo fmt --all -- --check