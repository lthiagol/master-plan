#!/usr/bin/env bash
# scripts/verify-m-tui-flash-test.sh
#
# M163 AC-04 verification: end-to-end review-menu gate tests in
# crates/raul/tests/tui_review_menu_gate.rs. Drives:
#   - AC-01 focused M121 message (stderr + stdout paths)
#   - AC-02 truncation + `?` details
#   - AC-03 preflight gate per-AC statuses + dim rendering
#   - Non-M121 failures retain a useful message
# The prior version was a placeholder that printed success regardless.
#
# Usage: bash scripts/verify-m-tui-flash-test.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "M163 AC-04 verifier: review-menu gate integration tests"

if ! command -v cargo-nextest >/dev/null; then
    echo "FAIL: cargo-nextest not on PATH; install with 'cargo install cargo-nextest'" >&2
    exit 2
fi

# `--no-fail-fast` so every failing case in this slice is reported
# (one of the M163 acceptance tests checks stderr-only M121 emission;
# another checks per-AC preflight counts; both must run independently).
cargo nextest run -p raul --test tui_review_menu_gate --no-fail-fast
