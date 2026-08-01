#!/usr/bin/env bash
# Dependency audit for raul: single crossterm 0.29, no comfy-table/owo-colors, transitive gate.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail=0

echo "==> M73 baseline (problem statement):"
echo "  before: 118 transitive unique crates; crossterm 0.28 + 0.29 duplicate stack"
echo "  after target: <=100 transitive; single crossterm 0.29 via workspace pin"

echo "==> crossterm versions in raul tree:"
crossterm_versions=$(cargo tree -p raul -i crossterm 2>/dev/null | grep -E '^crossterm ' | sort -u || true)
if [ -z "$crossterm_versions" ]; then
  echo "  FAIL: crossterm not found in raul dependency tree"
  fail=1
else
  echo "$crossterm_versions" | sed 's/^/  /'
  count=$(echo "$crossterm_versions" | wc -l | tr -d ' ')
  if [ "$count" -ne 1 ]; then
    echo "  FAIL: expected exactly one crossterm version, found $count"
    fail=1
  fi
  if ! echo "$crossterm_versions" | grep -q 'crossterm v0\.29\.'; then
    echo "  FAIL: expected crossterm 0.29.x, got: $crossterm_versions"
    fail=1
  fi
fi

echo "==> comfy-table in Cargo.toml (should be absent):"
if grep -q 'comfy-table' crates/raul/Cargo.toml 2>/dev/null; then
  echo "  FAIL: comfy-table still listed in crates/raul/Cargo.toml"
  fail=1
else
  echo "  OK"
fi

echo "==> comfy-table in dependency tree (should be absent):"
if cargo tree -p raul --prefix none 2>/dev/null | grep -q '^comfy-table '; then
  echo "  FAIL: comfy-table still in raul dependency tree"
  fail=1
else
  echo "  OK"
fi

echo "==> owo-colors in Cargo.toml (should be absent):"
if grep -q 'owo-colors' crates/raul/Cargo.toml 2>/dev/null; then
  echo "  FAIL: owo-colors still listed in crates/raul/Cargo.toml"
  fail=1
else
  echo "  OK"
fi

echo "==> owo-colors in dependency tree (should be absent):"
if cargo tree -p raul --prefix none 2>/dev/null | grep -q '^owo-colors '; then
  echo "  FAIL: owo-colors still in raul dependency tree"
  fail=1
else
  echo "  OK"
fi

echo "==> raul transitive unique crates:"
n=$(cargo tree -p raul --prefix none 2>/dev/null | sort -u | wc -l | tr -d ' ')
echo "  now: $n (baseline was 118; target <=100)"
if [ "$n" -gt 100 ]; then
  echo "  FAIL: $n > 100 (target <=100)"
  fail=1
fi

echo "==> direct deps without explicit features= (document exceptions only):"
awk '/^\[dependencies\]/{f=1;next}/^\[/{f=0}f && /^[a-z]/ && !/features/ && !/^mp-model/ {print "  - "$1}' \
  crates/raul/Cargo.toml || true

if [ "$fail" -ne 0 ]; then
  exit 1
fi

echo "OK — raul dependency audit passed ($n transitive, single crossterm 0.29)"
