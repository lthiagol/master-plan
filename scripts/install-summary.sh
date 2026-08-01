#!/usr/bin/env bash
# install-summary.sh — human-friendly postamble for mp install JSON (stdin).
set -euo pipefail

python3 -c '
import json
import sys

report = json.load(sys.stdin)
mp_home = report.get("mp_home", "")
harnesses = report.get("harnesses") or []

print("Installed toolkit → " + mp_home)
if harnesses:
    print("Harnesses: " + ", ".join(harnesses))
else:
    print("Harnesses: (toolkit only)")

print(
    "Note: templates/schemas are embedded in the binary; "
    "on-disk copies are an optional override tree."
)

doctor = report.get("doctor") or {}
checks = doctor.get("checks") or []
path_check = next((c for c in checks if c.get("name") == "runtime:mp_on_path"), None)

print("")
if path_check:
    msg = path_check.get("message", "mp resolves on PATH")
    if path_check.get("ok"):
        print("✓ " + msg)
    else:
        print("→ " + msg)
        env_sh = mp_home + "/env.sh"
        print("  Agent shells: source \"" + env_sh + "\"")
        snippet = report.get("path_snippet", "")
        if snippet:
            print("  Or add to your shell rc:")
            for line in snippet.splitlines():
                print("    " + line)
else:
    snippet = report.get("path_snippet", "")
    if snippet:
        print("Shell snippet:")
        print(snippet)

print("")
print("Done. Verify: mp doctor")
'
