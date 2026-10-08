#!/usr/bin/env bash
# check-tap-checksums.sh — verify every homebrew-tap formula declares a sha256
# that belongs to the archive its own `url` names.
#
# Why this exists: the release workflow used to write the *tag* archive's
# sha256 into master-plan-dev.rb, whose `url` points at the *commit* archive.
# GitHub names an archive's root directory after the ref vs the sha, so those
# two archives are never byte-identical and their checksums can never match.
# The resulting formula passes `ruby -c` and `brew style` and only fails at the
# user's `brew install` with "Formula reports different checksum", after the
# full 34MB download. This script fails in seconds instead.
#
# Usage:
#   scripts/check-tap-checksums.sh                 # checks the local tap clone
#   scripts/check-tap-checksums.sh <formula.rb>... # checks specific files
#
# Env:
#   TAP_DIR  path to the tap checkout (default: brew's lthiagol/tap clone)
#
# Exit: 0 all match, 1 a mismatch or an unreadable field, 2 bad usage.
set -uo pipefail

command -v brew >/dev/null || { echo "error: brew required to locate the tap"; exit 1; }

if [ -z "${TAP_DIR:-}" ]; then
  TAP_DIR="$(brew --repository)/Library/Taps/lthiagol/homebrew-tap"
fi
if [ ! -d "$TAP_DIR" ]; then
  echo "error: tap dir not found: $TAP_DIR (set TAP_DIR=...)" >&2
  exit 1
fi

if [ "$#" -gt 0 ]; then
  files=()
  for f in "$@"; do
    [ -f "$f" ] || { echo "error: no such formula: $f" >&2; exit 2; }
    files+=("$f")
  done
else
  files=("$TAP_DIR"/Formula/*.rb)
fi

status=0
for f in "${files[@]}"; do
  name="$(basename "$f")"

  url=$(ruby -e 'print File.read(ARGV[0])[/^\s*url "(.*)"/, 1].to_s' "$f")
  if [ -z "$url" ]; then
    echo "SKIP  $name (no url field)"
    continue
  fi

  declared=$(ruby -e 'print File.read(ARGV[0])[/^\s*sha256 "([0-9a-f]{64})"/, 1].to_s' "$f")
  if [ -z "$declared" ]; then
    echo "FAIL  $name (no sha256 field)"
    status=1
    continue
  fi

  actual=$(curl -sSfL --retry 3 --max-time 300 "$url" | shasum -a 256 | cut -d ' ' -f1)
  if [ -z "$actual" ]; then
    echo "FAIL  $name (could not fetch $url)"
    status=1
    continue
  fi

  if [ "$declared" = "$actual" ]; then
    echo "OK    $name  $url"
  else
    echo "FAIL  $name  $url"
    echo "        declared: $declared"
    echo "        actual:   $actual"
    status=1
  fi
done

exit $status