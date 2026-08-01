#!/usr/bin/env bash
# backfill-step-tests.sh — Assign non-empty tests to empty-test steps in given milestone range.
# Usage: backfill-step-tests.sh <min-milestone> <max-milestone>
set -e
cd ~/code/master-plan
export MP_HOME="$(pwd)"
MP=./target/release/mp
MIN="${1:-07}"
MAX="${2:-07}"

STEPS=$("$MP" list steps --format json 2>/dev/null)

echo "$STEPS" | jq -c --arg min "$MIN" --arg max "$MAX" '.steps[] | select(.step.tests == "" or .step.tests == null) | select(.milestone | tonumber >= ($min|tonumber) and tonumber <= ($max|tonumber)) | {milestone, step}' | while read -r item; do
    ms=$(echo "$item" | jq -r '.milestone')
    id=$(echo "$item" | jq -r '.step.id')
    action=$(echo "$item" | jq -r '.step.action')
    done_when=$(echo "$item" | jq -r '.step.done_when // ""')
    files=$(echo "$item" | jq -r '.step.files // [] | join(" ")')

    # Heuristic: pick a test value based on files/action/done_when
    test=""
    if echo "$files $action $done_when" | grep -qiE '\.rs|rust|cargo|compile|test'; then
        test="cargo test -p mp"
    elif echo "$files" | grep -qiE 'schemas/'; then
        test="make adopt-check"
    elif echo "$files" | grep -qiE 'templates/'; then
        test="make adopt-check"
    elif echo "$files $action $done_when" | grep -qiE '\.md|docs/|readme|changelog|documentation|agents\.md'; then
        test="manual: verify doc content matches CLI behavior"
    elif echo "$files $action $done_when" | grep -qiE 'ci|github|workflow|act|docker|container'; then
        test="make adopt-check"
    elif echo "$files $action $done_when" | grep -qiE 'make |Makefile|script|\.sh'; then
        test="make adopt-check"
    elif echo "$files $action $done_when" | grep -qiE 'release|version|changelog|tag'; then
        test="manual: verify version and release notes correct"
    elif echo "$files" | grep -qiE '\.toml$'; then
        test="cargo build --release -p mp"
    else
        test="make adopt-check"
    fi

    echo "M$ms $id ← $test"
    "$MP" step update "$ms" "$id" --tests "$test" --quiet
done

echo "=== done: M$MIN-M$MAX ==="
