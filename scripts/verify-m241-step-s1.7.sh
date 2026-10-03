#!/usr/bin/env bash
# verify-m241-step-s1.7.sh — S1.7 "final verify: all gates green".
#
# S1.7 exists to run the complete gate set, so its `tests` value is the
# same four commands as the original chain. Wrapped for the argv-only
# parser. Kept as a separate script from verify-m241-ac11.sh even though
# the command sets match: the step and the AC are separate records with
# separate evidence, and collapsing them would make a future change to
# one silently alter the other.
set -euo pipefail
cargo nextest run -p mp -p raul --no-fail-fast
cargo clippy -p mp -p raul --all-targets --no-deps -- -D warnings
cargo fmt --all -- --check
make consumer-surface-lint
