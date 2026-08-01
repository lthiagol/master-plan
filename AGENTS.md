# Master Plan toolkit — agent instructions

This repo builds the **`mp` CLI** (`crates/mp/`) and the **`raul` CLI** (`crates/raul/`)
— a spec-driven project planner for coding agents. We **dogfood** the product: the
repo's own plan lives in `master-plan/`.

---

## Plan zone vs code zone

| Zone | Paths | Rule |
|------|-------|------|
| **Plan** | `master-plan/` | All reads/writes via `mp`. Never hand-edit plan files. |
| **Code** | `crates/mp/`, `crates/raul/`, `tests/`, `docs/`, root config | Normal dev |

**Spec before code:** no changes in `crates/mp/` until the relevant milestone is approved
(`lifecycle: approved`). After every `mp` write: `mp validate`.

Plan workflows (session start, execute, review): [**master-plan/AGENTS.md**](master-plan/AGENTS.md).
Command reference: [**docs/mp/commands.md**](docs/mp/commands.md). Agent orientation: [**docs/agent-guide/README.md**](docs/agent-guide/README.md).

> Hacking on the `mp` binary? Run `eval "$(make dev-env)"` to repoint `MP_HOME`/`PATH`
> at `target/release/mp`. Don't leave it on for plan/PM work — it shadows the installed product.

---

## Project mission & agent contract

This repository is the build target AND a first-class consumer of master-plan:
`./master-plan/` is the repo's own plan, and `mp` reads/writes it. The agent
running here maintains the toolkit and dogfoods the toolkit at the same time.
Bugs surfaced while planning this project are real bugs, not test noise.

### Two objectives for master-plan

1. **Standardize a project-management layer** across any project that adopts it,
   driven entirely through the `mp` CLI and its tools.
2. **Move formatting and structural bookkeeping off the agent's plate.** The
   agent's job is to plan and groom well; `mp` owns format, storage, and
   validation.

### Two CLIs, two audiences — non-negotiable routing

| CLI | Audience | Default output |
|-----|----------|----------------|
| `mp`  | Agent | JSON |
| `raul` | Human | TUI |

**Agent rule:** interact with the plan only through `mp`. Never invoke `raul`.
Humans use `raul` separately for status and review views.

### Conventions that follow from the above

- **Plan zone is `mp`-mediated.** No hand-edits of any file under `master-plan/`.
  Use `mp` for every read and every write. (Already a Non-Negotiable Rule; restated
  here because dogfooding is where the temptation is highest.)
- **Workaround queue = `mp-dogfood-log.md`** (repo root, one entry per finding):
  - Date / when.
  - Command attempted (or observed `mp` output, including exit code).
  - Suspected cause and the `mp` subcommand or code path involved.
  - One-line verdict: `wontfix | backlog | spec-gap | bug`.
  Treat the log as triage input for the next dogfood pass. Do NOT patch the symptom
  in `master-plan/` files or in code without recording the finding first.
- **Consumer-surface hygiene — no internal provenance.** Files that ship to
  adopters (`templates/skills/**`, `docs/**` except `docs-old/`, user-facing
  READMEs) must be self-contained: no internal milestone IDs (`M\d+`), no
  lesson codes (`L\d`), and no pointers to repo-internal files
  (`docs/code-review-lessons.md`, `docs/dogfood/…`). Name the *capability*
  (`mp config get agent.automation.branch_strategy`), not the milestone that
  introduced it. Milestone IDs remain the native vocabulary inside
  `master-plan/` and dogfood-only notes. Authoring detail + fix patterns:
  `docs/skills/README.md#authoring-rules--keep-the-consumer-surface-self-contained`.

---

## Dev commands

`make test` runs **`cargo nextest run` + `cargo fmt --check`**; lint is a
separate target (`make lint`). Agents should prefer the same toolchain directly —
`nextest` is parallel, ~10× faster than `cargo test`, and gives per-test
pass/fail with retries; `cargo test` is a serial fallback only.

| Command | What |
|---------|------|
| `make test` | `cargo nextest run` + `cargo fmt --check` (no clippy). Requires `mp` on PATH/`MP_HOME`. With `NEXTTEST=1` uses nextest `--profile ci` (`fail-fast=false`) |
| `make lint` | `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` + consumer-surface leak guard (CI runs clippy from `plan.yml` directly) |
| `make consumer-surface-lint` | ripgrep guard over the consumer surface (templates/skills/** + docs/**); flags internal milestone IDs, lesson codes, and dead doc pointers |
| `make ci` | `make lint` + `make test` + `mp-flow-lint` + `test-scenarios` — requires `mp` (preflight). Used by `wip-ci.yml` / `stable-ci.yml` after putting `target/release` on PATH; locally: `eval "$(make dev-env)"` |
| `make mp-flow-lint` | Assert mp-flow SKILL.md matches the 12-stage `stages.toml` |
| `make check` | `cargo check` (fast, no link) |
| `make build` | Release build of all workspace binaries (mp + raul) |
| `make test-serial` | Serial `cargo test` + fmt (no clippy; fallback when nextest is unavailable) |
| `make test-fixtures` | `mp validate` on fixture projects (run when touching validation) |
| `make test-scenarios` | Golden CLI scenario tests (`--test scenarios_runner`) |
| `make regen-goldens` | Rewrite committed JSON goldens under `tests/fixtures/` (json-shape + track). Not CI — run only after intentional schema changes, then review the diff |
| `make adopt-check` | Validate full + hybrid fixture paths |
| `make dep-audit` | mp transitive count gate (≤150) + explicit feature pins |
| `make dep-audit-raul` | raul: single crossterm 0.29, no comfy-table/owo-colors, ≤100 transitive |
| `make doctor` | Health check (dev mode) |
| `make clean` | `cargo clean` (build artifacts only — does **not** touch goldens) |
| `make install` | Global install: toolkit + OpenCode + Cursor + Pi harnesses |
| `scripts/audit-step-tests.sh` | Check all plan steps have valid `tests` values |
| `scripts/audit-stub-tests.sh` | Fail on compile-only integration test stubs |

### CI-parity local runs

`ubuntu-latest` has no `mp` on PATH. Reproduce that locally, then put the
just-built release bin first (same as CI / `make dev-env`):

```bash
make build
env -i HOME=$HOME PATH=$PWD/target/release:$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin \
  NEXTTEST=1 make ci
# Or full surface without make:
env -i HOME=$HOME PATH=$PWD/target/release:$HOME/.cargo/bin:/usr/bin:/bin \
  cargo nextest run --profile ci --no-fail-fast
```

Always prefer `--no-fail-fast` / `--profile ci` when hunting regressions —
nextest's default `--max-fail=1` hides later failures.

### Plan-zone noise after validate-style targets

`mp validate`, `make test-fixtures`, `make adopt-check`, and `make check` can
append to `master-plan/activity.json` (and sometimes touch config). That is
expected bookkeeping, not product work — discard before commit:

```bash
git checkout -- master-plan/activity.json master-plan/config.json
```

### Direct nextest / fmt / clippy usage

Use these directly when writing acceptance criteria or verifying a fix —
they are the source of truth, `make test` just orchestrates them.

```bash
# Run the full suite in parallel with --no-fail-fast so every failure
# surfaces (don't stop at the first red).
cargo nextest run --no-fail-fast --manifest-path Cargo.toml

# Filter to a single crate / test name / substring — much faster than
# building everything when you know the surface.
cargo nextest run -p mp --test config_set --no-fail-fast
cargo nextest run -p raul --no-fail-fast -E 'test(/tui_settings/)'
cargo nextest run -p mp --no-fail-fast -E 'not test(/some_slow_test/)'

# Format check + clippy with -D warnings (treats warnings as errors).
# Note: `make lint` uses the dev profile (faster local iteration); CI
# in .github/workflows/plan.yml uses `cargo clippy --release --all-targets`
# for a stricter gate. Both must remain clean — use the release form
# below when reproducing the exact CI gate.
cargo fmt --all -- --check
cargo clippy --manifest-path Cargo.toml --all-targets -- -D warnings
# Exact CI reproduction:
cargo clippy --release --manifest-path Cargo.toml --all-targets -- -D warnings

# Per-crate variant when you only touched one crate.
cargo clippy -p mp --tests --no-deps -- -D warnings
cargo clippy -p raul --tests --no-deps -- -D warnings
```

### Test idioms for acceptance criteria

When writing a milestone's `acceptance_criteria[AC-NN].verification` and
step `tests` fields, prefer **observable commands over prose** — a future
agent (or you, after context has decayed) should be able to copy/paste
the line and see pass/fail:

- ✅ `cargo nextest run -p mp --test config_set --no-fail-fast`
- ✅ `cargo clippy -p mp --tests --no-deps -- -D warnings`
- ✅ `cargo fmt --all -- --check`
- ❌ "all mp tests pass" — pin the command, not the outcome.
- ❌ `cargo test -p mp` — serial; prefer `cargo nextest run`.

For full-suite acceptance use `make test`. For crate-scoped acceptance
(the common case after a focused change) use the per-crate form above.
A step that only touches one file should pin the test name or test-file
substring so the agent verifying it does not rebuild the world.

---

## Agent essentials

`mp` read commands emit **JSON by default** — omit `--format json`. Use `--fields` for
projection, `--summary` for health rollups. Human UI: launch `raul` (TUI).
Exception: `mp backlog add` prefixes stdout with `Assigned: B-<n>`
before the JSON body — skip the first line when parsing JSON.

**Fragment-first reads/writes:** edit one AC / step / WP at a time via
`mp milestone ac|step|wp show|add|update|remove <id> [<fragment-id>]`. Use
`--fields 'acceptance_criteria[AC-03]'` to project one element by stable id.
**Do not rebuild `acceptance_criteria` / `steps` arrays via `mp milestone update --json`** —
that path is rejected by default; `--replace-arrays` is a migration escape hatch only.
Full command surface: [docs/mp/commands.md](docs/mp/commands.md).

**Bulk milestone metadata:** for multi-id `set-priority`,
`set-spec-status`, `depends-on add|remove`, use `mp milestone bulk …` instead
of shell-for-loops over single-id commands. Targets resolve via `--ids …, …`
and/or `--where 'field==value'`; `--dry-run` previews without writing;
per-id results report succeeded/failed with exit code `2` on partial failure.
**Anti-pattern:** `for id in 82 92 93; do mp milestone set-priority $id high; done`.

**Search-first discovery:** to find content in the plan — an AC, a
step, a work package, an idea — use `mp search <query> [--type ac|step|wp|…]
[--include object]`. Hits carry a `suggested_action` that maps to the
matching fragment command. `--include object` embeds the full matched
fragment so agents can skip a second `mp show`. **Anti-pattern:**
`grep master-plan/` or `rg master-plan/`.

**Permanent rules:** never complete on red tests · never hand-edit `master-plan/` ·
evidence is test output not prose · `--force` is recorded debt · review is mandatory
for milestones (`mp reviews pass`). Full rules, loop guard, and review flow:
`master-plan/AGENTS.md`.

**Review findings:** `mp reviews finding list <id>` · resolve one with
`mp reviews finding resolve <id> <F-XX>` · bulk-resolve all open findings with
`mp reviews finding resolve <id> --all`.

**Edit-tool batch correctness:** the external `edit` tool's batch
success counter can report `Successfully replaced N block(s)` even when
the underlying `oldText` was not unique and only some sites changed
(see dogfood log entry 17 sub-2). Treat the tool's success counter as
informational; verify with a grep before trusting the result:

```bash
# After a batch edit, count the new pattern; compare against the batch size.
grep -c '<expected-new-text>' path/to/file.rs
```

If the grep count disagrees with the batch size, the edit silently
dropped some sites — re-run per-site with uniquely matched `oldText`,
or fall back to `mp milestone update --json @file` (path-mode;
see `mp scratch` below).

**Temporary workspace:** for `mp milestone update --json @file`,
stash the JSON payload under a scratch path rather than the repo root:

```bash
SCRATCH=$(mp scratch new m-update)
cat > "$SCRATCH/payload.json" <<'JSON'
{"intent": {"outcome": "..."}}
JSON
mp milestone update <id> --json @"$SCRATCH/payload.json"
```

`mp scratch new <label>` is the workspace primitive — see `mp scratch --help`
for the subcommand surface (`path`, `new`). **Anti-pattern:** writing
`/tmp/<random>.json` directly; agents have been bitten by clean-up timing
on long-running commands.

---

## Key references

- `master-plan/AGENTS.md` — full plan workflows
- `ARCHITECTURE.md` — codebase map, layering, entry-point recipes
- `docs/README.md` — documentation index
- `docs/agent-guide/README.md` — agent orientation + per-workflow detail
- `docs/mp/commands.md` — CLI reference
- `docs/milestone-lifecycle/` — lifecycle state machine + gates
- `docs/milestone-details/` — data model & field reference

> The previous `docs/concepts/` tree is archived under `docs-old/` (unmaintained).
