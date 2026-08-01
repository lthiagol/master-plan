# `mp` architecture

> **Last updated:** 2026-07-10 — module locality splits (`cli/`, `milestone/{io,spec,complete}`, raul `render/*`).

---

## 1. Module map

| Module | Responsibility |
|--------|---------------|
| `main.rs` | Binary entry point (minimal — call cli) |
| `cli/` | Clap CLI surface split by command group (`cli/mod.rs` + `cli/{milestone,plan,reviews,…}.rs`); stable `crate::cli::*` re-exports |
| `milestone/` | Domain CRUD split: `io` (load/write), `spec` (create/update/lifecycle), `complete` (verify/complete); stable `crate::milestone::*` re-exports |
| `app.rs` | Command dispatch (`cmd_*` routers, ≤400 lines) |
| **`commands/`** | Handler modules (one per top-level command) |
| `commands/common.rs` | Shared helpers: `emit()`, `emit_gate_failure()`, `read_evidence()`, `milestone_summary()` |
| `commands/mod.rs` | `mod` declarations |
| `model.rs` | Re-exports `mp-model` domain types (`MilestoneFile`, `TrackFile`, `PlanFile`, …) |
| `store.rs` | JSON I/O — load/save, atomic write, bounded reads, `init`, `sync` |
| `schema.rs` | JSON Schema validation dispatch (delegates to `mini_schema`) |
| `mini_schema.rs` | Minimal JSON Schema validator (M21, 10 keywords, no jsonschema in binary) |
| `assets.rs` | Embedded templates + schemas (M29, `include_dir`); `read_embedded`, `toolkit_home` |
| `step.rs` | Step CRUD: add, update, set-status, done, fail, split |
| `wp.rs` | Work package CRUD |
| `paths.rs` | `PlanContext` — project-root resolution, plan-dir discovery, safe path-segment helpers |
| `path_engine.rs` | `mp path` / `mp next` — execution order, adoption pins, strategy |
| `plan_gaps.rs` | Per-milestone readiness/coverage (`mp plan gaps`, decompose scaffolding) |
| `track_kind.rs` | Track kind enum (`bugfix`, `tweak`, `chore`) |
| `validate/` | Validation gates (G1–G14, T1–T2, R1, W01) — `plan.rs`, `gates.rs`, `milestone_warnings.rs`, `tracks.rs`, `report.rs` |
| `ac_verify.rs` | AC verification execution (runnable via `sh -c`; `MP_VERIFY_NO_SHELL=1` strict mode) |
| `annotation.rs` | Annotation CRUD (M43 — review-request, approval-request, notes) |
| `execution.rs` | `mp execution check/handoff/pause/status` |
| `config.rs` | Config templates, profile resolution |
| `doctor.rs` | `mp doctor` — toolkit health check (embedded assets + project) |
| `interview.rs` | `mp interview checklist` — structured question prompts |
| `json_input.rs` | `@-` / `@file` / inline JSON reader (project-root bounded) |
| `graph.rs` | `mp graph` — dependency/coverage graph |
| `inbox.rs` | `mp inbox` |
| `hygiene.rs` | `mp hygiene` |
| `groom.rs` | `mp groom milestone` |
| `brief.rs` | `mp brief *` |
| `charter.rs` | Charter (goals/non-goals) |
| `brownfield.rs` | Brownfield detection + delta-rebase |
| `challenge.rs` | `mp challenge *` — stress-test specs |
| `idea.rs` | `mp idea *` |
| `note.rs` | Meeting notes → ideas |
| `backlog.rs` | `mp backlog *` |
| `bootstrap.rs` | `mp init` scaffold |
| `install.rs` | `mp install / uninstall` |
| `harness.rs` | Harness detection (opencode, cursor, claude-code) |
| `digest.rs` | `mp digest` (stakeholder summary) |
| `delta.rs` | Delta-milestone merge |
| `sync.rs` | `mp sync` — index rebuild |
| `config_cmd.rs` | `mp config set/show/get` |
| `decisions.rs` | `mp decision *` |
| `session.rs` | `mp session *` |
| `specs.rs` | `mp specs *` |
| `skill.rs` | `mp skill context` — agent-context generation |
| `scenario.rs` | Scenario test runner |
| `path_prefs.rs` | Path preference data structures |
| `git.rs` | `mp git *` (status, commit, push) |
| `lib.rs` | Library entry point (re-exports for `mp-model`, test support) |

**Entry:** `main.rs` → `cli/` (parse) → `app.rs` (dispatch) → `commands/*` (handler) → domain modules → `store.rs` (persistence) + `schema.rs` (enforcement).

**Sibling crate:** `crates/raul` — human-facing PM CLI that shells out to `mp --format json` and consumes `mp-model` types. Its integration tests live in `crates/raul/tests/`. See M40/M42.

---

## 2. Layering

```
main.rs                       # argv → cli::Cli
  └─ lib.rs                  # library entry (re-exports)
       └─ cli/               # clap parse by command group (Commands, MilestoneCmd, …)
            └─ app.rs        # dispatch: cmd_milestone(), cmd_track(), …
                 └─ commands/*.rs  # handler: validate gates → domain → store/schema
                      └─ domain: milestone/{io,spec,complete}, step.rs, wp.rs, …
                           └─ store.rs       # JSON I/O (load / save / atomic_write)
                           └─ schema.rs      # enforce via mini_schema::Validator
                                └─ mini_schema.rs
                           └─ assets.rs      # embedded templates + schemas

Paths: paths.rs / path_engine.rs / plan_gaps.rs / path_prefs.rs
Gate enforcement: validate/ (G1–G14, R1, T1–T2), ac_verify.rs (M30 complete gate)
Harness / install: install.rs / harness.rs / doctor.rs
```

Rules: `app.rs` stays ≤400 lines; handlers delegate to domain modules; `store.rs` is the sole persistence seam.

---

## 3. Key types

| Type | File | Description |
|------|------|-------------|
| `PlanContext` | `paths.rs` | Resolved project root + plan dir, helpers for milestone/track/backlog paths |
| `MilestoneFile` | `mp-model` (`milestone.rs`) | A milestone spec + steps + work packages (self-contained JSON file) |
| `TrackFile` > `TrackMeta` + `Vec<TrackItem>` | `mp-model` (`track.rs`) | Track definition + items (bullets) |
| `PlanFile` | `mp-model` (`plan.rs`) | `plan.toml` — project config + milestone list + adoption order |
| `TrackKind` | `track_kind.rs` | Enum: `bugfix`, `tweak`, `chore` |
| `SchemaKind` | `schema.rs` | Enum: `Milestone`, `Brief`, `Track`, `Idea`, `Challenge` |
| `ValidationIssue` | `validate/report.rs` | Gate check result (code + message) |

---

## 4. Persistence model

`master-plan/` (or `.mp/` for hybrid profile):
```
plan.toml          # [[milestones]] index, [project], [workflow], pins
config.toml        # workflow profile
AGENTS.md          # session-start snippet (generated from template)
milestones/
  XX-slug.toml     # one file per milestone (self-contained)
tracks/
  <kind>.toml      # bugfix.toml, tweak.toml, chore.toml
backlog.toml       # deferred scope items
ideas.toml         # parked ideas
brief.toml         # project-brief topics (during planning)
decisions.toml     # architecture decision records
annotations.toml   # annotation items (review-request, approval-request)
sessions/          # per-session context (hybrid profile)
reviews/
  challenges/      # challenge session files (CH-XX-NN.toml)
specs/             # domain spec files ({domain}.toml)
archive/           # soft-deleted milestones, backlog items
```

**ID formats:** M## (milestone) · S## (step) · WP## (work package) · AC-## (acceptance criterion) · AN-## (annotation) · BF-## / TW-## / CH-## (track items) · B-## (backlog) · Q-XX (question) · F-## (challenge finding) · D-### (decision) · ID-01 (idea).

`store.rs`: generic `load_toml<T>` / `save_toml<T>` + `atomic_write`. Writes enforce schema via `schema.rs` (now backed by `mini_schema`). IDs generated via `next_available_id` (scans existing files in the directory). `mp sync` rebuilds the `[[milestones]]` index in `plan.toml`.

---

## 5. Test structure

| Layer | Location | Example |
|-------|----------|---------|
| Integration (mp) | `crates/mp/tests/*.rs` | Shells out to compiled `mp` via `TestEnv` (cwd=temp, `MP_HOME=repo_root`) |
| Integration (raul) | `crates/raul/tests/*.rs` | Shells out to compiled `raul` via temp plan dirs |
| Scenario | `tests/scenarios/*.toml` | Golden CLI snapshots (runner: `crates/mp/tests/scenarios_runner.rs`) |
| Fixture projects | `tests/fixtures/projects/` | Hand-crafted plan dirs (`adopt-check`) |
| Per-binary tests | `tests/mp/` `tests/raul/` | Shared scenario fixtures + per-binary golden scenarios (M42 split) |
| Unit (rare) | Inline in module | `model.rs`, `path_engine.rs`, `ac_verify.rs` classification |

**Test commands:** `cargo test -p mp` (unit + integration) · `make test-fixtures` (validate+adopt fixtures) · `make test-scenarios` (golden) · `make adopt-check` (full + hybrid path).

**Convention:** each plan step declares a `tests` value — single path/command linking the step to its verification. See root `AGENTS.md` Step-testing convention.

---

## 6. Entry-point recipes

### Add a new CLI command
1. Define variant in `cli/milestone.rs` `enum MilestoneCmd` (or top-level `Commands`).
2. Add handler arm in `app.rs` (or `commands/milestone.rs` for milestone subcommands).
3. Implement domain logic in `milestone/{io,spec,complete}.rs` (or `step.rs`, etc.).
4. Add integration test in `crates/mp/tests/<feature>.rs` via `TestEnv`.

### Add a new JSON plan resource
1. Define the type in `crates/mp-model` (appropriate module under `plan.rs`, `milestone.rs`, `track.rs`, or `config.rs`).
2. Re-export from `crates/mp/src/model.rs` if needed by handlers.
3. Add `load_<resource>()` / `save_<resource>()` in `store.rs`.
4. Register in `store::init()` if it's a bootstrap file.
5. Add CLI command to create/manage it (see #1).

### Add a validation gate
1. Add `check_gate_*` (or rule helper) in `validate/gates.rs` (or `validate/tracks.rs` for track/annotation rules).
2. Wire it into `validate/plan.rs` (`validate_plan`) and the command-time helper (`validate_milestone_ready`, `validate_milestone_start_execution`, etc.).
3. The door-in-progress / ready / complete flows automatically pick it up.

### Add a human-facing display for an entity
1. Add a view in `crates/raul/` that reads `mp --format json` output (or `mp-model` types) and renders styled tables.
2. Do **not** add table renderers to `mp` — human display belongs in `raul` (M41/M76).
3. Add golden fixture + parity test under `crates/raul/tests/` if changing display output.

### Add a scenario test
1. Create `tests/scenarios/<name>/scenario.json` describing CLI invocations + expected outcomes.
2. The runner (`scenarios_runner.rs`) auto-discovers it.
3. Run `make test-scenarios` to verify.

---

## 7. Conventions

- **Emit/ok shape:** every command returns `{ "ok": true, … }` or `{ "ok": false, "errors": […] }`. Gate failures exit 2.
- **Output formats:** `json` (default — omit `--format` on reads) · `toml` (debug: raw file passthrough, serialized lists, or GraphViz DOT on `graph`). Styled tables and TUI live in `raul`.
- **`mp validate` vs `plan_gaps`:** use **`mp validate`** for plan-wide integrity — schema-ish gates (G1–G14), index drift (W01/W03), annotation rules (R1), track shape (T1–T2). Use **`plan_gaps`** for **per-milestone execution readiness** — missing work packages/steps, AC coverage map, `execution_ready` hints for `mp execution check` / `mp path`. Overlap on empty `step.tests` is intentional: `validate` enforces it under `strictness=full` (G10); `plan_gaps` reports it as a coverage gap for grooming. Do not duplicate gate logic into `plan_gaps`.
- **`mp validate` gates:** G1–G5 (structure), G6–G7 (spec status), G8–G10 (start execution, strictness), G11–G13 (delta/brownfield), G14 (approval-annotation gate), R1 (annotation validation), T1–T2 (tracks), B1–B3 (brief), W01 (index drift).
- **Spec lifecycle:** `draft` → `interview` → `review` → `ready` → `implemented` → `verified`.
- **Two-zone rule:** plan zone (`master-plan/`) → all I/O via `mp`; code zone (`src/`, `tests/`, docs) → open editing.
- **Dep policy:** pin features explicitly where impactful (M21 `dep-audit` gate ≤150). Documented exceptions: `mp-model` (path crate, serde-only), `walkdir` / `include_dir` (no optional feature flags). Pinned: `regex` (`std`, `unicode-perl`), `serde_json` (`std`), `tempfile` (`getrandom`). `jsonschema` is dev-only oracle; runtime uses `mini_schema`.
- **raul dep policy (M73):** workspace-pinned `crossterm = 0.29`; `ratatui 0.30` with `crossterm_0_29` (required to dedupe crossterm 0.28 pulled by ratatui 0.29). CLI tables via `raul::table`; styling via `crossterm::style::Stylize`. Gate: `make dep-audit-raul` (≤100 transitive, no comfy-table/owo-colors). Baseline was 118 transitive with duplicate terminal stacks.
- **Embedded assets (M29):** templates + schemas compiled into the binary via `include_dir`. `MP_HOME` is an optional disk override.

---

## 8. Cross-references

| Doc | Contents |
|-----|----------|
| `AGENTS.md` (root) | Session start, dev commands, two-zone rules |
| `master-plan/AGENTS.md` | Full plan workflows (§1–7) |
| `docs/README.md` | Documentation index |
| `docs/agent-guide/README.md` | Agent orientation + per-workflow detail (planning, decomposing, executing, reviewing) |
| `docs/mp/commands.md` | CLI reference |
| `docs/mp/config.md` | Project config & profiles (`full` / `hybrid` / `session`) |
| `docs/mp/getting-started.md` | Install & first-project onboarding |
| `docs/milestone-lifecycle/` | Lifecycle state machine + gates (planning / execution / review) |
| `docs/milestone-details/` | Milestone anatomy — every field & what it's for |
| `docs/raul/` | `raul` TUI — lanes, keybinds, settings |
| `docs/skills/` | Shipped skills (`mp-flow` / `mp-runner` / `mp-coordinator` + catalog) |

> The previous `docs/concepts/` tree is archived under `docs-old/` (unmaintained).
> `docs-old/concepts/06 - Reference/` still holds the `mp docgen` generated
> command reference, pinned by `crates/mp/tests/docgen.rs`.

---

## 9. Maintenance

- Update the module map (section 1) when adding/removing `src/` modules.
- Update the last-updated marker at the top when making structural changes.
