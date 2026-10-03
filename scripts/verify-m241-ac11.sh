#!/usr/bin/env bash
# verify-m241-ac11.sh — the full self-review gate for the milestone.
#
# The original verification chained four commands with `&&`. mp's
# argv-only parser rejects the operators, so the set is wrapped into one
# argv-clean command.
#
# This is the one AC whose scope IS the whole workspace: its claim is
# that nothing else regressed and that the consumer surface stayed clean,
# so a per-crate filter would be a weaker check than the claim.
set -euo pipefail
cargo nextest run -p mp -p raul --no-fail-fast
cargo clippy -p mp -p raul --all-targets --no-deps -- -D warnings
cargo fmt --all -- --check
make consumer-surface-lint
