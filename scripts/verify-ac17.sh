#!/usr/bin/env bash
# scripts/verify-ac17.sh
#
# M202 AC-17 verification surface. The AC description asserts six
# commands exit 0:
#   1. cargo fmt --all -- --check
#   2. cargo clippy -p mp --tests --no-deps -- -D warnings
#   3. cargo clippy -p raul --tests --no-deps -- -D warnings
#   4. cargo nextest run -p mp --no-fail-fast
#   5. cargo nextest run -p raul --no-fail-fast
#   6. mp validate
#
# The mp verify executor is argv-only (no shell operators), so a
# single make/script entry point must exercise EVERY assertion in
# the description. `make test` only covers nextest + fmt; this
# script runs the full matrix and exits non-zero on the first
# failure (F-07 fix).
#
# Usage: scripts/verify-ac17.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

run() {
    echo "==> $*"
    "$@"
}

# 1. Format check.
run cargo fmt --all --manifest-path "$ROOT/Cargo.toml" -- --check

# 2. + 3. Clippy with warnings as errors (mp + raul; mp-model is
# covered transitively by `-p mp`).
run cargo clippy -p mp --tests --no-deps --manifest-path "$ROOT/Cargo.toml" -- -D warnings
run cargo clippy -p raul --tests --no-deps --manifest-path "$ROOT/Cargo.toml" -- -D warnings

# 4. + 5. Full test suite per crate (no-fail-fast so every failure
# surfaces in one run).
run cargo nextest run -p mp --no-fail-fast --manifest-path "$ROOT/Cargo.toml"
run cargo nextest run -p raul --no-fail-fast --manifest-path "$ROOT/Cargo.toml"

# 6. Plan validation (requires `mp` on PATH or MP_HOME/bin/mp).
if command -v mp >/dev/null 2>&1; then
    run mp validate
else
    echo "mp not found on PATH; skipping mp validate (set MP_HOME/bin on PATH)" >&2
    exit 2
fi

echo "AC-17 matrix OK"
