#!/usr/bin/env bash
# verify-m233-ac07.sh — M233 AC-07 (flakiness gate).
#
# Three consecutive runs of the new/changed tests must pass; then the
# full mp suite must stay green. Wrapped in a shell script because
# mp's argv-only parser rejects the `for` loop, the `|` filter
# union, and the `--test <name>` per-binary runs needed to scope
# each iteration. The script preserves the full sub-check set
# required by AC-07:
#
#   - 3 back-to-back runs of the new/changed test binaries + suites
#     (autopilot_drive_detach, breaking_release_apply,
#      config_cmd_negative, archive_negative, digest_negative,
#      suite_misc metrics::*, suite_validate gate_matrix::g6_*,
#      suite_track track_archive)
#   - one full `cargo nextest run -p mp --no-fail-fast`
set -euo pipefail
for i in 1 2 3; do
  cargo nextest run -p mp --no-fail-fast \
    -E 'binary(autopilot_drive_detach) | binary(breaking_release_apply) | binary(config_cmd_negative) | binary(archive_negative) | binary(digest_negative) | test(/^metrics::|track_archive|^gate_matrix::g6_/)'
  echo "flakiness run $i: pass"
done
cargo nextest run -p mp --no-fail-fast