# Testing Strategy — Fixture-Driven TDD

How we test `mp` using hand-crafted fixtures, golden outputs, and scenario manifests.

**Status:** Runner implemented — `make test-scenarios`, `crates/mp/src/scenario.rs`.  
**Counts (2026-06-18):** 49 integration tests (`cargo test -p mp --tests`) + 10 golden scenarios (`"phase": "implemented"`).

See also [PLANNING-STATUS.md](./PLANNING-STATUS.md), [STORAGE.md](./STORAGE.md).

---

## 1. Is now a good time?

**Yes — v1 RC.** The runner, fixtures, and golden scenarios are in CI via `make test-fixtures`
and `make test-scenarios`. Add scenarios when new commands stabilize; use `phase = "planned"`
for work-in-progress specs.

**Runtime matrix for commands under test:** [AGENT-READINESS.md](./AGENT-READINESS.md).

---

## 2. TDD layers

```text
┌─────────────────────────────────────────────────────────┐
│  E2E CLI          mp subprocess, temp copy of fixture    │
│  tests/scenarios/*/                                    │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│  Integration      load JSON → model → validate/render    │
│  tests/fixtures/projects/*                               │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│  Unit             pure fns: id sort, path sort, topo     │
│  inline #[test] + small JSON inputs                      │
└─────────────────────────────────────────────────────────┘
```

| Layer | Tests | Fixtures |
|-------|-------|----------|
| **Unit** | Outline sort (`S3.1 < S3.10`), topo sort, coverage map | Inline or `tests/fixtures/unit/*.json` |
| **Integration** | `validate_plan`, `render` human, JSON round-trip | `tests/fixtures/projects/*` |
| **E2E** | Full CLI: exit code, stdout JSON, filesystem diff | `tests/scenarios/*` |

**Rule:** fixture **project trees are hand-authored** (or curated), not produced by the
CLI under test in the same scenario. That way we test “CLI reaches known good state”
independently.

Exception: **init** scenarios start from empty `input/` and compare `expected/` tree.

---

## 3. Directory layout

```text
tests/
├── README.md
├── fixtures/
│   ├── projects/              # complete master-plan/ trees (input state)
│   │   ├── minimal-ready/
│   │   ├── gate-g1-fail/
│   │   ├── hybrid-work/       # .mp/ layout, hybrid profile (P3.1)
│   │   └── linear-deps/       # M1→M2→M3 for path tests (planned)
│   └── unit/                  # small JSON inputs for pure functions
│       └── outline-sort.json
└── scenarios/                 # one folder per test case
    ├── p0-validate-ok/
    │   ├── scenario.json
    │   └── expected/
    │       └── validate.json
    └── p0-validate-g1-fail/
        ├── scenario.json
        └── expected/
            └── validate.json
```

**Toolkit (`MP_HOME`):** scenarios use `{{repo_root}}` — the repository root contains
`templates/` and `schemas/`. No duplicate toolkit copy unless we need isolation.

**Project root:** `fixture/projects/<name>/` contains `master-plan/` (and optional
`AGENTS.md`). Scenario `cwd` is the project root (parent of `master-plan/`).

---

## 4. Scenario manifest (`scenario.json`)

```json
{
  "id": "p0-validate-ok",
  "phase": "implemented",
  "description": "Minimal valid plan passes validate",
  "fixture": "projects/minimal-ready",
  "command": ["validate", "--format", "json"],
  "env": {
    "MP_HOME": "{{repo_root}}"
  },
  "assert": {
    "exit_code": 0,
    "stdout_json_file": "expected/validate.json",
    "fs_unchanged": true
  }
}
```

| Field | Meaning |
|-------|---------|
| `phase` | `implemented` \| `planned` — planned scenarios skip CI until ready |
| `fixture` | Path under `tests/fixtures/` |
| `command` | Args after `mp` |
| `assert.exit_code` | Process exit code |
| `assert.stdout_json_file` | Golden JSON (full or subset match — TBD in runner) |
| `assert.stdout_contains` | Substring match for human output |
| `assert.fs_diff` | Path to expected tree after mutating commands (init, track add) |
| `assert.fs_unchanged` | No writes for read-only commands |

**Mutating scenario example (future):**

```json
{
  "id": "p0-init-fresh",
  "phase": "implemented",
  "fixture": "projects/empty",
  "command": ["init", "--format", "json"],
  "assert": {
    "exit_code": 0,
    "fs_diff": "expected/master-plan"
  }
}
```

---

## 5. What to mock vs generate

| Asset | Source | Used for |
|-------|--------|----------|
| `master-plan/*.json` | Hand-written fixtures | Input state |
| `expected/*.json` | Captured once, reviewed, committed | CLI stdout goldens |
| `expected/master-plan/` | Hand-written or diff-reviewed | After init/write commands |
| Human `.md` views | Optional goldens in `expected/human/` | Render snapshot tests |
| `templates/` | Repo root (real) | init, render |
| `schemas/` | Repo root (real) | validation, interview |

**Translate tests** (integration, no CLI):

- JSON on disk → `MilestoneFile` → JSON CLI shape → compare golden
- JSON stdin payload → write JSON → compare fixture file

---

## 6. Phased rollout

| When | Add |
|------|-----|
| **Now (scaffold)** | `docs/TESTING.md`, `tests/` layout, P0 validate scenarios |
| **Makefile** | `make test-fixtures` — validate fixture projects |
| **P3.1 hybrid** | `hybrid-work` fixture, `p3.1-hybrid-work-*` scenarios — [ADOPTION-CHECKLIST.md](./ADOPTION-CHECKLIST.md) |
| **Doctor / brownfield** | Doctor JSON goldens; `brownfield-likely` fixture project (planned) |
| **Rust test harness** | `crates/mp/tests/cli.rs` or `tests/runner.rs`, copy fixture to tempdir |
| **Per command ship** | Scenario + golden when command implemented |
| **P1.8 path engine** | `linear-deps` fixture, `expected/path.json` |
| **P0.5 brief** | `brief-in-progress` fixture |

---

## 7. Scenario runner (implemented)

See `crates/mp/src/scenario.rs` and `make test-scenarios`. Skips scenarios with
`"phase": "planned"` unless the `planned` feature is enabled.

---

## 8. References

- [../tests/README.md](../tests/README.md) — fixture conventions
- [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) — command contracts
- [PLANNING-STATUS.md](./PLANNING-STATUS.md) — what's implemented
