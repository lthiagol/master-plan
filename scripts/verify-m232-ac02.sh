#!/usr/bin/env bash
# verify-m232-ac02.sh — M232 AC-02 (helper dedup).
#
# Wraps three `&&`-chained ripgrep counts so mp's argv-only parser
# accepts the verifier at `mp milestone complete` time. Each of
# `seed_handoff_gate`, `init_git`, and `capture_stdio` must have
# exactly one `fn ...\b` definition under `crates/mp/tests/`.
set -euo pipefail
test "$(rg 'fn seed_handoff_gate\b' crates/mp/tests | wc -l)" -eq 1
test "$(rg 'fn init_git\b' crates/mp/tests | wc -l)" -eq 1
test "$(rg 'fn capture_stdio\b' crates/mp/tests | wc -l)" -eq 1