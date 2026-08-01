#!/usr/bin/env bash
set -euo pipefail

fixture=${1:?fixture name required}
plan_dir=${2:-}
expect=${3:-success}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT

cp -R "$root/tests/fixtures/projects/$fixture/." "$workspace/"
args=(validate)
if [[ -n "$plan_dir" ]]; then
  args+=(--plan-dir "$plan_dir")
fi

if [[ "$expect" == "failure" ]]; then
  if (cd "$workspace" && MP_HOME="$root" "$root/target/release/mp" "${args[@]}"); then
    echo "fixture $fixture unexpectedly validated" >&2
    exit 1
  fi
else
  (cd "$workspace" && MP_HOME="$root" "$root/target/release/mp" "${args[@]}")
fi
