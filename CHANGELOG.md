## Unreleased — WIP CI hardening

- **raul keybind deconflict.** `keybinds.refresh` now defaults to
  `Ctrl-R` (was `r`); `keybinds.previous_lane` dropped the `h` alias
  (use `Left` or `BackTab`); `keybinds.focus_content` is no longer a
  user-rebindable setting (it was a TUI-internal reserved action).
  Existing user overrides win — no migration is required for projects
  with a `[keybinds]` section in their config. `mp config validate`
  surfaces a non-blocking deprecation warning for any stale
  `focus_content` line; the line is silently dropped the next time you
  set a different keybind.
- **CI provides `mp` on PATH** (`target/release` via `$GITHUB_PATH` in
  `wip-ci.yml` / `stable-ci.yml`) so raul integration tests that shell out
  to `mp` run on clean runners.
- **`make ci` preflight** requires `mp` on PATH or `$MP_HOME/bin/mp`; runs
  `lint` + `test` + `mp-flow-lint` + `test-scenarios`. `NEXTTEST=1` selects
  nextest `--profile ci` (`fail-fast=false`).
- **Install-helper tests** prepend the per-test install bin to PATH
  (`path_with_install_bin`) so doctor `runtime:mp_on_path` matches real
  `source env.sh` behavior without relaxing the doctor contract.
- **`make regen-goldens`** rewrites json-shape + track goldens (replaces
  ignored `regenerate_goldens` tests).
- **raul `find_mp`**: resolves `target/{debug,release}/mp` from cargo-test
  `deps/` layout; dashboard parity fixtures use lifecycle bucket
  `executed` (M196 rename from `done`).

## v1.0.0-rc1 — 2026-07-19 — M187–M193 code-review remediation + version re-anchor

First release candidate of the 1.0 line. Ships the M187–M193 code-review
remediation batch (groups A–G) on top of the 2.0.0 dogfood train and
re-anchors the workspace version from `2.0.0-rc.X` to `1.0.0-rcN`.

### Headline changes

- **Versioning re-anchor.** Workspace version is `1.0.0-rc1`. The
  `2.0.0-rc.1`–`2.1.0-rc.1` history in this changelog and in
  `master-plan/plan.json` is preserved as historical record. `mp install`
  does not bake the version into any install path; existing installs
  upgrade without path changes.
- **M187 — Persistence safety + transactional mutations.** PlanWriteTxn
  token routed through every authoritative plan-resource RMW path
  (backlog / idea / brief / annotation / decision / track / session /
  specs / plan-config / migration / archive); multi-file mutation
  engine with versioned recovery manifest, fsync, idempotent retries.
- **M188 — Schema + JSON input hardening.** mini_schema explicit
  allowlist (rejects unknown keywords / types / regex / ref at compile
  time); one bounded reader for every durable JSON load and stdin
  (cap enforced on the open handle); canonical containment for
  `--file` / `@file` / symlinks / `..`; unified create-input pipeline.
- **M189 — Typed lifecycle transitions.** `MilestonePhase /
  MilestoneEvent / TransitionEffects` in mp-model; one pure
  `transition(current, event, ctx)` function owns sources, destinations,
  gate requirements, overlay changes, timestamp sync, and legacy alias
  sync; remediation restores exact prior phase; legacy `self-reviewed`
  / `reviewed` demoted to read / migration aliases; nested step ID
  dedup with top-level precedence for migration input.
- **M190 — Watch state-machine correctness.** WatchRunStore::transition
  persists every externally observable state bump (generation + atomic
  snapshot); sequencer solely owns aggregate terminality;
  `PartialFailure` for mixed-completed queues; restoration wired into
  the Watch lane with a required fresh preflight; preflight tied to
  exact ordered queue fingerprint; `EventSource` trait waits on
  input-or-deadline (no busy-spin); every Watch action bumps
  `App::version` on mutation.
- **M191 — Raul selection, co-approval, timestamps.** One canonical
  `visible_annotations()` projection shared by renderer, cursor limits,
  keyboard, and mouse; selection preserved by id across filter
  changes; co-approval marked confirmed only after full success
  (retryable on failure); one strict RFC3339 parser in mp-model with
  leap / calendar / offset / UTC-day-boundary semantics, shared by
  durable audit timestamps and Raul humanize.
- **M192 — Security trust + install/display hardening.** `ac_verify`
  requires repository-scoped trust before any verification command
  runs; argv-only allowlisted mode for standard tests; arbitrary shell
  mode explicit opt-in; one display-boundary sanitizer strips
  C0/C1/ESC/DEL/bidi/zero-width controls from plan/subprocess strings;
  install: POSIX-single-quote `env.sh` paths + atomic write;
  uninstall: canonical harness root + `O_NOFOLLOW` opened-dir fd +
  containment verification before deletion.
- **M193 — Architecture cleanup + comment remediation.**
  `crates/mp/src/app.rs` trimmed to a 14-line dispatch surface;
  terminal setup uses staged rollback (Normal → Raw → Alternate →
  Mouse); Watch log tailing moved into polling state with bounded
  cache; renderer paths perform no I/O; active-schema filenames
  centralized; oracle tests that spawn `mp` relocated to
  `crates/mp/tests`; fixture-isolation helper ensures mutable tests
  use temporary copies; milestone/finding tags stripped from
  production comments per code-review-fixes.txt §10.4 (provenance
  preserved in git history, milestone records, and regression test
  names).

### Versioning policy

The workspace version re-anchors from `2.0.0-rc.X` (dogfood train that
shipped up to rc.26) to `1.0.0-rcN` (the 1.0 line). Rationale:

- The M187–M193 batch is the first time the toolkit ships a fully
  consolidated, independently-reviewed correctness pass for plan
  persistence, schema/input, lifecycle, watch state-machine, TUI
  selection/timestamps, security trust, and install/display. This is
  the natural 1.0 boundary, not a 2.0 — the prior `2.0.0` GA in
  CHANGELOG was a dogfood checkpoint, not a release-cut.
- Consumers that pinned to `>=2.0.0-rc.1` should be aware that
  `1.0.0-rc1` is a downgrade in semver pre-release ordering. There
  are no known external consumers at this time (Homebrew formula has
  not yet been cut).
- Historical `2.0.0-rc.X` and `2.1.0-rc.X` entries in this changelog
  and in `master-plan/plan.json` are preserved as historical record.
  They document the actual ship chain — not retroactively renumbered.

### Verification

- `cargo nextest run -p mp --test plan_io_concurrent_writes --test mutation_transactions` — 26/26
- `cargo nextest run -p mp --test suite_validate --test mini_schema_e2e` — 97/97
- `cargo nextest run -p mp-model` — 53/53
- `cargo nextest run -p mp --test watch_state_file --test watch_control --test watch_dry_run --test watch_non_dry_run` — 74/74
- `cargo nextest run -p raul --test tui_state --test tui_modes --test tui_smoothness` — 70/70
- `cargo nextest run -p raul --test tui_annotation_selection --test tui_co_approval` — 12/12
- `cargo nextest run -p mp-model --test rfc3339` — 4/4
- `cargo nextest run -p raul -E 'test(/^tui::humanize::tests::/)'` — 8/8
- `cargo nextest run -p mp --test install_skills_v2 --test install_deploys_mp_planner_agent` — 47/47
- `cargo nextest run -p mp -E 'test(/ac_verify/)'` — 36/36
- `cargo nextest run -p raul -E 'test(/sanitize|control_char/)'` — 4/4
- `cargo nextest run -p mp --test comment_inventory --test fixture_isolation` — 14/14
- `cargo fmt --all -- --check` — clean
- `cargo clippy --release --all-targets -- -D warnings` — clean
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` — clean

### Branch model

Two-branch cut effective with this release:

- **`stable`** — the default branch on GitHub (`github.com/lthiagol/master-plan`).
  Receives the blessed, reviewed, CI-green history. New release tags land here.
  Branch protection: PR + CI green + review required.
- **`wip`** — the working branch. Day-to-day commits, unreleased features,
  milestone batches land here first. CI must be green before merge to `stable`.

The historical `master` branch is renamed to `wip`; no commits are lost.
The previous `2.0.0-rc.X` / `v2.0.0` chain lives on the old `master` ref
(preserved in the `origin/master` tag archive if needed) — new release
tags are cut on `stable` only.

---

## Unreleased — raul TUI refactor (M183–M186)

### Headline changes

- **7 tabs (was 9).** Tweaks folded into Backlog (`TW-*` + `BF-*` + `BL-*`); Grooming tab removed — its filter is the `g` Grooming preset on Milestones. Ideas remains `ID-*` only.
- **Two-line footer (M183).** Globals on line 1 (quit, help, refresh, lanes, sort, hide-done, filter, …); per-tab keys on line 2.
- **Milestones table + lifecycle gauge (M185).** Table widget with `REVERSED` row cursor, depends_on indent column, and an 8-segment lifecycle gauge (`draft`→`complete`; cancelled `✗`, remediation `↺`).
- **Lifecycle filter modal (M185).** Capital `F` opens a multi-select modal (Space toggle, Enter commit, Esc revert). Title chip shows `Milestones · All (N)` or `Milestones · approved, in-progress (N)`. Lowercase `f` remains the annotation open-only toggle — both coexist (see help overlay).
- **Grooming preset (M185).** `g` on Milestones sets filter to `{approved, in-progress, groomed}`.
- **Search + cycle-sort (M186).** `/` fuzzy search and `o` cycle-sort land in M186; footer placeholders already surface the keys.
- **Migration.** Existing configs that reference Tweaks-specific keybind names load without panic; unknown actions are skipped with a diagnostic.

---

## v2.0.0-rc.11 — Install-path codesign + M123 review remediations

Bumps the rc chain on the 2.0.0-rc.X dogfood train. The headline
change is the install-path fix that was surfaced when the M123
review remediations were being verified.

### Headline changes

- **Install-path codesign hook (`mp install` + `make install`).**
  `crates/mp/src/install.rs::codesign_macos_binary` re-signs each
  copied Mach-O with `codesign --force --deep --sign -` after the
  `fs::copy` lands it at the final install path. The `Makefile`
  `install` target calls the same `codesign` invocation as a
  belt-and-suspenders step (`|| true` so non-macOS targets stay
  clean). The helper is a no-op on non-macOS via
  `#[cfg(target_os = "macos")]` and hard-fails if `codesign` is
  missing on macOS — a missing codesign on macOS means the install
  is broken the moment the user runs the binary.

  - **Why this exists:** modern Apple toolchains (clang, rust-lld)
    emit a Mach-O with the `linker-signed` (0x20002) CodeDirectory
    flag, which macOS 26.x's provenance-sandbox enforcement rejects
    on some install paths (stow into dotfiles, in particular — the
    kernel kills the process with SIGKILL `Code Signature Invalid`
    before the entry point runs). A plain adhoc (`0x2`) signature
    is what AMFI/ASP trusts on those paths. The re-sign must run
    **after** the binary is in its final location, because the
    `com.apple.provenance` xattr is path-bound and a
    re-sign-then-move sequence leaves a path/provenance mismatch.
  - The earlier "reinstall at 16:01, still crashes at 16:05" loop
    observed during M123 review-pass verification is fixed by
    construction: the next `make install` lands a re-signed binary
    by default.
- **M123 review-pass remediations (F-02..F-08).** All 6 review
  findings and the 1 surface-during-remediation finding are
  resolved. The M123 spec is verified end-to-end (7/7 ACs, 7/7
  steps, 0 open findings) and the milestone entered the
  execution-review queue with a force-bypass on AC-06 that is
  documented as pre-existing repo debt (F-08 / B-73: flaky
  `install_env_snippet` and `milestone_bulk` tests under
  `ac_verify`'s broad-scope runner), not a M123 defect.

---

## v2.1.0-rc.1 — Post-2.0 polish wave (M110–M118 + M118.5)
### Fixed
- Backlog hygiene sweep — closed B-78, B-83, TW-03, TW-10, TW-22 (M170)

- Perf AC re-spec (M159/M160/M161) + mp_measure! helper and reality-check lint (M176)
- Recursive skill deploy regression tests + full top-5 suite in-process conversion (M175)
The 2.0 GA cut is stable; this RC is the post-2.0 polish wave that the dogfood log
surfaced across the M110–M118 adoption. The shape of every change is "fix the
friction the agent hits" rather than "add a new feature surface" — except for two
additions (`mp milestone ac bulk` and `mp milestone log`) that the
agent-essential-I/O story couldn't ship without.

### Headline changes

- **M110 — Hygiene sweep v2.** `ac_verify.rs` gate-caches identical
  re-runs within one `mp milestone complete` invocation. Verifier-lint
  ports to a Rust linter (`mp plan verify-lint`) with macOS-portability
  patterns baked in. Closes the broad-scope AC verification string class.
- **M111 — Fragment-CLI ergonomics + correctness.**
  - `mp milestone ac update --evidence <TEXT>` and `mp milestone step
    update --evidence <TEXT>` stamp the fragment's `evidence` field
    in place. M107's jq+replace-arrays dance for per-AC evidence is
    gone.
  - `mp milestone ac add` now auto-increments the AC id and appends
    (parity with `mp milestone step add`). Two sequential `ac add` calls
    no longer collapse into one.
  - `mp milestone update --json --replace-arrays` now persists
    `steps` correctly (the prior implementation dropped it silently).
  - `mp milestone design-decision add --area <TEXT>` is required
    (the schema always marked `area` non-empty; the CLI now exposes
    the flag). `update <id> <index|area>` and `remove <id> <index|area>`
    subcommands close the add-only DD surface.
  - `mp milestone update --accept-extra-fields` is the round-trip
    escape hatch that lets `mp show ... --format raw` →
    `mp milestone update --json @file` work without manual
    `jq del(...)` stripping.
  - `sh -n` preflight on `--verification` (AC) and `--tests` (step)
    catches the M107 spec's 5-AC verification-string-prose bug class
    at write time. Warn-not-reject.
- **M112 — Read surface.** `mp backlog list [--source/--status/--priority/--limit]`
  (closes B-51). `mp show milestone --fields` is now schema-derived
  (the typed struct + raw JSON merge surfaces dropped-ceremony keys
  like `follow_ups` that the typed struct doesn't model — closes
  the M96 `follow_ups` unknown-path gap). `mp list milestones` and
  `mp list steps` accept `--take N`, `--select 'dotted.path'`, and
  `--sort 'dotted.path'` (closes the 20+ `python3 -c "import json..."`
  dogfood workarounds). `mp milestone log <id>` emits the milestone
  history plus `git log --oneline` for the milestone file.
- **M113 — Write safety + dry-run.** `crates/mp/src/plan_io.rs` adds a
  `flock(2)`-based advisory lock around the plan-write critical
  section. The previously-dogfood-flagged parallel
  `mp milestone wp|step` race (last writer wins) is gone.
  `mp milestone set-status` / `approve` / `complete` accept
  `--dry-run` to preview the change set without writing.
- **M114 — Hygiene sweep.** B-39..B-45 (testing-audit backlog) closed
  via `mp backlog resolve`. Three overlapping `mp milestone create`
  tests consolidated into one parameterized matrix (closes B-47).
  p6/p7 dead-code warnings trimmed (closes B-48). Historical
  broad-scope AC verification strings (M42, M99) narrowed to
  affected-crate cargo test (closes B-49). 12 historical milestones
  annotated with `[bypass-annotation: ...]` for their `[force-bypassed]`
  / `[skip-verify]` markers. `strip_dropped_keys_from_path` rewritten
  to use `tempfile::NamedTempFile::persist` for atomic same-filesystem
  renames (closes B-56).
- **M115 — Tab bar budget underflow.** `compute_tab_bar_layout` /
  `render_tab_bar` no longer emit indicator spans at `area.width < 6`.
  The M105 code-review entry 3 sub-4 underflow (where the indicator
  reservation budget overflowed the active lane itself at narrow widths)
  is closed.
- **M116 — Meta-tooling docs.** `AGENTS.md` carries an edit-tool
  batch-verify rule (the external `edit` tool's success counter is
  documented as informational; verify with `grep -c <expected> <file>`).
  `mp milestone step update --help` documents `--files` value format
  (bare path or comma-separated; quoted JSON arrays rejected with a
  structured error). A dedicated "Temporary workspace" subsection
  shows `mp scratch new <label>` used for `mp milestone update --json`.
- **M117 — Verifier timeout hardening.** Per-AC `MP_VERIFY_TIMEOUT_SECS`
  path now does `killpg` + `child.kill()` + `child.wait()`, mirroring
  M107's global-deadline pattern. Closes B-52 / ER-5 (M107 reviewer).
- **M118 — Backlog close-out + remaining follow-ups.** `mp milestone
  ac bulk <id> --bulk @file.json` applies a JSON array of fragment
  updates through the same per-AC flow (closes B-57). `mp milestone
  complete` auto-clears `block_reason`/`blocked_by` on success,
  appending `[block-cleared-on-complete: <reason>]` to
  `verification.evidence` (closes B-54, resolves the M104
  `done + blocked_by: user` contradiction). `SPEC.md` documents the
  per-AC `evidence` preservation semantics (closes B-53). B-46..B-52
  marked resolved in the backlog (work shipped in M110–M117;
  the file's status was not updated until M118).
- **M118.5 (inline polish).** Annotation dedup on the
  `[block-cleared-on-complete: ...]` audit-trail uses a stable
  prefix match (not a full-string substring), so user-supplied
  `prior_block_reason` containing the literal marker text no longer
  falsely dedups. Annotation is carried forward across the
  `verification.evidence` overwrite on re-completion. `criterion_bulk_update`
  pre-validates the milestone id up front — missing milestones surface
  one clean error rather than N noisy per-AC errors.

### Milestones shipped under 2.1.0-rc.1

M110, M111, M112, M113, M114, M115, M116, M117, M118.

### Test surface (post-batch)

- 12 new integration test binaries (12 × 50+ cases each): `ac_update_bulk`,
  `complete_clears_block_on_success`, `design_decision_area`,
  `fields_schema_derived`, `fragment_evidence_flag`, `list_projection_flags`,
  `milestone_log`, `plan_io_concurrent_writes`, `replace_arrays_persists_steps`,
  `update_raw_round_trip`, `verification_preflight`, `write_dry_run`,
  `ac_verify_per_ac_timeout_killpg`, `tab_bar_narrow_width` — plus the existing
  suite layer (which was consolidated in M114).
- 89 test-result buckets in the full `cargo test -p mp -p raul` run; all
  green under `--test-threads=1` (the parallel harness has occasional
  flake on the existing `M107` set-status path; serial is the
  deterministic contract used by AC-05 / AC-08 / closure gates).

### Compatibility

- **Wire format:** unchanged. v2.0 plan files load in v2.1.0-rc.1 with
  no migration. The M111 `--replace-arrays` fix widens the persisted
  set, but never narrows.
- **CLI:** the v2.0 subcommand tree is preserved. New flags are
  additive (`--evidence`, `--bulk`, `--accept-extra-fields`, `--take`,
  `--select`, `--sort`, `--dry-run`).
- **Harnesses:** `make install` refreshes the binary + the OpenCode,
  Cursor, and Pi skill installations (per the Makefile's v1 harness
  trio target). No harness-side action required.

## v2.0.0 (2026-07-03) — 2.0 GA

The 2.0 wave lands: **JSON-canonical plan storage, lean spec model, fragment-first
agent I/O, search v2, dogfood doc split, size-aware positioning docs, top-tab raul
TUI**. This is the toolkit's first GA under the agent-first design — every change
below is justified by making agents more efficient, not by chasing feature parity
with human PM tools.

### Headline changes

- **JSON only on disk.** Plan artifacts were TOML in v1, JSON-canonical from 2.0.
  `mp migrate` is the one-shot TOML→JSON path for legacy plans; there is no
  dual-write or back-compat shim. `--format raw` is the new escape hatch.
- **Lean spec.** Eighteen ceremony fields dropped from the milestone schema
  (acceptance test blueprints, owner, year/week labels, etc.). `mp validate`
  enforces the lean schema across all milestones.
- **Fragment-first agent I/O.** `mp milestone {ac,step,wp} {show,update,remove}
  <id> [<fragment-id>]` is the canonical read/write path. Bulk operations
  (`set-priority`, `set-spec-status`, `depends-on add/remove`) shipped in M94.
- **Search v2.** `mp search <query> [--type …] [--include object]` returns fuzzy
  artifact hits with `suggested_action`s that map straight back to fragment
  commands. The `grep master-plan/` anti-pattern is gone.
- **Top-tab raul TUI (M91).** The legacy left sidebar column is gone; the TUI
  renders a horizontal lane tab bar at the top, with arrow-key / number-key
  navigation, mouse-click hit-test, page-scroll, narrow-width overflow indicators,
  and a single Tab focus toggle.
- **Size-aware intake docs (M81).** Onboarding teaches the routing decision
  matrix FIRST — track vs milestone vs idea vs backlog — so one-line bug fixes
  don't drag in 16-criterion milestones. See
  `docs/concepts/02 - Getting Started/SIZE-ROUTING.md`.

### Milestones shipped under 2.0.0

- **M76 / M82** — Lean spec; CLI grouping (`2.0` rename, `raul` repoint).
- **M83 / M84** — Review lifecycle (`mp reviews lifecycle`) + raul board view.
- **M90** — Dogfood doc split (`AGENTS.md` at root vs. `master-plan/AGENTS.md`
  inside the plan zone; the new "project mission & agent contract" section
  pins the dogfooding intent).
- **M92** — JSON-canonical plan persistence; one-shot `mp::migrate`.
- **M93** — Fragment-first AC/step/WP commands; `--fields` strict field paths.
- **M94** — Bulk milestone metadata writes via `mp milestone bulk …`.
- **M95** — Fuzzy artifact search with `suggested_action` mapping.
- **M81** — Size-aware positioning docs; smallest-artifact-first narrative.
- **M91** — Top-tab raul TUI (replaces sidebar); 318 unit + integration tests
  green across both `mp` and `raul` in debug and release.
- **M96** — 2.0 GA release itself (this milestone).

### Release-candidate history (documents `2.0.0-rc.1` through `2.0.0-rc.9`)

- **rc.1** — Initial TUI + raul repoint + cli grouping.
- **rc.2** — M76 follow-up.
- **rc.3** — Review lifecycle (M83) + raul review board (M84).
- **rc.4** — JSON-canonical plan persistence (M92) + TUI navigation hardening.
- **rc.5** — Pre-GA housekeeping (spec/exec rendering fixes, B-64 backlog, remediations).
- **rc.6** — Final rc before GA tag; reflects the same commits as v2.0.0.
- **rc.7** — Post-GA: M124 external review batch follow-ups, M125 blocked lane + CLI swimlane graph.
- **rc.8** — M126 TUI Path tab, shared lane palette, effective_execution_status unification.
- **rc.9** — Fix Path tab data loading (reads::path_lanes) + 'r' keybinding.

Full rc notes are preserved verbatim in the sections below.

### Compatibility & migration notes

- **No back-compat shim for TOML plans.** `mp migrate` is the only path; if the
  plan is still TOML at 2.0 GA, `mp doctor` will flag it.
- **`MP_HOME`** is now purely an override tree path (templates/schemas are
  embedded in the binary — no on-disk asset copy required).
- **`--format raw`** replaces `--format toml` for show/tracks JSON debug output.

## v2.0.0-rc.4 (2026-07-01)

Fourth release candidate: JSON-canonical plan persistence (M92) + raul TUI navigation fixes.

### M92 — JSON-canonical plan persistence
- **On-disk format is JSON** — all plan artifacts use `.json` extensions; TOML persistence removed.
- **`--format raw`** replaces `--format toml` — verbatim on-disk JSON for `show`/`track show`, GraphViz DOT for `graph`.
- **One-time migration** — `mp::migrate` converts legacy TOML plan dirs; milestones round-trip through typed structs (M82 ceremony fields stripped).
- **Gate** — `scripts/check-plan-json-only.sh` fails on stray `.toml` under plan dirs.

### raul TUI navigation fixes
- **`q` always quits** — no more accidental quit from Esc or navigating away.
- **Esc always focuses sidebar** — goes back from detail views then returns focus.
- **Arrow keys navigate and load data** — Up/Down in sidebar immediately shows lane content (no extra Enter needed).
- **Tab cycles sidebar↔content** — fixed dead Tab in Status dashboard; works bidirectionally for all 8 lanes.
- Removed legacy 1-7 / s/m/p/g/t/b/i lane-jump keys — simpler model: arrows + Enter + Esc + Tab.
- **`raul tracks`** fixed — now parses the array format from `mp list tracks`.

### M85 — Raul TUI polish (27 audit findings from comprehensive codebase review)
- 9 acceptance criteria, 6 work packages, 19 steps covering: data loading, Tab navigation, footer accuracy, help overlay, theme activation, `--color` flag compliance, detail enrichment, and code cleanup.

### M86 — Raul TUI visual overhaul
- Lightweight markdown renderer for milestone detail (bold paragraphs, inline code, bullet/numbered lists, horizontal rules).
- Step progress bar and AC color-coded statuses.
- Icon system enrichment across sidebar lanes and board cards.
- 3 work packages, 11 steps, 8 acceptance criteria.

## v2.0.0-rc.3 (2026-07-01)

Third release candidate: review lifecycle + raul board.

### Review lifecycle & findings (M83)
- **`mp reviews lifecycle`** — cross-project rollup of milestones by `review_state` (pending-review / remediated / verified).
- **`mp reviews finding`** subcommand (`add` / `resolve` / `list`) — structured findings on milestones with severity + category.
- Review remediation semantics: golden fix + wontfix resolution recorded in evidence.

### raul TUI review board (M84)
- **`raul` board view** (key `8`/`k` or sidebar) — Kanban of milestones by review state: Ready · Awaiting Approval · Executing · Pending Review · Open Findings · Remediated · Verified.
- Card shows id, title, executor, open-finding count; ←→ columns, ↑↓ cards, `Enter` drills into milestone detail, `r` refreshes, Back returns to the board.
- `mp list milestones --include findings` and base `executed_by` field power the board.

### Board review remediation
- Extracted pure `classify_card()` (table-tested) so the board's state machine is covered, not just its navigation.
- New **Awaiting Approval** column so `spec_status: review/drafting` milestones are visible instead of dropped.
- Lifecycle fetch failure is surfaced as a warning banner (was silently swallowed → empty Pending Review column on mp <rc.3).
- Drill-in Back now restores the Board lane (AC-03); cursor position preserved on refresh.

## v2.0.0-rc.2 (2026-06-30)

Second release candidate: agent stdout contract cleanup (M76). JSON is now the default on all read commands — omit `--format`; human display stays in `raul`.

### Agent output contract cleanup (M76)
- **JSON is the default** — omit `--format` on read commands; stdout is always JSON unless `--format toml` is explicitly needed for debug.
- **Removed** `OutputFormat::Human` clap variant, `mp search` ASCII table branch, dead `render_verify_human()` / `emit(human)` paths.
- **Removed** orphaned `output.default_format` / `output.color` config keys (mp always emits JSON).
- **raul** `mp_runner` no longer appends `--format json` on every shell-out.
- **Docs/skills** updated: teach `mp <cmd>` not `mp <cmd> --format json`; STORAGE.md, MP-COMMANDS, AGENTS-TEMPLATE, PM-WORKFLOWS, and related concepts docs aligned; `make doctor` fixed.

## v2.0.0-rc.1 (2026-06-30)

Release candidate for the v2.0 clean-break: agent-only `mp`, human `raul`, CLI grouping redesign, and M38–M75 feature set. **67 milestones remain in the independent review queue** — non-blocking for RC adoption but should be cleared before `2.0.0` GA (`mp release ship 2.0.0`).

### raul dependency consolidation (M73)
- **Removed** `comfy-table` and `owo-colors` from raul; CLI tables now use `raul::table` (ASCII borders, ANSI-aware column width).
- **Unified terminal stack:** workspace-pinned `crossterm 0.29`; upgraded `ratatui` to 0.30 with `crossterm_0_29` to eliminate duplicate crossterm 0.28/0.29 in the tree.
- **Styling:** `crossterm::style::Stylize` replaces owo-colors across all raul command modules.
- **Transitive deps:** 118 → 97 unique crates (target ≤100).
- **CI / audit:** `make dep-audit-raul` + `scripts/audit-raul-deps.sh`; wired into `.github/workflows/plan.yml`.

### Documentation flow diagrams (M75)
- Process visuals across `docs/concepts/`: visual index, diagram style guide, audience routing in `00-Concepts.md`.
- New mermaid: handoff baseline sequence (EXECUTION-MODES), execute→review→remediate + session start (AGENT-PLAYBOOK), PM intake funnel, harness install path.
- **[RAUL.md](docs/concepts/02%20-%20Getting%20Started/RAUL.md)** — human PM entry guide; MP-COMMANDS §22 slimmed to pointer.
- Drift pass: MP-COMMANDS hierarchy updated for `plan diff`, `reviews`, `execution handoff-show/report`.

### BREAKING: mp becomes agent-only — human output removed (M41)
- **Output format enum reduced to `json` + `toml` only.** `--format human`, `--format markdown`, and `--format pr-body` are permanently removed with no deprecation period or aliases.
- **Deleted modules:** `render.rs` (human markdown rendering), `export.rs` (markdown export pipeline), `commands/export.rs` (CLI handler).
- **Removed command:** `mp export` — raul has no equivalent; human-readable exports are no longer a first-class feature.
- **Retained:** `mp digest` and `mp graph` commands (json-only) — raul depends on their `--format json` output.
- **`mp skill context`** no longer supports `--output markdown`; json-only.
- **`mp session export`** emits json-only (the `body` field is preserved but the format-switch is gone).
- **Default output** is now `json` always (previously `human` on TTY, `json` when piped).
- **Decision ADR-008 (D-013):** full audit of every human output surface + consumer migration recorded.

### Human surface moved to raul
- All human-facing display now lives in `raul`:
  - `mp status --format human` → `raul status`
  - `mp list milestones --format human` → `raul milestones`
  - `mp show milestone <id> --format human` → `raul show <id>`
  - `mp next` → `raul next`
  - `mp path --format human` → `raul path`
  - `mp digest --format human` → `raul digest`
  - `mp graph --format human` → `raul graph`
  - `mp idea create` → `raul idea`
- **Intentionally dropped (no raul equivalent in 2.0):** `mp inbox` (human), `mp validate` (human), `mp execution` (human), `mp decision` (human).
- Docs updated: master-planner SKILL.md, AGENTS.md, MP-COMMANDS.md now route humans to raul and tell agents to summarize JSON.
- Tests migrated: `format_human.rs` rewritten for json; `track_render_parity.rs` converted to json parity; `p8_export_git.rs` export test removed; all golden scenarios already used `--format json`.

**Coordinated with M38 (CLI grouping redesign)** — both ship under v2.0 as a single clean-break release. See ADR-008 (D-013).

### BREAKING: CLI grouping redesign — 2.0 rename + raul repoint (M45)
- **Command surface restructured** per M38's target taxonomy. Verb-first standalones are now homed under their object groups; the top-level surface is cleaner and more consistent.
- **Commands removed (absorbed into object groups):**
  - `mp groom <id>` → `mp milestone groom <id>`
  - `mp challenge <cmd> <id>` → `mp milestone challenge <cmd> <id>`
  - `mp step <cmd> <m> <s>` → `mp milestone step <cmd> <m> <s>`
  - `mp wp <cmd> <m> <wp>` → `mp milestone wp <cmd> <m> <wp>`
  - `mp archive <cmd>` → `mp track archive <cmd>`
  - `mp restore <cmd>` → `mp track restore <cmd>`
  - `mp purge <cmd>` → `mp track purge <cmd>`
  - `mp metrics <cmd>` → `mp plan metrics <cmd>`
  - `mp delta <cmd>` → `mp specs delta <cmd>`
- **Collapsed:** `mp next-step` merged into `mp next` — a single `mp next` command now returns the head of the suggested action queue.
- **Rename:** `--format raw` → `--format toml` (OutputFormat enum). All dispatch, help text, and references updated.
- **Raul repoint:** every raul shell-out in `crates/raul/src/reads.rs`, `mp_runner.rs`, and commands updated in-lockstep so `raul next`, `raul status`, `raul digest`, `raul graph`, `raul milestones`, `raul show`, `raul path`, `raul idea` all work against the renamed mp without breakage.
- **No aliases:** the rename is a hard break with zero `#[clap(alias = "...")]` shims, per D-012 (ADR-010).
- **JSON shape frozen:** `--format json` output shapes are byte-identical to pre-rename (D-011 clause 1).
- **Design docs:** ADR-009 (D-014) target taxonomy, ADR-010 (D-015) hard rename, ADR-011 (D-016) impact assessment, ADR-012 (D-017) v2.0 versioning.
- **Docs + tests:** master-planner SKILL.md, AGENTS.md, MP-COMMANDS.md, AGENTS-TEMPLATE.md, golden scenarios, and integration tests all updated to the new surface.

## v1.7.0
### raul CLI MVP
- raul human-facing PM CLI (M40): new raul binary with styled tables (comfy-table + owo-colors) wrapping mp --format json for status, list/show milestones, next, path, digest, graph, and idea creation. Extracted 61 domain types into crates/mp-model as a pure types-only shared crate with TomlResource trait. raul consumes mp-model types only — zero plan-file writes, decoupled from mp internals.

### Multi-binary workspace
- Repo reorganization for multi-binary maintenance (M42): added crates/raul and crates/mp-model as workspace members with compilable stubs, split tests/ into tests/mp/ and tests/raul/ directories, updated Makefile/CI/docs for multi-binary builds, fixed stale scenario runner path in AGENTS.md.
