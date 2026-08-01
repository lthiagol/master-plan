#!/usr/bin/env bash
# Linux-only linker shim: use mold when installed, else system ld.
set -euo pipefail
if [[ "$(uname -s)" != "Linux" ]]; then
  exec clang "$@"
fi
for mold in /usr/bin/mold /usr/local/bin/mold /opt/homebrew/bin/mold; do
  if [[ -x "$mold" ]]; then
    exec clang -fuse-ld="$mold" "$@"
  fi
done
exec clang "$@"
