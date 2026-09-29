#!/usr/bin/env bash
# verify-m230-ac06.sh — M230 AC-06 raul suite + clippy + fmt (cycle 2 wrapper).
#
# Wraps the original 3-check chain into a single argv-clean
# command so mp's argv-only verifier can run all three at
# `mp milestone complete` time. The checks run sequentially
# with `set -e` so any failure aborts; exit 0 means the raul
# suite, clippy with `-D warnings`, and `cargo fmt --check`
# all passed.
set -e
cargo nextest run -p raul --no-fail-fast
cargo clippy -p raul --all-targets --no-deps -- -D warnings
cargo fmt --all -- --check