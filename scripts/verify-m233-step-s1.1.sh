#!/usr/bin/env bash
# verify-m233-step-s1.1.sh — M233 S1.1 (negative integration tests).
#
# Wraps the four `&&`-chained nextest runs into one argv-clean
# command so mp's argv-only parser accepts the verifier at
# `mp milestone complete` time. Each negative binary must pass;
# any failure exits non-zero.
#
# Same shape as verify-m233-ac06.sh; the fourth entry (git_negative)
# was added in cycle 2 per F-02.
set -euo pipefail
cargo nextest run -p mp --test config_cmd_negative --no-fail-fast
cargo nextest run -p mp --test archive_negative --no-fail-fast
cargo nextest run -p mp --test digest_negative --no-fail-fast
cargo nextest run -p mp --test git_negative --no-fail-fast