#!/usr/bin/env bash
# audit-stub-tests.sh — fail when integration tests are compile-only stubs.
set -euo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

collect_dirs() {
  printf '%s\n' \
    crates/mp/tests \
    crates/raul/tests \
    crates/mp-model/tests \
    tests
  find crates -type d -name tests 2>/dev/null || true
}

stubs_file=$(mktemp)
trap 'rm -f "$stubs_file"' EXIT

while IFS= read -r TESTS_DIR; do
  [ -d "$TESTS_DIR" ] || continue
  while IFS= read -r f; do
    [ -f "$f" ] || continue
    if rg -q 'fn [a-zA-Z0-9_]+_compiles\(\) \{\}' "$f" 2>/dev/null; then
      echo "$f" >>"$stubs_file"
      continue
    fi
    if rg -q '#\[test\]' "$f" \
      && ! rg -q 'assert!|assert_eq!|assert_ne!|panic!|env\.run|Command::|mp::|raul::' "$f"; then
      echo "$f" >>"$stubs_file"
    fi
  done < <(rg -l '#\[test\]' "$TESTS_DIR" -g '*.rs' 2>/dev/null || true)
done < <(collect_dirs | sort -u)

stubs=$(sort -u "$stubs_file" | sed '/^$/d' || true)

echo "=== Stub test audit (crates/*/tests + tests/) ==="
if [ -z "$stubs" ]; then
  echo "OK: no compile-only stub tests"
  exit 0
fi

count=$(printf '%s\n' "$stubs" | wc -l | tr -d ' ')
echo "FAIL: ${count} stub test file(s):"
printf '%s\n' "$stubs" | sed 's/^/  /'
exit 1
