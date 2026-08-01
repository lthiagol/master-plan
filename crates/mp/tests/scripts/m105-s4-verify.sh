#!/usr/bin/env bash
# M105 S4 / AC-04 closure verification.
#
# Tolerates two macOS quirks:
#   * `wc -l` emits leading whitespace → piped through `xargs` to trim.
#   * `grep -l` exits non-zero when no files match → `|| true` so the
#     count chain still feeds `wc -l` and `set -e` doesn't kill the script.
set -euo pipefail

mp edit strip-dropped-keys >/dev/null

PLAN_DIR="${MP_PLAN_DIR:-master-plan}"
FOLLOW=$( { grep -l '"follow_ups": \[\]' "$PLAN_DIR/milestones/"*.json 2>/dev/null || true; } | wc -l | xargs)
ERR=$(mp validate --summary | jq .error_count | xargs)

if [ "$FOLLOW" = "0" ] && [ "$ERR" = "0" ]; then
  echo "AC-04: PASS (follow_ups_files=$FOLLOW, validate_error_count=$ERR)"
  exit 0
fi

echo "AC-04: FAIL (follow_ups_files=$FOLLOW, validate_error_count=$ERR)" >&2
exit 1
