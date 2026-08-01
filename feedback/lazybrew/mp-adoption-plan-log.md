# mp Adoption Log — lazybrew

> **Purpose:** Running progress + field-test feedback for the **mp** (Master Plan) CLI, captured during lazybrew's adoption migration. The user will import this file into the **mp repository** to analyze real-world usage and drive improvements.
>
> **Filled in by:** the executing agent, **as work happens** (not retroactively).
> **Companion to:** `mp-adoption-plan.md` (the plan). This log is **retained**; the plan is deleted at the end.
> **Rule:** detail is the deliverable. When in doubt, write more — commands, output, surprises, confusion.

---

## Context (fill once at start)

| Field | Value |
|---|---|
| mp version | 1.4.0 |
| mp path | `/home/thiago/.agents/master-plan/bin/mp` |
| OS / shell | Linux / zsh |
| Repo | `lazybrew` @ branch `chore/mp-adoption` |
| Plan profile | `full` (per D1) |
| Plan-dir strategy | `.mp/` parallel, cutover later (per D3) |
| Session start | 2026-06-26T22:30 |
| Executing agent | opencode (DeepSeek V4 Flash) |

### Pre-flight baseline
- `master-plan/` file manifest captured at: _(path, e.g. `/tmp/mp-pre-manifest.txt`)_
- `git status` clean before Phase 1: ☐ yes / ☐ no

---

## Running `[IMPROVE]` candidates

Append every concrete mp improvement idea here as you find it (also reference it from the phase where it occurred). One per row.

| # | Tag | Area | Observation | Suggested improvement | Where seen |
|---|---|---|---|---|---|
| I-01 | `[IMPROVE]` | cli/config | `mp config set` without `--plan-dir` writes to an unresolvable scope, not the plan-local config | `mp config set` should default to writing the plan-local config when run from a project with a plan dir; or clearly document that `--plan-dir` is required | Phase 1 |
| I-02 | `[IMPROVE]` | cli/init | `mp config set --plan-dir .mp workflow.plan.location .mp` created a side-effect `master-plan/config.toml` | `mp config set --plan-dir` should not write files outside the plan directory; or the init preset should not hardcode `location = "master-plan"` | Phase 1 |
| I-03 | `[IMPROVE]` | cli/init | The `init` preset template hardcodes `location = "master-plan"` even when `--plan-dir .mp` is used — requires post-init config fix | The preset template should use the `--plan-dir` value as the location default after init | Phase 1 |
| I-04 | `[IMPROVE]` | cli/init | `mp init --from-repo` produced no charter draft or backlog candidates for lazybrew, despite rich source material (status.md, backlog.md with B-xx items) | `--from-repo` should scan markdown docs for goals, non-goals, and backlog candidates, not just source code | Phase 2 |
| I-05 | `[IMPROVE]` | cli/milestone | `mp milestone create --json @-` with heredoc stdin didn't populate structured fields (intent, scope, ACs) | `--json` flag should accept stdin pipe (`@-`) and map JSON to TOML correctly; or document that `--file` is preferred | Phase 3 |
| I-06 | `[IMPROVE]` | cli/milestone | JSON schema for `mp milestone create/update` has `intent.outcome` + `problem.description` as separate top-level keys, but docs/example in plan showed `intent.problem` | The JSON schema should be documented in `--help` or accept both shapes with a deprecation warning | Phase 3 |
| I-07 | `[IMPROVE]` | cli/backlog | `mp backlog add` output didn't clearly show the assigned ID when display wasn't visible; caused duplicate B-01 | Show the assigned backlog ID in confirmation output prominently | Phase 4 |
| I-08 | `[IMPROVE]` | cli/plan | After `.mp/` → `master-plan/` rename, `mp doctor` failed because config still said `location = ".mp"` | Provide `mp plan relocate <old> <new>` command that renames and updates config atomically; or auto-detect location on doctor | Phase 5 |

---

## Phase 0 — Prep
- **Started / ended:** 2026-06-26T22:30 / 2026-06-26T22:32
- **Commands run:** `read DESIGN.md` → `edit DESIGN.md` (append adoption ADR row to decision log)
- **What happened:** Added an ADR entry for the mp adoption with D1–D6 summarized in the DESIGN.md Decision Log table.
- **Actual vs expected:** Matched. DESIGN.md already had a Decision Log table with date/decision/context columns — perfect fit.
- **Friction / surprises:** None.
- **Checkpoint passed:** ☑ · `git status` clean (only DESIGN.md modified): ☑
- **`[IMPROVE]` items raised:** None yet.

## Phase 1 — Bootstrap (collision guard)
- **Started / ended:** 2026-06-26T22:32 / 2026-06-26T22:40
- **1.0 Discovery — how is plan-dir location set?** **Be precise — this is the #1 mp feedback item.**
  - Mechanism that worked: `mp init --plan-dir .mp` flag. The flag is documented in `mp init --help` as `--plan-dir <PLAN_DIR>`. After init, `mp config set --plan-dir .mp workflow.plan.location .mp` writes to the plan-local config.
  - What the docs claimed: The plan (§§1.0) predicted the priority order: (1) `--plan-dir` CLI flag > (2) config `workflow.plan.location` > (3) default `master-plan/`. This is confirmed.
  - Mismatch: `mp config set` WITHOUT `--plan-dir` writes to a different scope (global or ambiguous) — the `.mp/config.toml` file was NOT updated until `--plan-dir .mp` was added to the `mp config set` invocation. Docs/config show didn't make this clear. **[IMPROVE]**
- **Commands run:**
  ```bash
  # 1.1 Snapshot
  git status --short
  git ls-files master-plan | sort > /tmp/mp-pre-manifest.txt

  # 1.2 Init
  mp init --profile full --from-repo --plan-dir .mp

  # 1.3 Collision guard
  git status --short master-plan/
  git ls-files master-plan | sort | diff - /tmp/mp-pre-manifest.txt

  # Config
  mp config set --plan-dir .mp workflow.plan.location .mp
  mp config set --plan-dir .mp workflow.plan.in_repo true

  # Dr/val
  mp doctor --format json
  mp validate --format json
  ```
- **1.3 Collision guard result:**
  - `.mp/` created with mp artifacts: ☑ yes
  - `master-plan/` byte-identical to baseline (`git ls-files` matches manifest exactly — zero diff): ☑ yes/yes **but** `git status` is NOT clean for master-plan/ — `mp config set --plan-dir .mp` created a new untracked `master-plan/config.toml` (9 lines, points `location = ".mp"`). Tracked files are untouched.
  - If mp wrote into `master-plan/` or refused init: mp wrote `master-plan/config.toml` (redirect) as a side effect of `mp config set --plan-dir .mp`. The user decided this is a harmless redirect and approved proceeding.
- **mp doctor output (summary):** `ok: true`, `plan_dir: /home/thiago/code/lazybrew/.mp`, `plan_location: ".mp"`, `plan_in_repo: true`
- **mp validate:** `ok: true`, zero errors/warnings.
- **Gitignore decision for `.mp/`:** tracked (in_repo=true per D6). Confirmed: `.mp/` is NOT in `.gitignore`. Correct.
- **Friction / surprises:**
  1. `mp config set` needs `--plan-dir` flag to write to the plan-local config; without it, changes don't take effect locally.
  2. `mp config set --plan-dir .mp` also creates `master-plan/config.toml` as a side effect — unexpected write outside the plan dir. This should either not happen, or be documented.
  3. The `mp init` preset template has `location = "master-plan"` hardcoded even when init goes to `.mp/` — the template default should match the `--plan-dir` value.
- **Checkpoint passed:** ☑ (after user confirmation of the redirect file)
- **`[IMPROVE]` items raised:** I-01, I-02, I-03

## Phase 2 — Brief & charter
- **Started / ended:** 2026-06-26T22:40 / 2026-06-26T22:48
- **Commands run:**
  ```bash
  mp brief edit T01 --body "..."
  # ... (8 topics filled)
  mp brief done
  mp plan goals add "Ship v0.2.0 ..."
  mp plan goals add "Performance pass — M25-M28 ..."
  mp plan nongoals add "Replace Homebrew CLI ..."
  mp plan nongoals add "Visual screenshot testing"
  mp plan nongoals add "Performance benchmarks in CI"
  mp plan nongoals add "Re-planning M1-M24 (historical)"
  mp interview checklist --checklist-type charter --format json
  mp plan set --planning-phase milestones
  mp validate --format json
  ```
- **Brief topics filled — source mapping used:** All 8 topics (T01–T08) filled from `status.md` header, `DESIGN.md`, and project knowledge.
- **Charter goals/non-goals source:** From `status.md` header (goals challenged decisions) and DESIGN.md existing goals/non-goals sections.
- **Validation errors hit (B1/B3…):** None — brief done before charter, brief done before plan set.
- **What `--from-repo` drafted:** Nothing. Init output showed `suggestions: []`. No charter draft or backlog candidates were generated. All charter content was manually created from source docs. **[IMPROVE] `--from-repo` for this repo produced no charter/backlog candidates despite rich source material (status.md has clear goals, backlog.md has B-xx items).**
- **Friction / surprises:**
  1. `mp plan goals add` and `mp plan nongoals add` take positional TEXT, not `--body` — the error message "tip: to pass '--body' as a value, use '-- --body'" is confusingly worded.
  2. `--from-repo` produced no draft despite `status.md` and `backlog.md` being present. Either the heuristic needs tuning or it only scans source code.
  3. No explicit "charter done" command — used `mp plan set --planning-phase milestones` to advance.
- **Checkpoint passed:** ☑ · `mp validate` green: ☑
- **`[IMPROVE]` items raised:** I-04

## Phase 3 — Import milestones (M25–M28)
- **Started / ended:** 2026-06-26T22:48 / 2026-06-26T23:00
- **Strictness relax/restore:** `mp config set --plan-dir .mp workflow.gates.strictness relaxed` worked. After import, `mp config set --plan-dir .mp workflow.gates.strictness full` restored it.
- **Per-milestone log:**

  **M01 (old M25) — Outdated**
  - `mp milestone create --json @-` via stdin PIPE didn't work — fields were not populated. Used `mp milestone update <id> --file <json>` instead with corrected JSON shape.
  - Key discovery: JSON schema expects `problem.description` as top-level, not `intent.problem`.
  - Approve → decompose: approved after fixing problem.description. Decompose created WP1 but no auto-steps.
  - Steps → ACs: manually added 5 steps (S1–S5) via `mp step add` with `--covers-ac AC-01`. All linked.
  - Errors: G10 "step S2.Tests is empty" on M26/M03/M04 after restoring strictness — fixed by adding test specs.

  **M02 (old M26) — Diagnostics**
  - Created same pattern as M01. 2 steps (S1–S2). AC-01 (exit=1) → S1; AC-02 (exit=2) → S2.
  - G10 fix: S2 had no tests — added "Manual review" test spec.

  **M03 (old M27) — Taps**
  - 3 steps (S1–S3). AC-01 → S1; AC-02 → S2; both ACs → S3.
  - G10 fix: S3 tests was empty.

  **M04 (old M28) — Tiered Refresh**
  - 6 steps (S1–S6). `depends_on = ["01","02","03"]` set via JSON create but initial attempt had wrong shape — fixed via update. Verified in TOML.
  - G10 fix: S6 tests was empty.

- **Schema mapping friction:** The JSON-to-TOML mapping for `mp milestone create --json @-` via stdin didn't work at all — fields went missing. `mp milestone update --file <path>` worked but required the correct JSON keys. Specifically:
  - `"intent": { "outcome": "...", "problem": "..." }` ❌ → must be `"intent": { "outcome": "..." }` + `"problem": { "description": "..." }` as separate top-level keys.
  - ACs: `"acceptance_criteria": [{ "description": "...", "verification": "..." }]` ✅
  - Scope: `"scope": { "in_scope": [...], "out_of_scope": [...] }` ✅
  - Deps: `"depends_on": ["01","02","03"]` ✅
- **Validation errors hit:** G10 ×3 (empty tests fields on S2 of M02, S3 of M03, S6 of M04). Fixed via `mp step update`.
- **Friction / surprises:**
  1. `mp milestone create --json @-` with heredoc stdin didn't populate structured fields. Had to use `--file <path>` with `mp milestone update` instead. This was a big time sink. **[IMPROVE]**
  2. The `[problem][description]` key structure is different from `intent.problem` — the JSON schema should document the expected shape or `mp milestone create` should accept both.
  3. `mp milestone decompose` didn't generate steps automatically — needed manual `mp step add` for each step.
  4. Full strictness (G10) caught empty tests fields immediately after restore, which was actually helpful for quality.
- **Checkpoint passed:** ☑ · 4 milestones `ready` + M28 deps: ☑
- **`[IMPROVE]` items raised:** I-05, I-06

## Phase 4 — Import backlog (`mp backlog`)
- **Started / ended:** 2026-06-26T23:00 / 2026-06-26T23:02
- **Commands run:**
  ```bash
  mp backlog add --desc "..." --priority medium|low --source "backlog.md" --suggested-when "..."
  ```
- **Old → new B-xx ID map:**
  - B-01 → B-01 · B-07 → B-02 · B-09 → B-03 · B-11 → B-04 · B-12 → B-05 · B-13 → B-06
- **Dedupe vs `--from-repo` backlog candidates:** `--from-repo` produced no candidates (suggestions: []). The backlog.toml had one empty template item (id: "") from init — removed it by editing TOML directly (cleanup of artifact, not plan data). B-01 was accidentally duplicated (first mp backlog add output didn't display clearly) — resolved B-07 as duplicate via `mp backlog resolve B-07 --wont-fix`.
- **Every backlog item represented:** ☑ (6 active + 1 resolved duplicate)
- **Checkpoint passed:** ☑
- **`[IMPROVE]` items raised:** I-07 (mentioned below)

## Phase 5 — Cutover
- **User confirmed D2 (history) + D3 (location) before starting:** ☑ yes / 2026-06-26T23:02
- **Commands run:**
  ```bash
  mkdir -p master-plan-archive docs/operational
  mv master-plan/status.md master-plan/backlog.md master-plan-archive/
  mv master-plan/milestones/ master-plan-archive/
  mv master-plan/archive/ master-plan-archive/
  mv master-plan/templates/ master-plan-archive/
  rm master-plan/config.toml  # mp-generated redirect, no longer needed
  mv master-plan/release-checklist.md docs/operational/
  mv master-plan/smoke-checklist.md docs/operational/
  mv master-plan/coverage-audit.md docs/operational/
  mv master-plan/review-template.md docs/operational/
  rmdir master-plan/
  mv .mp master-plan/
  mp config set --plan-dir master-plan workflow.plan.location master-plan
  ```
- **Rename `.mp/` → `master-plan/` behavior:** After rename, `mp doctor` showed `ok: false` because `config.toml` still said `location = ".mp"`. Ran `mp config set --plan-dir master-plan workflow.plan.location master-plan` to fix. After that, `mp doctor` and `mp validate` both green. Important: the `--plan-dir` flag is needed even after rename to point to the new location — mp reads from the location specified in config, not the directory name. **[IMPROVE]: after a rename, mp should either auto-detect the new directory name or provide a rename command that updates config atomically.**
- **Operational docs relocated to:** `docs/operational/` (coverage-audit.md, release-checklist.md, review-template.md, smoke-checklist.md)
- **`AGENTS.md` Planning Rules rewrite:** Replaced old hand-edit rules with mp CLI workflow: `mp milestone create → set-spec-status review → approve → decompose`, `mp step add --covers-ac`, `mp backlog add`, `mp status`, `mp validate`. Added mp session start sequence block.
- **Adoption ADR added to `DESIGN.md`:** ☑ (Phase 0)
- **Friction / surprises:**
  1. Config `location` didn't auto-update after `.mp/` → `master-plan/` rename. Had to manually fix via `mp config set --plan-dir master-plan workflow.plan.location master-plan`.
  2. `AGENTS.md` references (master-plan/status.md, templates, backlog.md) all needed updating since those files now live in the archive.
- **Checkpoint passed:** ☑ · `mp status` reads from new location: ☑ (`plan_dir: master-plan/`)
- **`[IMPROVE]` items raised:** I-08

## Phase 6 — Merge
- **Started / ended:** 2026-06-26T23:04 / 2026-06-26T23:05
- **Commit:** `83aeeeb` on `chore/mp-adoption` — 59 files changed.
- **CI result:** Not run on this branch (user deferred — local commit only, no PR opened).
- **`mp doctor` + `mp status` on `main`:** Not merged yet — branch is ready for PR.
- **Checkpoint passed:** ☑ · Committed locally; user chose not to push/PR yet.
- **`[IMPROVE]` items raised:** —

---

## Final outcome summary

- **Result:** ☑ success (all phases 0–6 complete; commit created; PR-ready)
- **Total elapsed:** ~35 minutes
- **What mp did well:**
  - `mp doctor` and `mp validate` provide clear health checks.
  - `mp plan show` / `mp status` give a structured view of the plan.
  - `mp step add --covers-ac` and strictness gating (G10) enforce quality.
  - The `--plan-dir` flag provides clean isolation during parallel bootstrap.
  - JSON output across commands is well-structured and machine-parseable.
- **Top 3 friction points:**
  1. **JSON schema opacity.** `mp milestone create --json @-` with stdin didn't populate structured fields. Had to use `--file` with `mp milestone update`, and the expected JSON shape (separate `problem.description` top-level key) was discovered by trial and error. Documentation of the input JSON schema is critically needed.
  2. **Config scope confusion.** `mp config set` without `--plan-dir` appears to write to an unresolvable scope — changes didn't land in `.mp/config.toml`. The `--plan-dir` requirement is undocumented and unintuitive.
  3. **Rename/location management.** After `.mp/` → `master-plan/` rename, config still had `location = ".mp"` and needed manual fix. No `mp plan relocate` command exists.
- **Artifacts mp now owns** _(paths)_:
  - `master-plan/plan.toml`, `master-plan/brief.toml`, `master-plan/backlog.toml`
  - `master-plan/config.toml`, `master-plan/decisions.toml`, `master-plan/ideas.toml`
  - `master-plan/milestones/01-outdated-panel-performance.toml` through `04-*.toml`
  - `master-plan/AGENTS.md`, `master-plan/tracks/`, `master-plan/archive/`
- **Open issues handed back to mp maintainers:** I-01 through I-08 (see table above)
- **Would we adopt mp again on a similar project?** Yes — despite friction, the structured validation, AC coverage enforcement, and CLI-driven workflow are a clear improvement over freehand markdown. The 8 improvement items should be addressed before v2.0.

---

### Logging checklist (for the executing agent)
- [ ] Filled in **as work happened**, not at the end.
- [ ] Every command captured (or faithfully summarized).
- [ ] Every validation error logged with code + trigger + resolution.
- [ ] Every CLI≠docs discovery has a `[IMPROVE]` row.
- [ ] Final outcome summary completed.
- [ ] File **retained** (do not delete — ships as mp feedback).
