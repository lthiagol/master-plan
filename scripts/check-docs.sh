#!/usr/bin/env bash
# check-docs.sh — Verify files referenced in step file lists exist on disk.
set -e
cd "$(git rev-parse --show-toplevel 2>/dev/null || exit 1)"
export MP_HOME="${MP_HOME:-$(pwd)}"
MP="${MP_HOME}/target/release/mp"
[ -x "$MP" ] || { echo "error: mp not found at $MP"; exit 1; }

$MP list steps --format json 2>/dev/null | jq -r '
  .steps[].step | (.files // [])[]' | grep -v '^$' | sort -u > /tmp/oc-docs-refs

missing=0
while IFS= read -r f; do
    [ -f "$f" ] && continue
    echo "MISSING: $f"
    missing=$((missing + 1))
done < /tmp/oc-docs-refs

echo "Checked $(wc -l < /tmp/oc-docs-refs) file references, $missing missing"
[ "$missing" -eq 0 ] && echo "OK" || exit 1