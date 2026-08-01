#!/usr/bin/env bash
# scripts/verify-m162-no-spawns.sh
#
# M162 verification: scan the named suite aggregator(s) + their
# #[path]-included sub-modules for subprocess spawn sites
# (env.run / run_at_repo / run_with_env / Command::new(mp_bin())).
# Exits non-zero if any are found in a "convertible" suite.
#
# Usage:
#   scripts/verify-m162-no-spawns.sh                    # scan all top-5 suites
#   scripts/verify-m162-no-spawns.sh suite_validate     # scan one suite
#   scripts/verify-m162-no-spawns.sh suite_validate suite_fragment
#
# Top-5 suites are the M162 AC-02 list (in scope). Suites on the
# "MUST stay subprocess" list (install, doctor, watch, TUI) are not
# included — see docs/concepts/03 - Testing/test-taxonomy.md.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SUITES_DIR="$ROOT/crates/mp/tests"

# Top-5 most-spawned suites per M162 AC-02.
DEFAULT_SUITES=(suite_validate suite_fragment suite_milestone suite_step suite_plan)

# Patterns that count as a subprocess spawn.
SPAWN_PATTERNS=(
    'env\.run\('
    'env\.run_at_repo\('
    'env\.run_with_env\('
    'Command::new\(mp_bin\(\)\)'
)

# If no args given, scan the top-5 suites.
if [[ $# -eq 0 ]]; then
    set -- "${DEFAULT_SUITES[@]}"
fi

failures=0
scanned=0

scan_file() {
    local file="$1"
    local rel="${file#"$ROOT"/}"
    local in_match=0
    for pattern in "${SPAWN_PATTERNS[@]}"; do
        if grep -nE "$pattern" "$file" >/dev/null 2>&1; then
            echo "FAIL: $rel contains subprocess spawn: $pattern" >&2
            grep -nE "$pattern" "$file" | sed 's/^/    /' >&2
            in_match=1
        fi
    done
    return $in_match
}

scan_suite() {
    local suite_name="$1"
    local agg="$SUITES_DIR/${suite_name}.rs"
    if [[ ! -f "$agg" ]]; then
        echo "WARN: $agg not found — skipping" >&2
        return 0
    fi
    scanned=$((scanned + 1))
    echo "Scanning $suite_name ..."
    # Scan the aggregator file itself.
    if ! scan_file "$agg"; then
        failures=$((failures + 1))
    fi
    # Find every #[path = "..."] line and scan those files too.
    # Allow either single or double quotes around the path.
    while IFS= read -r subpath; do
        local sub_file="$SUITES_DIR/$subpath"
        if [[ -f "$sub_file" ]]; then
            if ! scan_file "$sub_file"; then
                failures=$((failures + 1))
            fi
        fi
    done < <(grep -oE '#\[path\s*=\s*"[^"]+"\]' "$agg" | sed 's|#\[path[[:space:]]*=[[:space:]]*"\(.*\)"\]|\1|')
}

for s in "$@"; do
    scan_suite "$s"
done

echo ""
echo "Scanned $scanned suite aggregator(s); failures=$failures"

if [[ $failures -gt 0 ]]; then
    echo "M162 AC-02: FAIL — at least one top-5 suite still uses subprocess spawns." >&2
    exit 1
fi

echo "M162 AC-02: PASS — no subprocess spawns in the requested suite(s)."
exit 0