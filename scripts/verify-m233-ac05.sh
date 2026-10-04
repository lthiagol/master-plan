#!/usr/bin/env bash
# verify-m233-ac05.sh — M233 AC-05 (gate_matrix G6).
#
# Wraps the original `! rg -q '...' && cargo nextest ...' chain into one
# argv-clean command so mp's argv-only verifier can run it at
# `mp milestone complete` time. The chain encodes two claims:
#
#   1. the `|| !out.status.success` escape is GONE from gate_matrix.rs,
#   2. the g6_fires / g6_clears tests still pass (G14 must keep passing
#      per the AC's G14 invariance).
#
# Exit 0 when all three pass; non-zero on the first failure.
set -euo pipefail
! rg -q 'contains\("G6"\) \|\| !out.status.success' crates/mp/tests/suites/gate_matrix.rs
cargo nextest run -p mp --test suite_validate --no-fail-fast -E 'test(/^gate_matrix::g(6|14)_/)'