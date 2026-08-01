#!/usr/bin/env bash
# check-consumer-surface.sh — M195: prevent internal-provenance leaks on the
# consumer surface (templates/skills/**, docs/**, adopter-facing READMEs).
#
# Patterns flagged (consumer surface = anything an adopter would read):
#   - \bM\d{2,4}\b   internal milestone IDs (provenance, not capability)
#   - \bL\d{1,3}\b   internal lesson codes (provenance, not capability)
#   - docs/code-review-lessons.md  archived; references are dead
#   - docs/dogfood/…              archived; references are dead
#
# Repository-internal skills (templates/skills/mp-code-review/) are excluded:
# the spec marks them as master-plan-repo-only and exempts them from the
# consumer-surface de-internalization rules.
#
# Inline allowlist: each ALLOW entry is "label:file:line:anchor:reason" and
# matches a known-good exception. The `anchor` field is a substring of the
# matched line content; both line AND anchor must match for the allowlist
# to fire. Anchor on content (not just line number) so adding a line above
# the allowlisted location does not silently break the lint.
#
# Exit 0 when clean, 1 when violations found, 2 when ripgrep is missing,
# 3 on a self-test failure (--self-test mode only).
#
# Wired into `make consumer-surface-lint`; `make lint` runs the same target.

set -euo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

# Patterns. Each entry: "<label>|<rg-regex>".
PATTERNS=(
    "milestone-id|\\bM\\d{2,4}\\b"
    "lesson-code|\\bL\\d{1,3}\\b"
    "dead-code-review-lessons|docs/code-review-lessons\\.md"
    "dead-dogfood|docs/dogfood"
)

# Paths scanned. Repo-relative (the script cd's to the repo root above).
# `mp-code-review/` is repository-internal and excluded. Adopter-facing
# entrypoints (root README.md) are included alongside templates/skills + docs.
SCAN_PATHS=(
    "templates/skills"
    "docs"
    "README.md"
)
EXCLUDE_PATHS=(
    "templates/skills/mp-code-review"
)

# Inline allowlist. Each entry: "<label>:<file>:<line>:<anchor>:<reason>".
# - label: the pattern label, must match one of PATTERNS
# - file: repo-relative path
# - line: 1-indexed line number (documentation only; NOT used for matching)
# - anchor: a substring that must appear in the matched line's content
# - reason: human-readable reason for the exception
# Matching is file + anchor only so inserting a line above an allowlisted
# site does not break the exception. Keep entries short; treat additions
# as recorded debt.
ALLOWLIST=(
    "milestone-id:docs/agent-guide/reading-state.md:43:do M42 before M17:CLI example comment 'do M42 before M17' is a synthetic milestone id; the surrounding command uses bare numeric ids 42/17, so the M-prefixed form in the comment is illustrative only"
    "dead-code-review-lessons:docs/skills/README.md:154:read \`docs/code-review-lessons.md\`:the 'Authoring rules' section uses the dead-link path inside a backticked negative example to teach the rule"
    "dead-code-review-lessons:docs/skills/README.md:191:paths \`docs/code-review-lessons.md\`:the 'Preventing recurrence' section names the dead-link patterns this guard detects"
    "dead-dogfood:docs/skills/README.md:191:docs/dogfood:the 'Preventing recurrence' section names the dead-link patterns this guard detects"
)

# Portable: BSD `mktemp -d` requires a template; GNU accepts `-d` alone.
mktemp_dir() {
    local template="${1:-consumer-surface}"
    mktemp -d -t "${template}.XXXXXX" 2>/dev/null \
        || mktemp -d 2>/dev/null \
        || mktemp -d -t "${template}"
}

mktemp_file() {
    local template="${1:-consumer-surface}"
    mktemp -t "${template}.XXXXXX" 2>/dev/null \
        || mktemp 2>/dev/null \
        || mktemp -t "${template}"
}

build_rg_excludes() {
    local exclude_args=() p
    for p in "$@"; do
        exclude_args+=(--glob "!${p}/**")
    done
    printf '%s\n' "${exclude_args[@]}"
}

usage() {
    cat <<'EOF'
check-consumer-surface.sh — M195 consumer-surface lint

Usage:
    check-consumer-surface.sh          lint the consumer surface (default)
    check-consumer-surface.sh --self-test
                                      create a known-violation fixture, run
                                      the lint against it, and assert the
                                      guard catches it. Exits 0 when the
                                      guard is correct, 3 on a self-test
                                      failure. Use this after editing the
                                      patterns or the allowlist to confirm
                                      the guard is still working.

Exit codes:
    0   clean (no violations)
    1   one or more violations
    2   ripgrep not installed
    3   self-test failure (only with --self-test)
EOF
}

self_test() {
    local workdir fixture_dir

    workdir="$(mktemp_dir consumer-surface-self-test)"
    trap 'rm -rf "$workdir"' EXIT

    fixture_dir="$workdir/fixture"
    mkdir -p "$fixture_dir/templates/skills/example" \
             "$fixture_dir/docs/example"

    # Clean file — must pass the lint.
    cat >"$fixture_dir/templates/skills/example/SKILL.md" <<'CLEAN'
# Example skill
This skill does not leak.
CLEAN

    # Each known-violation case: must be caught by the corresponding pattern.
    cat >"$fixture_dir/docs/example/m99.md" <<'M99'
# M99 is a leaked milestone id
M99 was here.
M99
    cat >"$fixture_dir/docs/example/l5.md" <<'L5'
# L5 lesson code leak
L5 should not be here.
L5
    cat >"$fixture_dir/docs/example/deadlink.md" <<'DEAD'
Read `docs/code-review-lessons.md` here.
DEAD
    cat >"$fixture_dir/docs/example/dogfood.md" <<'DOG'
See `docs/dogfood/M99-audit.md`.
DOG

    # Inline the lint core against the fixture, with empty allowlist. We
    # avoid spawning another shell so the function-local variables stay in
    # scope, and we use a small loop to exercise each pattern.
    local violations_per_pattern=()
    local label pattern matches count i

    for entry in "${PATTERNS[@]}"; do
        label="${entry%%|*}"
        pattern="${entry#*|}"
        matches="$(rg --line-number --no-heading --color=never \
            "$pattern" \
            "$fixture_dir/templates/skills" \
            "$fixture_dir/docs" 2>/dev/null || true)"
        if [ -z "$matches" ]; then
            count=0
        else
            count="$(echo "$matches" | wc -l | tr -d ' ')"
        fi
        violations_per_pattern+=("$label:$count")
    done

    # The fixture must produce at least 1 match per pattern (>= 4 total).
    # We assert per-pattern so a regression in one pattern is reported
    # individually rather than as a single fuzzy total.
    local total=0
    for v in "${violations_per_pattern[@]}"; do
        label="${v%%:*}"
        count="${v#*:}"
        total=$((total + count))
        case "$label" in
            milestone-id|lesson-code|dead-code-review-lessons|dead-dogfood)
                if [ "$count" -lt 1 ]; then
                    echo "self-test FAIL: pattern '$label' should catch >=1 violation, caught $count" >&2
                    return 3
                fi
                ;;
            *)
                echo "self-test FAIL: unknown pattern label '$label'" >&2
                return 3
                ;;
        esac
    done

    if [ "$total" -lt 4 ]; then
        echo "self-test FAIL: expected >=4 violations across the fixture, got $total" >&2
        return 3
    fi

    # Allowlist shift test: file+anchor match must survive a line-number
    # change. Simulate is_allowlisted with a stale line number.
    local allow_entry="milestone-id:docs/example/m99.md:1:M99 was here:shift-test"
    local label_e file_e line_e anchor_e _reason
    IFS=':' read -r label_e file_e line_e anchor_e _reason <<<"$allow_entry"
    local shifted_line=99
    local content_ok="M99 was here."
    if ! { [ "$label_e" = "milestone-id" ] \
        && [ "$file_e" = "docs/example/m99.md" ] \
        && [[ "$content_ok" == *"$anchor_e"* ]]; }; then
        echo "self-test FAIL: file+anchor allowlist match should succeed after line shift" >&2
        return 3
    fi
    # Sanity: wrong anchor must not match.
    local bad_anchor="NOT-THIS-TEXT"
    if [[ "$content_ok" == *"$bad_anchor"* ]]; then
        echo "self-test FAIL: bad anchor unexpectedly matched" >&2
        return 3
    fi
    # shifted_line is intentionally unused — documents that line is ignored.
    : "$shifted_line" "$line_e"

    echo "self-test OK: caught $total violation(s) across the fixture ($(IFS=,; echo "${violations_per_pattern[*]}")); allowlist file+anchor shift ok"
    trap - EXIT
    return 0
}

if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    usage
    exit 0
fi

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit $?
fi

if ! command -v rg >/dev/null 2>&1; then
    echo "error: ripgrep (rg) is required for the consumer-surface lint" >&2
    echo "       install: brew install ripgrep  (or apt-get install ripgrep)" >&2
    exit 2
fi

violations=0
tmp_matches="$(mktemp_file consumer-surface)"
trap 'rm -f "$tmp_matches"' EXIT

# is_allowlisted label file line content — returns 0 if a known-good
# exception applies. Match is file path + content anchor only; the
# allowlist line number is documentation for humans and is ignored so a
# one-line insert above an allowlisted site does not break the exception.
is_allowlisted() {
    local label="$1" file="$2" _line="$3" content="$4"
    local entry label_e file_e _line_e anchor_e _reason
    for entry in "${ALLOWLIST[@]}"; do
        IFS=':' read -r label_e file_e _line_e anchor_e _reason <<<"$entry"
        if [ "$label_e" = "$label" ] \
            && [ "$file_e" = "$file" ] \
            && [[ "$content" == *"$anchor_e"* ]]; then
            return 0
        fi
    done
    return 1
}

# Build rg exclusion args. Done with a simple loop so the script works on
# bash 3.2 (macOS default) where `mapfile` is not available.
rg_excludes=()
while IFS= read -r arg; do
    [ -n "$arg" ] && rg_excludes+=("$arg")
done < <(build_rg_excludes "${EXCLUDE_PATHS[@]}")

for entry in "${PATTERNS[@]}"; do
    label="${entry%%|*}"
    pattern="${entry#*|}"
    echo "==> pattern: $label  ($pattern)"
    # rg exits 1 when no matches; that's clean for this pattern. We capture
    # output and only fail if violations remain after allowlist filtering.
    rg --line-number --no-heading --color=never \
        "${rg_excludes[@]}" \
        "$pattern" "${SCAN_PATHS[@]}" >"$tmp_matches" || true

    if [ ! -s "$tmp_matches" ]; then
        echo "    clean"
        continue
    fi

    # Process each match; surface violations after allowlist filtering.
    # rg's default output is "file:line:content" (no column without
    # --column), and consumer-surface paths do not contain ":", so the
    # first two colon fields are reliably the file and the line number.
    pattern_violations=0
    while IFS= read -r match_line; do
        [ -z "$match_line" ] && continue
        file="${match_line%%:*}"
        rest="${match_line#*:}"
        line_no="${rest%%:*}"
        content="${rest#*:}"
        if is_allowlisted "$label" "$file" "$line_no" "$content"; then
            continue
        fi
        echo "    VIOLATION  $file:$line_no  $match_line"
        pattern_violations=$((pattern_violations + 1))
        violations=$((violations + 1))
    done <"$tmp_matches"

    if [ "$pattern_violations" -eq 0 ]; then
        absorbed="$(wc -l <"$tmp_matches" | tr -d ' ')"
        echo "    clean (allowlist absorbed $absorbed match(es))"
    else
        echo "    $pattern_violations violation(s)"
    fi
done

if [ "$violations" -gt 0 ]; then
    echo ""
    echo "consumer-surface lint: $violations violation(s) found"
    echo "    fix the provenance leak on the consumer surface, or add a"
    echo "    line+anchor allowlist entry to $0 with a recorded reason."
    exit 1
fi

echo ""
echo "consumer-surface lint: clean"
exit 0

