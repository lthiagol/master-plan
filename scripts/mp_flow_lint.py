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
  6. The SKILL.md role-binding table (`## Role-binding table`, the
     `| Stage | Name | Owner |` table) has one row per stage and every
     row's Stage / Name / Owner matches the manifest's `[[stages]]`
     number / name / role.
  7. The manifest's own `[role_binding.*].stages` lists agree with the
     per-stage `role` fields.

Check 6 is what keeps a rename in one file from silently drifting the
other: before it, a stage could be called "Draft" in stages.toml and
"Define outcome" in the table with the lint still green.

Exits 0 on success, 1 with a precise diff on failure. The lint is
intentionally strict — every stage must be present, every command must
appear in its section, and every role-binding cell must match. Wording
changes are tolerated (the heading match is exact; the command match is
substring).

Usage:
    python3 scripts/mp_flow_lint.py
    python3 scripts/mp_flow_lint.py --json
    python3 scripts/mp_flow_lint.py --skill <path> --manifest <path>
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST_PATH = REPO_ROOT / "templates" / "skills" / "mp-flow" / "stages.toml"
SKILL_PATH = REPO_ROOT / "templates" / "skills" / "mp-flow" / "SKILL.md"

# The role-binding table is identified by heading + exact header cells.
# SKILL.md carries a second `| Stage | Name | …` table (the autopilot
# lane table, third column "Autopilot owner"); scoping to the section
# *and* requiring the exact header keeps that table out of scope.
ROLE_TABLE_SECTION = "Role-binding table"
ROLE_TABLE_HEADER = ("Stage", "Name", "Owner")


def split_row(line: str) -> list[str]:
    """Split one markdown table row into stripped cells."""
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def parse_role_table(body: str) -> tuple[list[tuple[int, str, str]], list[str]]:
    """Parse the role-binding table out of a SKILL.md section body.

    Returns `(rows, errors)` where each row is `(stage_number, name,
    owner)`. Structural problems (no table, ragged rows, non-numeric
    stage) are returned as errors rather than raised, so a single run
    reports everything that is wrong.
    """
    lines = body.splitlines()
    errors: list[str] = []

    header_at = next(
        (
            i
            for i, line in enumerate(lines)
            if line.strip().startswith("|") and tuple(split_row(line)) == ROLE_TABLE_HEADER
        ),
        None,
    )
    if header_at is None:
        return [], [
            f"role-binding table: no `| {' | '.join(ROLE_TABLE_HEADER)} |` table "
            f"in the `## {ROLE_TABLE_SECTION}` section"
        ]

    rows: list[tuple[int, str, str]] = []
    # Skip the header and the |---|---| separator row.
    for line in lines[header_at + 2 :]:
        stripped = line.strip()
        if not stripped:
            continue
        if not stripped.startswith("|"):
            break  # table ended at the first non-row line
        cells = split_row(stripped)
        if len(cells) != len(ROLE_TABLE_HEADER):
            errors.append(
                f"role-binding table: row {stripped!r} has {len(cells)} cells, "
                f"expected {len(ROLE_TABLE_HEADER)}"
            )
            continue
        raw_stage, name, owner = cells
        if not raw_stage.isdigit():
            errors.append(
                f"role-binding table: Stage cell {raw_stage!r} is not a stage number"
            )
            continue
        rows.append((int(raw_stage), name, owner))

    return rows, errors


def check_role_table(
    rows: list[tuple[int, str, str]], stages: list[dict]
) -> list[str]:
    """Diff every role-binding row against the manifest's `[[stages]]`.

    One diagnostic per mismatch, naming the stage, the field, and the
    expected vs actual value.
    """
    errors: list[str] = []
    by_number = {stage["number"]: stage for stage in stages}
    seen: dict[int, int] = {}

    for stage_number, name, owner in rows:
        if stage_number in seen:
            errors.append(
                f"role-binding table: stage {stage_number} appears twice "
                f"(rows {seen[stage_number]} and later)"
            )
        seen[stage_number] = seen.get(stage_number, 0) + 1

        stage = by_number.get(stage_number)
        if stage is None:
            errors.append(
                f"role-binding table: stage {stage_number} is not in stages.toml "
                f"(manifest has {sorted(by_number)})"
            )
            continue

        if name != stage["name"]:
            errors.append(
                f"role-binding table: stage {stage_number} Name mismatch: "
                f"expected {stage['name']!r}, found {name!r}"
            )
        if owner != stage["role"]:
            errors.append(
                f"role-binding table: stage {stage_number} ({stage['name']}) Owner mismatch: "
                f"expected {stage['role']!r}, found {owner!r}"
            )

    for stage in stages:
        if stage["number"] not in seen:
            errors.append(
                f"role-binding table: missing row for stage {stage['number']} "
                f"({stage['name']})"
            )

    if len(rows) != len(stages):
        errors.append(
            f"role-binding table: {len(rows)} row(s), expected {len(stages)}"
        )

    return errors


def check_role_binding_summary(manifest: dict, stages: list[dict]) -> list[str]:
    """Assert `[role_binding.*].stages` agrees with the per-stage `role`."""
    errors: list[str] = []
    role_binding = manifest.get("role_binding", {})
    if not role_binding:
        errors.append("stages.toml: no [role_binding.*] summary to check")
        return errors

    by_number = {stage["number"]: stage for stage in stages}
    listed: dict[int, str] = {}

    for role, config in role_binding.items():
        for stage_number in config.get("stages", []):
            if stage_number in listed:
                errors.append(
                    f"stages.toml: stage {stage_number} is listed under both "
                    f"role_binding.{listed[stage_number]} and role_binding.{role}"
                )
            listed[stage_number] = role
            stage = by_number.get(stage_number)
            if stage is None:
                errors.append(
                    f"stages.toml: role_binding.{role} lists stage {stage_number}, "
                    f"which is not a stage"
                )
            elif stage["role"] != role:
                errors.append(
                    f"stages.toml: role_binding.{role} lists stage {stage_number}, "
                    f"but stages.toml binds it to {stage['role']!r}"
                )

    for stage in stages:
        role = stage["role"]
        if stage["number"] not in listed:
            errors.append(
                f"stages.toml: stage {stage_number_label(stage)} is not listed in "
                f"any [role_binding.*] (role is {role!r})"
            )
    return errors


def stage_number_label(stage: dict) -> str:
    return f"{stage['number']} ({stage['name']})"


def run_lint(manifest_path: Path, skill_path: Path) -> tuple[list[str], list[dict]]:
    """Run every check. Returns `(errors, rows)` where `rows` is the
    parsed role-binding table (one entry per row, with the expected
    manifest values attached for the `--json` view)."""
    errors: list[str] = []
    rows: list[dict] = []

    if not manifest_path.exists():
        errors.append(f"manifest missing: {manifest_path}")
    if not skill_path.exists():
        errors.append(f"SKILL.md missing: {skill_path}")
    if errors:
        return errors, rows

    try:
        manifest = tomllib.loads(manifest_path.read_text())
    except tomllib.TOMLDecodeError as e:
        return [f"stages.toml parse error: {e}"], rows

    stages = manifest.get("stages", [])
    if len(stages) != 12:
        errors.append(f"stages.toml must have exactly 12 stages, found {len(stages)}")
    numbers = sorted(s["number"] for s in stages)
    if numbers != list(range(1, 13)):
        errors.append(f"stage numbers must be 1..12 consecutive, got {numbers}")
    if errors:
        return errors, rows

    skill_text = skill_path.read_text()
    section_pattern = re.compile(
        r"^## (?P<name>[^\n]+?)\s*\n(?P<body>.*?)(?=^## |\Z)",
        re.MULTILINE | re.DOTALL,
    )
    sections = {
        m.group("name").strip(): m.group("body")
        for m in section_pattern.finditer(skill_text)
    }

    # Section + command presence per stage (checks 3-5).
    for stage in stages:
        name = stage["name"]
        body = sections.get(name)
        if body is None:
            errors.append(f"stage {stage_number_label(stage)}: missing `## {name}` section in SKILL.md")
            continue
        commands = stage.get("commands", [])
        if not commands:
            errors.append(f"stage {stage_number_label(stage)}: no commands in stages.toml")
            continue
        missing = [c for c in commands if c not in body]
        if missing:
            errors.append(
                f"stage {stage_number_label(stage)}: section missing commands: {missing}"
            )

    # Role-binding table vs the manifest (check 6).
    table_body = sections.get(ROLE_TABLE_SECTION)
    if table_body is None:
        errors.append(f"SKILL.md: missing `## {ROLE_TABLE_SECTION}` section")
    else:
        table_rows, table_errors = parse_role_table(table_body)
        errors.extend(table_errors)
        if not table_errors:
            errors.extend(check_role_table(table_rows, stages))
            by_number = {stage["number"]: stage for stage in stages}
            for stage_number, name, owner in table_rows:
                stage = by_number.get(stage_number, {})
                expected_name = stage.get("name")
                expected_owner = stage.get("role")
                rows.append(
                    {
                        "stage": stage_number,
                        "name": name,
                        "owner": owner,
                        "expected_name": expected_name,
                        "expected_owner": expected_owner,
                        "ok": name == expected_name and owner == expected_owner,
                    }
                )

    # Manifest-internal role summary (check 7).
    errors.extend(check_role_binding_summary(manifest, stages))

    return errors, rows


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="mp_flow_lint.py",
        description="Assert the mp-flow SKILL.md matches the stages.toml manifest.",
    )
    parser.add_argument(
        "--skill",
        type=Path,
        default=SKILL_PATH,
        help=f"path to the mp-flow SKILL.md (default: {SKILL_PATH})",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=MANIFEST_PATH,
        help=f"path to the stages.toml manifest (default: {MANIFEST_PATH})",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="print a JSON report (parsed role-binding rows + errors) on stdout",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> None:
    args = parse_args(argv)
    errors, rows = run_lint(args.manifest, args.skill)

    if args.json:
        print(
            json.dumps(
                {
                    "ok": not errors,
                    "skill": str(args.skill),
                    "manifest": str(args.manifest),
                    "row_count": len(rows),
                    "rows": rows,
                    "errors": errors,
                },
                indent=2,
            )
        )
        sys.exit(1 if errors else 0)

    if errors:
        print("mp-flow-lint: FAIL", file=sys.stderr)
        for msg in errors:
            print(f"  - {msg}", file=sys.stderr)
        sys.exit(1)

    print(
        f"mp-flow-lint: OK (12 stages, all commands present, "
        f"{len(rows)} role-binding rows match the manifest)"
    )


if __name__ == "__main__":
    main()
