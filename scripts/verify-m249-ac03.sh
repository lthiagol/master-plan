#!/usr/bin/env bash
# verify-m249-ac03.sh — full self-review gate.
#
# The original verification chained three commands with `&&` (a
# nextest run, a clippy run, and a fmt --check), which mp's
# argv-only parser rejects. Wrapped so `mp milestone complete`
# can run the full check set as one argv-clean command.
#
# This AC's claim is that nothing else regressed: the M241 AC-05
# takeover tests, the new AC-01/AC-02 tests, and the format/lint
# surfaces all stay green. A per-crate filter would be a weaker
# check than the claim, so the scope is the full mp + raul
# workspace, then the cross-crate lint + fmt.
set -euo pipefail

cargo nextest run -p mp -p raul --no-fail-fast
cargo clippy -p mp -p raul --all-targets --no-deps -- -D warnings
cargo fmt --all -- --check
