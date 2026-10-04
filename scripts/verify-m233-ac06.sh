#!/usr/bin/env bash
# verify-m233-ac06.sh — M233 AC-06 (negative integration tests).
#
# Wraps the three `&&`-chained nextest runs into one argv-clean command
# so mp's argv-only parser accepts the verifier at `mp milestone
# complete` time. Each negative binary must pass; any failure exits
# non-zero.
set -euo pipefail
cargo nextest run -p mp --test config_cmd_negative --no-fail-fast
cargo nextest run -p mp --test archive_negative --no-fail-fast
cargo nextest run -p mp --test digest_negative --no-fail-fast