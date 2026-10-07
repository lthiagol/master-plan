#!/usr/bin/env bash
# verify-m249-ac02.sh — sample_session_show uses real QueueItem shape.
#
# The original verification chained two checks with `&&` (a ripgrep
# probe + a nextest run) and a `||` for the failure path, none of
# which mp's argv-only parser accepts. It also pointed at the wrong
# file path (`crates/mp/tests/common/lib_api/sample_session_show.rs`
# does not exist — the fixture is the `sample_session_show` *function*
# inside `crates/raul/tests/autopilot_manual_refresh.rs`) and used
# the wrong ripgrep pattern (`fn label` / `fn role` would match Rust
# *function* names, not the JSON keys the fixture actually carries).
# Wrapped so `mp milestone complete` can run the full check set as
# one argv-clean command.
#
# Two distinct claims, two distinct proofs:
#   1. The `sample_session_show` fixture's queue section no longer
#      carries pane-graph keys (`"label"` / `"role"` / `"role_skill"`).
#      The check is scoped to the fixture's body — other tests in
#      the same file (`manual_refresh_combines_session_show_with_status_pane_ids`,
#      `manual_refresh_produces_a_consistent_status_graph`) test the
#      StatusGraph (pane-graph view), which reads those keys from
#      the queue items deliberately. The AC's claim is about the
#      *shared* fixture, not the file as a whole.
#   2. The fixture, fed to the production refresh adapter, populates
#      a QueueView row with a non-empty id. The new
#      `sample_session_show_fixture_yields_a_non_empty_queue_row_id`
#      test is the pin; the `autopilot_manual_refresh` binary also
#      covers the four pre-existing tests that share the fixture.
set -euo pipefail

# 1. The fixture's queue items use the real QueueItem shape.
#
# Extract the `fn sample_session_show` body via brace-matching so the
# pane-graph check is scoped to the fixture, then locate the
# `"queue": [ ... ]` block within it. Pane-graph keys are forbidden
# *only* on queue items — `working_on.role` is a real field on mp's
# `WorkingOn` struct, and StatusGraph's other tests in this file
# still use the legacy shape intentionally (they test the pane-graph
# view). The AC's claim is about the shared fixture's queue items.
FIXTURE=crates/raul/tests/autopilot_manual_refresh.rs
python3 - "$FIXTURE" <<'PY'
import re, sys
path = sys.argv[1]
src = open(path).read()
m = re.search(r'\bfn sample_session_show\b', src)
if not m:
    print(f"FAIL: fn sample_session_show not found in {path}", file=sys.stderr)
    sys.exit(1)
# Walk braces from the function's opening `{` to find the matching `}`.
start = src.find('{', m.end())
depth = 0
end = start
for i, ch in enumerate(src[start:], start):
    if ch == '{':
        depth += 1
    elif ch == '}':
        depth -= 1
        if depth == 0:
            end = i
            break
body = src[start:end+1]
# Find the `"queue": [ ... ]` block. Brace-match the array's brackets
# so a fixture with multiple queue items or nested objects still
# parses correctly.
qm = re.search(r'"queue"\s*:\s*\[', body)
if not qm:
    print(f"FAIL: no 'queue' array in sample_session_show body", file=sys.stderr)
    sys.exit(1)
arr_start = qm.end() - 1  # position of '['
depth = 0
arr_end = arr_start
for i, ch in enumerate(body[arr_start:], arr_start):
    if ch == '[':
        depth += 1
    elif ch == ']':
        depth -= 1
        if depth == 0:
            arr_end = i
            break
queue_block = body[arr_start:arr_end+1]
for key in ('"label"', '"role"', '"role_skill"'):
    if key in queue_block:
        for ln, line in enumerate(queue_block.splitlines(), 1):
            if key in line:
                print(f"FAIL: pane-graph key {key} still in sample_session_show queue items (line {ln}): {line.strip()}", file=sys.stderr)
        sys.exit(1)
print("  OK: sample_session_show queue items use QueueItem shape")
PY

# 2. The fixture produces a non-empty queue row id, and every test
#    that shares the fixture still passes.
cargo nextest run -p raul --no-fail-fast --test autopilot_manual_refresh

