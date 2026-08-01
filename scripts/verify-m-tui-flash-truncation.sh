#!/usr/bin/env bash
# scripts/verify-m-tui-flash-truncation.sh
#
# M163 AC-02 verification: flash_message truncation to first sentence
# boundary. Runs the unit-test surface that pins the helper behaviour
# and propagates a non-zero exit code on failure (the prior version
# printed success unconditionally — a placeholder).
#
# Usage: bash scripts/verify-m-tui-flash-truncation.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "M163 AC-02 verifier: footer flash-message truncation"

# Unit tests pin the truncation helper at crates/raul/src/tui/flash_message.rs
# (the post-M163 split-out module — the truncation rules no longer live in
# render/mod.rs). Run a focused nextest invocation so a regression on this
# surface fails fast and visibly, without paying for the full raul test run.
if ! command -v cargo-nextest >/dev/null; then
    echo "FAIL: cargo-nextest not on PATH; install with 'cargo install cargo-nextest'" >&2
    exit 2
fi

# `--no-fail-fast` ensures any failure in this slice is reported even when
# other failures follow. The `-E` filter narrows to the flash_message
# module's tests; a regex match is the only way to drive unit tests that
# live inside the lib crate from a binary target.
cargo nextest run -p raul --lib --no-fail-fast -E 'test(/flash_message::tests/)'
