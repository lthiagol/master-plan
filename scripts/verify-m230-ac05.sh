#!/usr/bin/env bash
# verify-m230-ac05.sh — M230 AC-05 source-pin + lint (cycle 2 wrapper).
#
# Wraps the original rg-precondition AND consumer-surface
# lint into a single argv-clean command so mp's argv-only
# verifier can run both at `mp milestone complete` time.
#
# The rg precondition asserts that no `stale-watch:docs/
# raul/keybinds.md` allowlist entry remains in
# scripts/check-consumer-surface.sh; the lint is what proves
# the rest of the consumer surface is clean.
set -e
! rg -q 'stale-watch:docs/raul/keybinds.md' scripts/check-consumer-surface.sh
make consumer-surface-lint