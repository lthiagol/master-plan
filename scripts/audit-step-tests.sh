#!/usr/bin/env bash
# audit-step-tests.sh — Categorize every step's `tests` field and report gaps.
# Exit 0 if no empties or missing-on-disk refs remain on DONE milestones, 1 otherwise.
# Test files referenced by not-yet-done (planned/in-progress) milestones are
# expected to be absent and are reported separately, not failed.
set -e

cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
export MP_HOME="${MP_HOME:-$(pwd)}"

# Resolve mp: repo release build, then MP_HOME/bin, then PATH.
MP=""
for cand in "${MP_HOME}/target/release/mp" "${MP_HOME}/bin/mp" "$(command -v mp)"; do
    if [ -n "$cand" ] && [ -x "$cand" ]; then MP="$cand"; break; fi
done
if [ -z "$MP" ]; then
    echo "error: mp binary not found (looked in MP_HOME/target/release, MP_HOME/bin, PATH)"
    exit 1
fi

command -v jq >/dev/null || { echo "error: jq required"; exit 1; }

# Fetch all steps + the set of done milestone ids.
STEPS=$("$MP" list steps --format json 2>/dev/null) || { echo "error: mp list steps failed"; exit 1; }
DONE_MS=$("$MP" list milestones --format json 2>/dev/null | jq -r '[.milestones[] | select(.execution_status=="done") | .id] | join(" ")')

is_done() { echo "$DONE_MS" | grep -qw "$1"; }

# Categorize
empty=0
missing=0
manual=0
comma=0
adhoq=0
okay=0
future=0
declare -a missing_files

while IFS= read -r line; do
    id=$(echo "$line" | jq -r '.id')
    milestone=$(echo "$line" | jq -r '.milestone')
    tests=$(echo "$line" | jq -r '.tests // ""')

    if [ -z "$tests" ]; then
        empty=$((empty + 1))
        continue
    fi

    case "$tests" in
        manual:*)
            manual=$((manual + 1))
            ;;
        *,*)
            comma=$((comma + 1))
            ;;
        *[!\ ]*/*)
            # Looks like a path or command. Check if it's a known shell command.
            if echo "$tests" | grep -qE '^(cargo |make |\./|scripts/)'; then
                adhoq=$((adhoq + 1))
            elif echo "$tests" | grep -qE '\.rs$'; then
                if [ -f "$tests" ]; then
                    okay=$((okay + 1))
                elif is_done "$milestone"; then
                    missing=$((missing + 1))
                    missing_files+=("$tests on $id (M$milestone)")
                else
                    future=$((future + 1)) # not-yet-executed milestone: expected absent
                fi
            else
                adhoq=$((adhoq + 1))
            fi
            ;;
        *)
            adhoq=$((adhoq + 1))
            ;;
    esac
done < <(echo "$STEPS" | jq -c '.steps[] | {id: .step.id, milestone: .milestone, tests: .step.tests}')

# Report
echo "=== Step tests audit ==="
echo "  empty (G10):  $empty"
echo "  manual:       $manual"
echo "  comma-list:   $comma"
echo "  ad-hoc:       $adhoq"
echo "  ok:           $okay"
echo "  missing-ref:  $missing  (done milestones only)"
echo "  future-ref:   $future  (not-yet-done milestones; expected absent)"
echo "  total:        $((empty + missing + manual + comma + adhoq + okay + future))"
echo ""

if [ ${#missing_files[@]} -gt 0 ]; then
    echo "--- Missing referenced files (done milestones) ---"
    for f in "${missing_files[@]}"; do echo "  $f"; done
    echo ""
fi

rc=0
if [ "$empty" -gt 0 ]; then
    echo "FAIL: $empty steps have empty tests (G10)"
    rc=1
fi
if [ "$missing" -gt 0 ]; then
    echo "FAIL: $missing steps reference files not on disk"
    rc=1
fi

if [ "$rc" -eq 0 ]; then
    echo "OK: no empty tests or missing file refs on done milestones"
fi

exit $rc
