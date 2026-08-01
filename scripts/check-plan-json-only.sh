#!/usr/bin/env bash
# check-plan-json-only.sh — M92 AC-02 gate: no .toml plan artifacts remain under
# the dogfood master-plan/ or any tests/fixtures/projects/* plan dir.
# Exit 0 if the plan tree is JSON-only, 1 if any stray .toml is found.
#
# Exemptions (must stay TOML by design):
#   - repo-root legacy-toml/          frozen pre-M92 rollback snapshot (outside plan dirs)
#   - Cargo.toml / pyproject.toml     project manifests, not plan artifacts
# These are outside the scanned plan dirs, so no explicit exemption list is needed.

set -euo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

status=0
found=0

scan_dir() {
    local label="$1"
    local dir="$2"
    if [ ! -d "$dir" ]; then
        return 0
    fi
    # Find .toml files; legacy-toml snapshots inside a plan dir would be caught here,
    # but M92 relocated the dogfood snapshot to the repo root.
    local matches
    matches=$(find "$dir" -type f -name '*.toml' 2>/dev/null || true)
    if [ -n "$matches" ]; then
        echo "FAIL: $label has .toml plan artifacts (M92 requires JSON-only):"
        echo "$matches" | sed 's/^/  /'
        found=1
        status=1
    else
        echo "ok: $label is JSON-only"
    fi
}

scan_dir "dogfood master-plan/" "master-plan"

# Every fixture project's plan dir (master-plan/ or .mp/).
for proj in tests/fixtures/projects/*/; do
    [ -d "$proj" ] || continue
    [ -f "${proj}.gitkeep" ] && continue  # empty fixture (e.g. write-blank)
    scan_dir "fixture $proj" "${proj}master-plan"
    scan_dir "fixture $proj (.mp)" "${proj}.mp"
done

exit $status
