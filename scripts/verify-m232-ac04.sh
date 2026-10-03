#!/usr/bin/env bash
# verify-m232-ac04.sh — M232 AC-04 (suite green / clippy / fmt).
#
# Wraps the three `&&`-chained gates into one argv-clean command so
# mp's argv-only verifier can run it at `mp milestone complete` time.
# All three must pass; any failure exits non-zero.
set -euo pipefail
cargo nextest run -p mp --no-fail-fast
cargo clippy -p mp --tests --no-deps -- -D warnings
cargo fmt --all -- --check