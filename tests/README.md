# Tests

Fixture-driven contract tests for `mp`. **Runner not wired yet** — see [docs/TESTING.md](../docs/TESTING.md).

## Layout

- `fixtures/projects/` — hand-crafted `master-plan/` trees (input state)
  - `minimal-ready/` — validate-ok baseline
  - `walkthrough-oauth/` — M03 handoff-ready ([WALKTHROUGH.md](../docs/WALKTHROUGH.md))
  - `hybrid-work/` — work repo with gitignored `.mp/`, tracks, session ([ADOPTION-PROFILES.md](../docs/ADOPTION-PROFILES.md))
  - Fixtures may use `[[work_packages.steps]]` until P1 ([ADR-001](../docs/DECISIONS.md#adr-001-steps-on-disk))
- `scenarios/` — `scenario.json` + `expected/` goldens per test case
  - `p3.1-hybrid-work-validate` — implemented (`mp --plan-dir .mp validate`)
  - `p3.1-hybrid-work-next-track` — implemented
  - `p3.1-hybrid-work-session-show` — planned (P3.1)

## Adding a scenario

1. Create or copy a project under `fixtures/projects/<name>/`.
2. Run `mp` manually (when implementing) and review output.
3. Add `scenarios/<id>/scenario.json` and commit golden `expected/` files.
4. Set `"phase": "implemented"` only when the command exists in Rust.

## Environment

Scenarios assume:

- `MP_HOME` = repository root (has `templates/`, `schemas/`)
- `cwd` = project root (parent of `master-plan/` or `.mp/` for hybrid fixtures)

```bash
make test-fixtures    # validate fixture projects locally
```

Placeholder `{{repo_root}}` in scenario JSON is expanded by the future test runner.
