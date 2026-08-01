#!/usr/bin/env python3
"""make mp-flow-lint: assert templates/skills/mp-flow/SKILL.md matches stages.toml.

The stages.toml manifest is the source of truth for the 12-stage mp-flow
lifecycle. The SKILL.md is the agent-facing reference. This lint enforces
that:

  1. stages.toml exists and is valid TOML.
  2. stages.toml has exactly 12 stages, numbered 1..12.
  3. Each stage has a name (used as the ## heading in SKILL.md).
  4. SKILL.md has a `## <name>` section for every stage.
  5. Each section contains at least one of the `mp` commands listed in
     stages.toml.

Exits 0 on success, 1 with a precise diff on failure. The lint is
intentionally strict — every stage must be present, every command must
appear in its section. Wording changes are tolerated (the heading match
is exact; the command match is substring).

Usage:
    python3 scripts/mp_flow_lint.py
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST_PATH = REPO_ROOT / "templates" / "skills" / "mp-flow" / "stages.toml"
SKILL_PATH = REPO_ROOT / "templates" / "skills" / "mp-flow" / "SKILL.md"


def fail(messages: list[str]) -> None:
    print("mp-flow-lint: FAIL", file=sys.stderr)
    for msg in messages:
        print(f"  - {msg}", file=sys.stderr)
    sys.exit(1)


def main() -> None:
    if not MANIFEST_PATH.exists():
        fail([f"manifest missing: {MANIFEST_PATH} (M120 ships stages.toml)"])
    if not SKILL_PATH.exists():
        fail([f"SKILL.md missing: {SKILL_PATH}"])

    try:
        manifest = tomllib.loads(MANIFEST_PATH.read_text())
    except tomllib.TOMLDecodeError as e:
        fail([f"stages.toml parse error: {e}"])

    stages = manifest.get("stages", [])
    if len(stages) != 12:
        fail([f"stages.toml must have exactly 12 stages, found {len(stages)}"])

    numbers = sorted(s["number"] for s in stages)
    if numbers != list(range(1, 13)):
        fail([f"stage numbers must be 1..12 consecutive, got {numbers}"])

    skill_text = SKILL_PATH.read_text()
    section_pattern = re.compile(
        r"^## (?P<name>[^\n]+?)\s*\n(?P<body>.*?)(?=^## |\Z)",
        re.MULTILINE | re.DOTALL,
    )
    sections = {
        m.group("name").strip(): m.group("body")
        for m in section_pattern.finditer(skill_text)
    }

    errors: list[str] = []
    for stage in stages:
        name = stage["name"]
        body = sections.get(name)
        if body is None:
            errors.append(f"stage {stage['number']} ({name}): missing `## {name}` section in SKILL.md")
            continue
        commands = stage.get("commands", [])
        if not commands:
            errors.append(f"stage {stage['number']} ({name}): no commands in stages.toml")
            continue
        missing = [c for c in commands if c not in body]
        if missing:
            errors.append(
                f"stage {stage['number']} ({name}): section missing commands: {missing}"
            )

    if errors:
        fail(errors)

    print(f"mp-flow-lint: OK ({len(stages)} stages, all commands present)")


if __name__ == "__main__":
    main()