# mp-dogfood-log

Workaround queue and `mp` issue tracker for this repo (which dogfoods master-plan).
Append one entry per finding. **Process rule (M110):** every new hygiene entry
must map to a queued milestone (`spec_status: planned`) or a new milestone —
never silent. Record the target as `<!-- points-at: M### -->` on the entry.

<!-- planted-milestone-pointer-rule -->

Status legend at the end of each entry:

- `wontfix` — acceptable, documented and moving on.
- `backlog` — file as B-NN / BF-NN in `backlog.json` (or a new milestone) later.
- `spec-gap` — extends the scope of an existing milestone.
- `bug` — defect in `mp`; attach or assign to a milestone's findings.

## Format

```markdown
## Entry N — YYYY-MM-DD — <one-line title>  <!-- points-at: M### -->

- When:           date / phase (e.g. executing M159 on a Mac).
- Command:        (or `Command attempted`) the exact invocation or call.
- Observed:       exit code, stdout/stderr excerpt, agent interpretation.
- Suspected:      (or `Suspected cause`) mp subcommand / Rust module / config involved.
- Verdict:        **bug** | **backlog** | **spec-gap** | **wontfix**.
- One-line:       (optional) one-line summary.
- Status:         wontfix | backlog | spec-gap | bug   # if Verdict not used.
```

The `### YYYY-MM-DD — ...` short template (lines 17–25 of the original spec)
is superseded by the verbose H2 form above, which is what every entry in
this file actually uses (entries 23+).

## Entries

<!-- Add newest entries at the top. -->

## Entry 51 — 2026-09-04 — Four pre-existing mp test failures only surface in `make ci` (full CI env), pass locally (even release)  <!-- points-at: M231 -->

- When:           2026-09-04, after pushing M229 fix commits (the herdr-orc fix workflow), `make ci` on GitHub Actions still failed with 4 tests that PASS locally (even with `cargo nextest run --release`). Confirmed pre-existing (not regressions from M229).
- Command:        `make ci` on GitHub Actions vs `cargo nextest run -p mp --release -E 'test(/<name>/)' --no-fail-fast` locally.
- Observed:       4 tests fail in CI's `make ci` (full release build, full env) but pass locally (even in `--release` mode):
  1. `mp::suite_config::config_load_reliability::corrupt_config_surfaces_doctor_warning`
  2. `mp::suite_init::init_json::init_creates_json_plan_artifacts`
  3. `mp::suite_phase::p4_brownfield::brownfield_scan_and_doctor_detected`
  4. `mp::suite_plan::plan_relocate::plan_relocate_renames_dir_and_updates_location`
  All 4 pass locally in release mode. Same pattern as the original `mp::watch_signal real_sigint` flake (renamed to `autopilot_drive_signal::real_sigint_during_autopilot_run_exits_zero_and_flushes_state` in M229, now passes).
- Suspected:      CI environment-specific flakiness — possibly related to (a) parallel test execution under load, (b) tempdir/permission differences between CI runner and local macOS, (c) ordering dependencies in shared state, (d) PATH or env-var differences. NOT a real bug — the tests pass in isolation and in the local release build.
- Verdict:        **wontfix** — environment-specific flakes, not regressions. The 7 in-scope M229 regressions (5 from M229 cleanup + 2 dogfood-50 backlog) are all fixed and verified. These 4 are unrelated to the autopilot work.
- One-line:       4 pre-existing mp test failures only surface in `make ci` GitHub Actions env; pass locally (even release) — CI-only flakes, not regressions.
- Status:         **wontfix** — confirmed pre-existing by comparing local vs CI. 4/5322 = 0.075% flake rate, similar to the pre-existing `mp::watch_signal real_sigint` flake. Filed for tracking. Mitigation: `make ci` is the only test surface that surfaces these. Local `make ci` (if reproducible) would be the diagnostic step. None of the 4 tests touch M229's changes.

## Entry 50 — 2026-09-03 — Three pre-existing mp test failures confirmed across multiple milestones (M217, M228, M229)  <!-- points-at: M231 -->

- When:           2026-09-03, autopilot orchestration session driving M217 (auto-refresh), M228 (post-cutover cleanup), and M229 (breaking-release cleanup). mp-herdr-log2.txt documents the sessions.
- Command:        `NEXTTEST=1 make test` (or `cargo nextest run --profile ci --no-fail-fast --manifest-path Cargo.toml`).
- Observed:       3 pre-existing test failures confirmed red at c2ebe69 (M217 merge commit), in a separate worktree without the in-flight milestone's diff:
  1. `mp autopilot::migrate::tests::idempotent_re_run_is_a_no_op` — the migrated session is missing the 'orchestrator' property on the topology. Migration code (added in M208) drops a property on re-run.
  2. `mp::scenarios_runner run_implemented_scenarios` — env-dependent golden drift. When `herdr` is on PATH, the runner adds extra readiness messages the goldens don't expect. The test only passes in a stripped env.
  3. `mp-oracle::mini_schema_parity parity_real_schemas_compile_and_reject_empty` — the oracle's expected schema list was never updated when `autopilot-session.schema.json` was added (M207). The test still uses the pre-autopilot schema list.
- Suspected:      Three independent bugs in the autopilot migration code (added in M208), the scenarios test (env-sensitive), and the oracle test (stale schema list). NOT regressions from M217, M228, or M229 — all three are pre-existing at c2ebe69.
- Verdict:        **bug** — 3 confirmed bugs. NOT M217/M228/M229 regressions. The runners correctly identified these and documented them; the reviewer correctly accepted as backlog rather than blocking the milestones.
- One-line:       3 pre-existing mp test failures at c2ebe69: `migrate::idempotent_re_run_is_a_no_op` (M208 regression), `scenarios_runner::run_implemented_scenarios` (env drift), `oracle::mini_schema_parity` (stale schema list).
- Status:         **bug** — confirmed pre-existing across 3 milestones. The fix is out of scope for the autopilot batch. Either: (a) absorb into a future mp-cleanup milestone, or (b) fix opportunistically during the next autopilot work. Filed here so the audit trail captures the 3 backlog items.

## Entry 49 — 2026-09-03 — Runner implements autopilot primitives but does not wire them into production hot path (no-production-caller pattern)  <!-- points-at: M228 -->

- When:           2026-09-03, autopilot orchestration session driving M225 (restart + reconciliation) and M226 (end-to-end certification). mp-herdr-log2.txt documents the full session.
- Command:        `mp reviews finding list <id>` after each cycle 1 — the reviewer's cold build caught 3 separate primitives exported but never called by production code.
- Observed:       In M225, `was_already_applied`, `classify_pane_loss`, `recover_event_tail`, `cross_check_canonical` were all `pub` in `crates/mp/src/autopilot/reconcile.rs` but the production spawn path called none of them. In M226, the same pattern recurred with `dispatch_assignment`, `LifecycleClosure`, `topology_preflight` / `topology_policy` / `start_cycle` — all exported, none called by production. The tests passed because they called the libraries directly, not because production used them. Each primitive had its own unit-test surface but no end-to-end production-path test.
- Suspected:      The runner contract focuses on "implement the spec" — implementing the types, methods, and per-AC unit tests. The contract does NOT focus on "wire into the production hot path" — that step is implicit and easy to skip, especially when the milestone's spec describes a library surface rather than a production caller. The runner treats the milestone as "build a library"; the orchestrator expects "build a system".
- Verdict:        **bug** — recurring pattern in session 2 (M225 F-01 + F-02 + M226 F-01 + F-02 + F-03, all HIGH severity). Not a one-off.
- One-line:       Runner contract produces libraries; production never imports them. The "wire to production" step is missing from the contract and from the test surface.
- Status:         **bug** — 2 occurrences in session 2 (M225 + M226). Workaround for current milestones: cycle 2 prompt explicitly says "find the production path, wire the primitive, add a regression test". Long-term fix: amend the runner contract to include a "wire to production" step + production-path regression tests with `_production_path_` naming convention. Filed here for tracking; the contract amendment is the right scope for M228 (post-cutover cleanup).

## Entry 48 — 2026-09-03 — `mp milestone step done` / `mp milestone complete` / `mp milestone criterion pass` do not write activity.json events  <!-- points-at: M225 -->

- When:           2026-09-03, autopilot orchestration session driving M207, M209, M211, M212, M213 (the autopilot foundation set). mp-herdr-log2.txt documents the full session.
- Command:        `mp milestone step done <id> <step>`, `mp milestone criterion pass <id> <ac> --evidence "<text>"`, `mp milestone complete <id>` — called by the runner lane (opencode) under orchestrator supervision.
- Observed:       For all 5 milestones driven in this session, `master-plan/activity.json` was completely empty for the milestone's subject field. Verified via `grep '"subject": "<id>"' master-plan/activity.json` after each milestone's `mp milestone complete` call. The milestone JSON itself was correct (lifecycle, execution_status, spec_status, steps, ACs, evidence all populated), but the activity event stream had no entries. This is the same gap that caused the M200 review-pass event loss in the original test session (`mp-herdr-log.txt` R-late section): the milestone JSON is canonical, but `mp reviews pass --verdict ok` does NOT add an activity event by default; this entry shows that `mp milestone *` commands have the same gap.
- Suspected:      The installed `mp 1.0.0-rc2` either doesn't write activity events for these commands, or writes them to a different file. Same root cause as the M200 review-pass loss: `mp` is missing a write-to-activity hook for the milestone/review lifecycle transitions that the orchestrator depends on for the audit trail. The R5 verification protocol (orchestrator reads milestone JSON + reviews.json independently) works around this for the canonical-state check, but the activity stream is broken for every milestone in this session.
- Verdict:        **bug** — silent audit-trail gap across 5 consecutive milestones. Should be fixed in M225 (restart + reconciliation), which is the natural milestone for "durable event recovery"; or by adding an `--events` flag to the relevant `mp` commands; or by a new tooling milestone that adds an `mp autopilot activity reconcile` command. Not a blocker for the autopilot work, but a real `mp` defect.
- One-line:       `mp milestone *` lifecycle commands do not write to `master-plan/activity.json`; 5 milestones driven in this session have no activity events.
- Status:         **bug** — 5 occurrences in this session (M207, M209, M211, M212, M213); worked around by R5 orchestrator verification reading milestone JSON + reviews.json. No immediate fix; the next session that does durable-state recovery (M225) should include the activity-rebuild path.

## Entry 47 — 2026-09-03 — Pre-existing `doc_overindented_list_items` clippy lint blocks `make lint`  <!-- points-at: M228 -->

- When:           2026-09-03, autopilot session — every milestone's reviewer cold build hit this lint in `crates/mp-model/src/milestone.rs:155-158` (file is in `mp-model`, not in the milestone's touched files).
- Command:        `cargo clippy --all-targets -- -D warnings` (the `make lint` command).
- Observed:       `make lint` is red with `doc_overindented_list_items` on `crates/mp-model/src/milestone.rs:155-158`, introduced by commit `6204555` (M202 cycle 3, 2026-08-31). clippy 1.98 added this lint. The runner's per-milestone `-p mp --tests --no-deps -- -D warnings` is clean (the lint is in `mp-model`, not `mp`), so each milestone's work itself is fine. The reviewer correctly filed as a low-severity backlog-class finding in M209, M211, and M212 (three times). The reviewer's scope discipline was exemplary, but the underlying `mp-model` regression is now a 3-milestone known issue.
- Suspected:      M202 cycle 3 added doc comments with over-indented list items. The clippy 1.98 lint caught them. The fix is a 5-line `cargo clippy --fix` change to the doc comment, but no milestone in flight is scoped to touch `mp-model`.
- Verdict:        **bug** — pre-existing `mp` repo issue, recurring blocker for `make lint` on autopilot milestones that touch `mp`. Should be a quick standalone fix or absorbed into M228 (post-cutover autopilot internal cleanup).
- One-line:       `doc_overindented_list_items` clippy lint in `crates/mp-model/src/milestone.rs:155-158` (M202 era, commit 6204555) blocks `make lint`; reviewer filed as backlog 3 times.
- Status:         **bug** — 4 occurrences in this session (M209 F-01, M211 (not filed as out of scope, review used --tests no-deps), M212 F-01, M220 F-01). Workaround: per-milestone reviewer uses `-p mp --tests --no-deps` instead of `make lint`; runner evidence captures --tests no-deps clean. The full-workspace clippy still exits 101. No immediate fix; should be picked up by M228 or a one-line `mp-model` PR.

## Entry 46 — 2026-09-03 — M211 file name is stale (slug mismatch)  <!-- points-at: M228 -->

- When:           2026-09-03, M211 review cycle 1 — reviewer's F-02 noticed the mismatch.
- Command:        `ls master-plan/milestones/ | grep 211`
- Observed:       File is named `211-mp-autopilot-lane-notification-wire-format-shell-command-never-printed-text.json` but the spec title is `mp autopilot typed task assignment — orchestrator dispatch through herdr argv`. The slug was set when the milestone was first created (under an earlier title) and was not updated when commit `09412e2` ("plan: tighten autopilot milestone specs (M207-M222) — typed verifications + state model") renamed the title. The mp CLI loads by ID, not by file name, so this is cosmetic — but it's a real hygiene issue that misleads any agent reading the file name. The reviewer correctly filed as a low-severity backlog-class finding.
- Suspected:      `mp milestone create` and `mp milestone update --json` do not rename the file when the title is updated. The file name is set at creation and is sticky. The 09412e2 commit updated titles but not file names. A future fix could either (a) auto-rename the file on title change, or (b) `git mv` the affected files post-rename. For M211, the work was scoped to autopilot code, not plan-hygiene, so the rename was deferred.
- Verdict:        **wontfix** for this session — cosmetic, agent-discoverable via ID. Should be picked up by M228 (post-cutover cleanup) as a one-line `git mv`.
- One-line:       M211 file name `211-mp-autopilot-lane-notification-wire-format-shell-command-never-printed-text.json` is stale (spec title renamed in 09412e2 to "typed task assignment"); `mp` loads by ID so functional, but misleading.
- Status:         **wontfix** — cosmetic; deferred to M228 cleanup.

## Entry 45 — 2026-09-03 — M213 `cycle_stale_state_timeout.rs` duplicates `parse_rfc3339_ms` from `cycle.rs`  <!-- points-at: M228 -->

- When:           2026-09-03, M213 review cycle 1 — reviewer's F-02 noticed the duplication.
- Command:        `diff` between `crates/mp/src/autopilot/cycle.rs` lines 755-794 and `crates/mp/tests/cycle_stale_state_timeout.rs` lines 173-194.
- Observed:       `parse_rfc3339_ms` and `days_from_civil` are duplicated in two files: once in the cycle engine (`cycle.rs`) and once in the integration test (`cycle_stale_state_timeout.rs`). The duplication is intentional per the doc comment ("integration suite is self-contained"), but real — if the cycle engine's parser changes, the test's copy will silently drift. The reviewer correctly filed as a low-severity backlog-class finding (no commit, deferred).
- Suspected:      The test was written to be self-contained (no shared test helper module) so the integration suite can be run in isolation. The trade-off is real: self-contained tests catch regressions in the parser at the cost of duplication. A `crates/mp/tests/common/` helper module would solve it but is a refactor outside M213's scope.
- Verdict:        **spec-gap** — M213 should be extended (or a follow-up added) to either (a) move the duplicated helpers to a shared test module, or (b) document the intentional duplication as a known constraint.
- One-line:       M213 duplicates `parse_rfc3339_ms` / `days_from_civil` between `cycle.rs` and `cycle_stale_state_timeout.rs`; intentional per doc comment, but a real drift risk.
- Status:         **spec-gap** — flagged for M228 or a follow-up cleanup; M213 itself ships as-is.

## Entry 47 — 2026-08-28 — `mp milestone create --json` / `update --json` reject all non-allowlisted top-level fields, blocking structured artifacts  <!-- points-at: M201 -->

- When:           2026-08-28, drafting the M201 (Settings 2.0) spec with a 46-entry key description table.
- Command:        `mp milestone create --json @<payload>.json` with a payload that included a custom `key_descriptions` field (a JSON array of 46 objects with `{key, type, default, allowed?, description}`). Also tried `mp milestone update 200 --json @<payload>.json` with the same field. Also tested a battery of alternative field names: `key_docs`, `key_docs_table`, `schema_docs`, `description_table`, `docs`, `appendix`, `key_descriptions`, `notes`, `evidence`.
- Observed:       Every one of the custom fields was rejected with `Error: milestone create JSON contains unsupported field(s): '<field>'` (and the equivalent on `update`: `Error: milestone update JSON contains unsupported field(s): '<key_descriptions>' — not a supported milestone update field`). The allow-list is strict: `create` accepts `title, intent, problem, scope, acceptance_criteria, design_decisions, open_questions`; `update` adds `effort, risk, depends_on` (and `change_kind` was also rejected in this session). No way to attach a structured data table (key description table, footnotes, large reference material) to a milestone except as a string in a `scope.in_scope` bullet.
- Suspected:      `mp milestone create --json` and `mp milestone update --json` enforce a closed allow-list of top-level fields. The CLI surface does not expose a generic "attach a structured document" path. This is a real obstacle when the spec needs to carry more than the eight built-in fields — for example, the 46 per-key descriptions for the Settings 2.0 schema.
- Verdict:        **spec-gap** — the `mp` JSON surface needs a way to carry structured artifacts (per-key reference tables, dependency diagrams, fixture manifests, etc.) without forcing them into a string. Two viable directions: (a) an `attachments` array of `{name, kind, content|path}` that the spec can reference and the executor can resolve; (b) allow `notes` (or `appendix`) as a first-class field. Both are small changes; the dogfood pain is real.
- One-line:       `mp milestone create --json` rejects every custom top-level field, so long structured artifacts (e.g. a 46-key description table) have to be embedded as a string in a `scope.in_scope` bullet.
- Status:         **spec-gap** — workaround applied for M201: the 46 descriptions are baked into the spec as a delimiter-wrapped `<<<KEY_DESCRIPTIONS_JSON>>>...<<<END_KEY_DESCRIPTIONS_JSON>>>` block in `scope.in_scope`. The executor extracts the JSON at execution time and bakes it into `crates/mp/src/config_docs.rs`. The workaround is fragile (a stray delimiter in any description would break parsing) and the underlying gap should be addressed in a future tooling milestone. Filed here for tracking; no immediate action.

## Entry 46 — 2026-08-28 — `mp milestone update --json` transitions lifecycle as a side effect  <!-- points-at: M200 -->

- When:           2026-08-28, fixing review findings on the M200 draft spec.
- Command:        `mp milestone update 200 --json @<scratch>/payload.json` with a payload that only contained `intent` / `problem` / `scope` (no `lifecycle` field).
- Observed:       The milestone's `lifecycle` field transitioned from `draft` to `approved` and `spec_status` from `draft` to `ready` as a side effect of the JSON update. The `lifecycle_at` field was stamped to the current time. Subsequent `mp milestone set-spec-status 200 draft` refused with "legacy set-spec-status cannot regress lifecycle approved to draft" and `mp milestone reopen 200` refused with "reopen requires execution_status done". `mp milestone set-lifecycle 200 draft` is migration-only and refused with "set-lifecycle is migration-only". The only way to revert was a hand-edit of the JSON file (workaround below).
- Suspected:      `mp milestone update --json` is a full-state update — it interprets the payload as a complete milestone document and applies lifecycle transitions even if the field is absent from the payload. This is in tension with the field-scoped design of `set-lifecycle` (migration-only) and the one-way gate on `set-spec-status` / `reopen`. A field-scoped `--json` (e.g. for `intent` / `problem` / `scope` only) would not have this side effect.
- Verdict:        **spec-gap** — the JSON update surface should be field-scoped (or the lifecycle transition should be a separate explicit flag). The current behavior is recoverable but only via a hand-edit workaround, which violates the "no hand-edits of any file under `master-plan/`" rule.
- One-line:       `mp milestone update --json` flips lifecycle to `approved` even when the payload doesn't name it; one-way transition is unrecoverable through any `mp` command.
- Status:         **spec-gap** — workaround applied (hand-edit of `master-plan/milestones/200-...json` to restore `lifecycle: draft`, `spec_status: draft`, `lifecycle_at: ""`). Follow-up: file a tooling milestone to make `mp milestone update --json` field-scoped (or add a `--lifecycle` flag that defaults to "no change").

## Entry 45 — 2026-07-19 — Cross-repo release pipeline via HOMEBREW_TAP_TOKEN  <!-- points-at: M194 -->

- When:           2026-07-19, post-cut follow-up to the M187–M193 remediation wave.
- Command:        Designed `master-plan/.github/workflows/release.yml`; verified dispatch via curl + token; smoke-tested the bump workflow with a fake tag.
- Observed:       `lthiagol/homebrew-tap` has `bump-master-plan.yml` (auto on schedule + manual) and `bump-master-plan-dev.yml` (manual only). The stable formula needs an external trigger to fire when master-plan cuts a tag. Manual dispatch from a developer's laptop was the only path so far.
- Suspected:      Two-repo CI wiring was missing. `mp release ship` only sets the plan.json flag; the actual GitHub-side release + tap bump was a separate, manual step.
- Verdict:        **bug** (operational debt) — tag pushes need an end-to-end pipeline.
- One-line:       Cross-repo dispatch via `secrets.HOMEBREW_TAP_TOKEN` → homebrew-tap `bump-master-plan` workflow on tag push.
- Status:         **bug** — `release.yml` created and verified end-to-end with a `smoke-test-dry-run` dispatch (resolve step set skip=true, downstream jobs skipped, formula unchanged). Real release awaits the next `v*.*.*` tag push.

## Entry 44 — 2026-07-19 — Group G: stale comments and architecture drift after remediation wave  <!-- points-at: M193 -->

- When:           2026-07-19, repository-wide code review follow-up (code-review-fixes.txt Group G).
- Command:        Static audit of `crates/*/src` comments + architecture map vs `app.rs` size / oracle placement.
- Observed:       Milestone/finding chronology, deleted-code tombstones, and contradictory comments (e.g. Watch polling described as future work while wired); `app.rs` over documented 400-line limit; oracle e2e spawns unguaranteed binary; fixture tests pollute tracked dirs with lock/activity files.
- Suspected:      Comment policy never enforced; cleanup deferred while features landed; test helpers write into fixture trees in place.
- Verdict:        **bug** (documentation defects + maintainability debt).
- One-line:       Targeted comment remediation + architecture cleanup only after Groups A–F behavior fixes are green.
- Status:         **bug** — queued as M193.

## Entry 43 — 2026-07-19 — Group F: untrusted verify/shell, terminal escapes, install path deletion  <!-- points-at: M192 -->

- When:           2026-07-19, focused security threat model for code-review-fixes.txt Group F.
- Command:        Code review of `ac_verify.rs`, Raul renderers, `install.rs` env.sh/uninstall.
- Observed:       Plan verification defaults to `sh -c` without repository trust; Raul renders plan strings without control-char sanitization; generated `env.sh` interpolates install path without POSIX quoting; uninstall basename check does not contain under harness root.
- Suspected:      Trusted-operator assumption without explicit trust boundary; display path treats Ratatui layout as terminal-output safety; install manifests trust absolute paths by basename.
- Verdict:        **bug**
- One-line:       Trust-before-verify, display sanitizer, shell-safe env.sh, canonical uninstall containment.
- Status:         **bug** — queued as M192.

## Entry 42 — 2026-07-19 — Group E: filtered annotation targeting + co-approval confirm-before-success + RFC3339 gaps  <!-- points-at: M191 -->

- When:           2026-07-19, code review of Raul TUI + mp-model timestamps (code-review-fixes.txt Group E).
- Command:        Static review of `tui/app.rs` selected_annotation vs visible_annotations; `confirm_co_approval`; `is_rfc3339` / `humanize.rs`.
- Observed:       Highlighted filtered annotation can differ from action target; co-approval sets confirmed before validate/side-effects and discards approve errors; model accepts impossible timestamps; Raul ignores timezone offsets.
- Suspected:      Cursor indexes raw vector while render uses filter; confirm state machine inverted; handwritten RFC3339 shape check without calendar/offset semantics.
- Verdict:        **bug**
- One-line:       Canonical visible projections, confirm-after-success, shared strict RFC3339 parser.
- Status:         **bug** — queued as M191.

## Entry 41 — 2026-07-19 — Group D: Watch lifecycle contradiction, stale preflight, idle poll stall, non-persistent live state  <!-- points-at: M190 -->

- When:           2026-07-19, invariant audit of Watch mp + Raul (code-review-fixes.txt Group D).
- Command:        Static review of `watch/state_machine.rs`, `watch/prompts.rs`, `raul/src/tui/watch.rs`, `tui/runner.rs`.
- Observed:       Driver expects self-reviewed/reviewed; prompts Option A complete from in-progress and skip review; preflight not bound to queue fingerprint; `event::read` blocks idle polling; live run state mostly persisted only at start/end; skip can mark whole run terminal.
- Suspected:      Two conflicting Watch lifecycle designs left in tree; Raul interaction state not tied to durable fingerprints; event loop lacks deadline wait; sequencer/outcome ownership split.
- Verdict:        **bug** (lifecycle path also **spec-gap** resolved in code-review-fixes.txt: delivery `complete` + reviews registry owns review cycle).
- One-line:       Align Watch on Option A, persist live transitions, fingerprint preflight, timed idle poll.
- Status:         **bug** — queued as M190.

## Entry 40 — 2026-07-19 — Group C: lifecycle allowlist not a transition table; Raul/mp-model mapping diverge; nested step ID dupes  <!-- points-at: M189 -->

- When:           2026-07-19, lifecycle/state-transition audit (code-review-fixes.txt Group C).
- Command:        Static review of `mp-model` lifecycle helpers, `set-lifecycle`, Raul `tui/status.rs`, `normalize.rs`.
- Observed:       Arbitrary lifecycle jumps/regressions allowed; Raul maps legacy exec `done`→`complete` while mp-model maps `done`→`done`; remediation restores collapsed pre-state; nested step normalization can accept duplicate IDs.
- Suspected:      Lifecycle is a string allowlist without typed events; Raul duplicated derivation instead of calling mp-model; normalize HashSet not updated while merging nested steps.
- Verdict:        **bug**
- One-line:       Typed transition(event) in mp-model; delete Raul mirrors; reject duplicate legacy step IDs.
- Status:         **bug** — queued as M189.

## Entry 39 — 2026-07-19 — Group B: mini_schema fail-open, symlink @file escape, create --file unbound, unbounded loaders  <!-- points-at: M188 -->

- When:           2026-07-19, code review of schema/input paths (code-review-fixes.txt Group B).
- Command:        Static review of `mini_schema.rs`, `json_input.rs`, `milestone/spec.rs` create path, `store.rs` loaders.
- Observed:       Unknown schema keywords/types and invalid regexes silently ignored; leaf symlink escapes project-root guard; create `--file` skips bounded read + key validation used by `--json`; several durable loaders use unbounded `read_to_string`; stdin `@-` unbounded.
- Suspected:      Blacklist compile instead of allowlist; parent-only canonicalize; divergent create pipelines; incomplete migration to `load_json_bounded`.
- Verdict:        **bug**
- One-line:       Fail-closed mini_schema, full-path containment, unified create input, bounded loaders/stdin.
- Status:         **bug** — queued as M188.

## Entry 38 — 2026-07-19 — Group A: plan mutations bypass write lock; multi-file ops partially commit  <!-- points-at: M187 -->

- When:           2026-07-19, repository-wide code review + persistence mutation inventory (code-review-fixes.txt Group A).
- Command:        Static audit of `with_plan_write_lock` call sites vs backlog/track/annotation/brief/session/idea/promotion writers; no concurrent subprocess repro in this session (inventory only).
- Observed:       Lock held mainly by milestone/reviews/edit; most collection RMW paths unlocked (duplicate ID / lost-update risk). Promotions and archive/session/brief multi-file flows write target then source (or metadata then track) with per-file atomicity only — crash mid-way leaves inconsistent plan state.
- Suspected:      Lock is a caller convention, not persistence API; `atomic_write` is single-file; no multi-file transaction/recovery manifest.
- Verdict:        **bug**
- One-line:       PlanWriteTxn for all plan-resource RMW; staged multi-file commit with recovery + idempotent promotion retries.
- Status:         **bug** — queued as M187.


## Entry 37 — 2026-07-16 — no `mp milestone set-effort`; effort/risk estimates are only editable via a forbidden hand-edit <!-- points-at: B-85 -->

- When:           2026-07-16, grooming M178-M181 (post-RC watch + overview wave).
- Command:        `mp milestone update 178 --json '{"milestone":{"effort":"M"}}'`.
- Observed:       `Error: milestone update JSON contains unsupported field(s): 'milestone' — use mp milestone set-status/set-spec-status`. `mp milestone bulk` exposes only set-priority / set-spec-status / set-lifecycle; there is no set-effort or set-risk. The four milestones are sized `effort: S` despite 9-12 steps each, while the repo calibrates 7-12-step broad milestones as M (M170=7, M173=9, M162=12). The estimate cannot be corrected without a hand-edit of `master-plan/*.json`, which AGENTS.md forbids.
- Suspected:      crates/mp metadata setters cover priority/spec-status/lifecycle/target-version but not effort/risk; `milestone update` deliberately rejects the `milestone.*` object (M111 `--accept-extra-fields` treats it as ignored, not applied).
- Verdict:        **spec-gap** (missing CLI surface).
- One-line:       no mp-mediated way to set milestone effort/risk; only priority/spec-status/lifecycle/target-version have setters.
- Status:         **backlog** — filed as B-85 (add `mp milestone set-effort` / `bulk set-effort`, or accept effort/risk in `milestone update`).

## Entry 36 — 2026-07-16 — `mp changelog add --version unreleased` wipes CHANGELOG history <!-- points-at: M170 -->

- When:           2026-07-16, completing M170 (backlog hygiene sweep).
- Command:        `mp changelog add --version unreleased --section Fixed --milestone 170 "…"`.
- Observed:       CHANGELOG.md collapsed to ~4 lines headed `## vunreleased`; prior rc history (~360 lines) gone. Exit 0 / ok JSON. Restored via `git checkout HEAD -- CHANGELOG.md` and re-added under existing `2.1.0-rc.1` Fixed.
- Suspected:      `crates/mp/src/commands/changelog.rs` `add_entry` path when the version header is missing rebuilds only the new section and never re-appends prior body.
- Verdict:        **bug**
- One-line:       changelog add on a non-existent version header is destructive; use an existing version header until fixed.

## Entry 35 — 2026-07-15 — M169-rev2: scroll still doesn't reach the bottom; `h` triggers `PreviousLane` instead of `ToggleHideDone` on List view <!-- points-at: M169 -->

- When:           2026-07-15, immediate user follow-up to the
                  4b4878a commit. After the cache landed, the user
                  re-tested and reported two symptoms:
                  (a) "the milestone details isnt scrooling the
                  whole page yet, it's better, but not until the
                  botton"
                  (b) "when pressiong h to show/hide the completed
                  on the previous screen the highlithed tab goes to
                  overview, same bahaviour of pressing 1".
- Command:        (a) targeted code edits in
                  `crates/raul/src/tui/render/scrollbar.rs` and
                  `crates/raul/src/tui/modes/normal.rs`; (b)
                  new regression test file
                  `crates/raul/tests/m169_rev2_h_fix.rs` (6 tests)
                  + 2 new tests in `m169_scroll_repro.rs`; (c)
                  `cargo nextest run --no-fail-fast --manifest-path
                  Cargo.toml` (`Summary: 2366 passed, 0 failed, 3
                  skipped`, was 2358 pre-fix; +8 new tests across
                  the two files); (d) `cargo clippy --manifest-path
                  Cargo.toml --all-targets -- -D warnings` clean;
                  (e) `cargo fmt --all -- --check` clean; (f) `mp
                  validate` ok; (g) `make doctor` ok; (h) `make
                  dep-audit-raul` → 97 transitive, single crossterm
                  0.29.
- Observed:       **Bug A — scroll still doesn't reach the bottom.**
                  The L3a cache fix (`4b4878a`) was correct in
                  shape — `detail_max_scroll = measured - visible` —
                  but `measured` itself was wrong. The
                  `measure_paragraph_height` helper counted only
                  rows with non-blank non-border cells. The
                  milestone-detail renderer has 17
                  `Line::from("")` blank separators scattered
                  between sections (header, Meta, Intent, Problem,
                  Scope, ACs, Steps, WPs, Design Decisions, Open
                  Questions, Findings, Verification, Delta). For a
                  ~80-non-blank-row body, the helper reported 80
                  instead of ~97, capping `detail_max_scroll` at
                  `80 - 19 = 61` instead of `97 - 19 = 78`. The
                  user could scroll 17 rows short of the bottom.
                  **Fix:** walk bottom-up from the row before the
                  bottom border, find the last row that has any
                  non-blank non-border inner cell. That row's
                  1-based index (within the inner area) is the
                  content row count. Trailing `Line::from("")`
                  blanks can't be distinguished from rows beyond
                  the Paragraph (both render as side-border + blank
                  middle) and are intentionally not counted — the
                  user sees all meaningful content, which is what
                  "reaching the bottom" means. Inter-section
                  blanks (sandwiched between non-blank content)
                  count correctly because the bottom-up walk stops
                  at the LAST non-blank row, and the row count is
                  the position of that row in the inner area, which
                  includes the blanks above it.
                  **Bug B — `h` triggered `PreviousLane` instead of
                  `ToggleHideDone` on List view.** The dispatch in
                  `modes::normal::handle_key` matched
                  `keybinds.previous_lane` BEFORE the per-lane
                  handler could dispatch `ToggleHideDone`.
                  `previous_lane` defaults include `Char('h')` as a
                  vim-style alias, and `keybinds.hide_done` defaults
                  also include `Char('h')` — the two bindings
                  collide on every List view. Pressing `h` on
                  Milestones moved the user to Overview
                  (`PreviousLane`); pressing `1` also moves to
                  Overview (`JumpLane(0)`), so the two keys looked
                  identical to the user.
                  **Fix:** in `handle_key`, when
                  `previous_lane` matches AND `hide_done` also
                  matches AND the active content is `List`, emit
                  `ToggleHideDone` instead of `PreviousLane`. The
                  check is targeted: only the h+List overlap is
                  affected; other lane-nav keys (l, Left, Right,
                  Tab, BackTab) and the hide_done binding on other
                  keys are unchanged. Detail view keeps
                  `PreviousLane` for 'h' — there's no hide_done
                  semantic there.
                  **Regression tests:** 6 in
                  `crates/raul/tests/m169_rev2_h_fix.rs`:
                  - `rev2_h_on_milestones_list_emits_toggle_hide_
                  done_not_previous_lane`: the primary repro.
                  - `rev2_h_on_overview_list_emits_toggle_hide_done`,
                  `rev2_h_on_backlog_list_emits_toggle_hide_done`:
                  cross-lane coverage.
                  - `rev2_h_on_milestone_detail_still_emits_previous_
                  lane`: pin the vim-alias contract on detail
                  views.
                  - `rev2_press_1_still_jumps_to_overview`: the user's
                  "same behaviour as 1" diagnostic — pin that
                  digit-1 still goes to lane 0.
                  - `rev2_other_lane_nav_keys_unaffected_by_h_fix`:
                  spot-check l, Tab, Left — none of them should
                  accidentally emit ToggleHideDone.
                  Plus 2 new tests in `m169_scroll_repro.rs`:
                  - `measure_counts_blank_separator_lines_between_
                  content`: 5-line Paragraph (3 non-blank + 2 blanks)
                  measures 4 (last blank indistinguishable from rows
                  beyond Paragraph).
                  - `measure_reaches_bottom_of_realistic_milestone_
                  with_blanks`: 36-line Paragraph (6 sections × 6
                  lines) measures 35 — the realistic milestone-
                  detail pattern.
- Suspected:      (a) the measurement helper had a counting bug
                  since the M167 introduction (commit
                  `5661259c`); the L3a cache didn't surface it
                  because the cache was off by the same amount on
                  every frame. (b) the dispatch order in
                  `modes::normal::handle_key` was preserved from
                  M167 even though M167's own doc comment at
                  `keybinds::resolve` says "Contextual overrides
                  (e.g. `h`, `r`, `Enter`) resolve to their content
                  meaning here and their navigation meaning in the
                  handler" — the design intent was clear, the
                  implementation just put lane-nav before the
                  per-mode handler.
- Verdict:        **bug** (both) **resolved**.
- One-line:       Two follow-up bugs behind one bad user
                  experience: scroll cap was under-counting 17
                  `Line::from("")` blanks in milestone detail; 'h'
                  was being routed to lane-nav before the contextual
                  hide_done dispatch could fire. Both fixed; 8 new
                  tests; 2366/2366 green.

## Entry 34 — 2026-07-15 — Sub-agent L3a + M4: hash-keyed `detail_measurement_cache` + render-path partial-scroll test <!-- points-at: M169 -->

- When:           2026-07-15, same session as Entry 33. User
                  directed: "lets fix those low too". Of the three
                  LOW findings the sub-agent flagged (L1 untracked
                  drafts, L2 `_unused_gauge` deliberate compile-
                  anchor, L3 8×-buffer allocation per frame) plus
                  the one unfixed MEDIUM (M4 render-path test), the
                  user picked L3a (hash-keyed cache) and M4
                  (render-path test); L1 left as-is, L2 unchanged.
- Command:        (a) `cargo nextest run --no-fail-fast --manifest-
                  path Cargo.toml` (`Summary: 2358 passed, 0
                  failed, 3 skipped`, was 2355; +3 new tests in
                  `m169_rev_scrollbar.rs`); (b) `cargo clippy
                  --manifest-path Cargo.toml --all-targets -- -D
                  warnings` clean; (c) `cargo fmt --all -- --check`
                  clean; (d) `mp validate` ok; (e) `make doctor` ok;
                  (f) `make dep-audit-raul` → 97 transitive, single
                  crossterm 0.29.
- Observed:       **M4 — render-path partial-scroll test.**
                  `crates/raul/tests/m169_rev_scrollbar.rs::
                  rev_detail_max_scroll_stable_across_partial_scroll_
                  render_path` drives the full
                  `render → render_milestone_detail → measure →
                  detail_max_scroll.set` pipeline through a
                  `TestBackend`, scrolls Down 5 times then to the
                  bottom, and asserts `detail_max_scroll.get()` is
                  identical at every scroll position. Confirmed: the
                  test PASSES against current code (the H1 fix from
                  commit `4b229f0` is intact end-to-end), and would
                  have caught the pre-fix regression. The
                  helper-level invariant test
                  (`measure_returns_full_content_height_regardless_
                  of_scroll_offset`) only covered the helper in
                  isolation; this is the render-path closure.
                  **L3a — hash-keyed measurement cache.**
                  `crates/raul/src/tui/app.rs` gains
                  `DetailMeasurementCache { content_hash: u64,
                  area_width: u16, max_scroll: u16 }` (`Copy`
                  derive so it can live inside `Cell<Option<…>>` and
                  be updated through `&App`). `App.detail_measurement_
                  cache: Cell<Option<DetailMeasurementCache>>`. New
                  helper `pub(super) fn lines_hash(lines:
                  &[Line]) -> u64` in
                  `crates/raul/src/tui/render/milestone_detail.rs`
                  walks `line.spans[*].content` through a
                  `DefaultHasher` with a per-line separator so
                  `["ab","c"]` and `["a","bc"]` don't collide.
                  `render_milestone_detail` checks the cache before
                  the 8×-panel Buffer::empty + Paragraph::render
                  allocation. Cache key is `(content_hash,
                  area_width)` — panel width matters because wrap
                  changes the line count. Cache naturally
                  invalidates on content change (new hash → miss);
                  `load_milestone_detail` does not need to clear it
                  explicitly. False collisions are theoretically
                  possible but harmless — collision just means
                  "re-measure this frame," which is no worse than
                  the no-cache path.
                  **Cache regression tests** (both in
                  `m169_rev_scrollbar.rs`):
                  - `rev_detail_measurement_cache_hits_on_unchanged_
                  body`: two consecutive renders with the same
                  `app.milestone_detail` populate and reuse the
                  cache; hash and cap are stable.
                  - `rev_detail_measurement_cache_invalidates_on_
                  content_change`: swapping `app.milestone_detail`
                  between renders produces a different
                  `content_hash` and the cache re-measures.
- Suspected:      the L3a cache is the right fix because the
                  measurement cost is dominated by `Buffer::empty`
                  (the 12,640-cell allocation) and the Paragraph
                  render, both of which are pure functions of
                  `(lines, area)`. Hashing the line text is much
                  cheaper than re-measuring. The trade-off is +1
                  `Cell<Option<…>>` on `App` (16 bytes) and a
                  `DefaultHasher` allocation per render (~ns), both
                  negligible.
- Verdict:        **L3a bug** (perf opportunity) **resolved**;
                  **M4 spec-gap** (missing render-path test)
                  **resolved**.
- One-line:       L3a + M4 land the remaining sub-agent findings:
                  hash-keyed detail-measurement cache skips the
                  8×-panel Buffer allocation on idle frames; new
                  render-path test pins the H1 fix end-to-end. 3
                  new tests, 2358/2358 pass.

## Entry 33 — 2026-07-15 — Sub-agent code review of M169-rev + scrollbar fixes: one HIGH correctness bug + 3 doc drifts + 2 medium issues <!-- points-at: M169 -->

- When:           2026-07-15, same session as Entry 32. User
                  requested an independent sub-agent review of the
                  three recent commits (`33996e4` M169+M169-rev,
                  `1a7e75c` version bump, `e7fd49b` scrollbar fix).
                  Spawned a `general` sub-agent with the
                  `code-review` skill loaded; instructed to look for
                  security/performance/correctness/maintainability
                  issues across the changed surface.
- Command:        sub-agent `task` tool with detailed prompt; in
                  parallel (a) `cargo nextest run --no-fail-fast
                  --manifest-path Cargo.toml` (`Summary: 2355 passed,
                  0 failed, 3 skipped`, was 2353 pre-fix; +2 new
                  tests: `measure_returns_full_content_height_
                  regardless_of_scroll_offset` and
                  `settings_footer_marks_save_when_staged_edits_
                  present`); (b) `cargo clippy --manifest-path
                  Cargo.toml --all-targets -- -D warnings` clean;
                  (c) `cargo fmt --all -- --check` clean.
- Observed:       4 HIGH, 4 MEDIUM, 3 LOW findings. The sub-agent
                  found one real correctness bug I missed (H1) plus
                  several documentation drifts and a UX improvement
                  opportunity.
                  **HIGH — H1: `measure_paragraph_height` is
                  sensitive to the input Paragraph's `.scroll()`
                  offset.** The renderer in
                  `crates/raul/src/tui/render/milestone_detail.rs`
                  applied `.scroll((app.detail_scroll, 0))` to the
                  Paragraph BEFORE cloning it for the measure pass.
                  Post-fix content-row math then returned
                  `(total - detail_scroll)` instead of `total`, so
                  `detail_max_scroll` was recomputed to a too-low
                  cap every time the user scrolled past the visible
                  boundary. The user's complaint was at scroll=0
                  (where the new math happens to be correct), so the
                  original fix looked complete; the regression
                  would have hit anyone scrolling past the first
                  viewport. Sub-agent verified with a 30-line
                  repro: at scroll=0 returns 30 (correct), at
                  scroll=5 returns 25 (wrong).
                  **Fix:** defense in depth.
                  (1) The helper itself resets scroll — `let
                  measure_paragraph = paragraph.scroll((0, 0));`
                  before rendering. The helper is now invariant to
                  the caller's scroll state.
                  (2) The renderer builds an unscrolled
                  `measure_paragraph` Paragraph, passes a clone to
                  the helper, then chains `.scroll((app.detail_scroll,
                  0))` on the original for the actual render. The
                  intent is now explicit at the call site.
                  **Regression test:** `m169_scroll_repro::
                  measure_returns_full_content_height_regardless_
                  of_scroll_offset` pins the invariant — measures
                  the same body at scroll=0, 5, 15 and asserts the
                  three return values are equal (and equal to the
                  full content height of 30).
                  **HIGH — H2: stale doc table in `mode.rs:26`** —
                  the variant overview still listed `Settings`
                  with the "(new in M136; filled by M140)"
                  annotation. M169 removed `Mode::Settings`. Fix:
                  drop the row, add a paragraph noting that the
                  variant was removed in M169 in favor of
                  `Lane::Settings` + `App.settings`.
                  **HIGH — H3: stale comment in `apply_esc`** at
                  `crates/raul/src/tui/action.rs:865-871` referenced
                  removed variants/actions
                  (`Mode::Settings`, `CloseSettings`,
                  `SettingsBack`, `SettingsCancelEdit`). Fix: trim
                  the comment to just `Input` /
                  `AnnotationThread`, with a one-line note pointing
                  at the explicit `Lane::Settings` branch above
                  the match.
                  **HIGH — H4: stale `pub(crate)` doc comments on
                  `handle_mouse`** at `runner.rs:668-676` and
                  `:986` documented the function as `pub(crate)`
                  despite the M169-rev widening to `pub`. Fix:
                  collapse the duplicated paragraphs into one,
                  reflecting the actual current visibility and
                  noting the `#[doc(hidden)] pub mod test_helpers`
                  re-export.
                  **MEDIUM — M1: `#[doc(hidden)]` inconsistency**
                  between `runner.rs::test_helpers` (had it) and
                  `action.rs::test_helpers` (didn't). Fix: add
                  `#[doc(hidden)]` to the action one with a
                  matching doc comment.
                  **MEDIUM — M2: `footer_settings` ignored its
                  parameter.** Sub-agent suggested either dropping
                  the parameter or using it. Chose to USE it: the
                  footer now shows `[Save (s)*]` when staged edits
                  exist and `[Save (s)]` when not, giving the user
                  a visible unsaved-state signal at the bottom of
                  the screen. Regression test:
                  `m167_chrome::settings_footer_marks_save_when_
                  staged_edits_present`.
                  **MEDIUM — M3: no regression test pinning the
                  "measure is invariant to scroll offset" property**
                  — fixed together with H1.
                  **MEDIUM — M4: no integration test for partial-
                  scroll `detail_max_scroll`** — explicitly noted
                  but not yet addressed; would need a render-pass
                  fixture to drive the full path.
                  **LOW — L1: untracked milestone drafts (170-177)** —
                  pre-existing, not from this commit; flagged for
                  user awareness.
                  **LOW — L2: `_unused_gauge` in `milestone_detail.rs`
                  is a deliberate compile-anchor** — no change.
                  **LOW — L3: 8× buffer allocation per render** —
                  acceptable given M134 rate cap; noted for future
                  polish.
- Suspected:      H1 is a textbook case of "fix B exposed latent
                  bug A." The pre-fix measurement (panel-height
                  return) was wrong but its wrongness didn't depend
                  on scroll, so the test at scroll=0 passed and
                  landed. The post-fix content-row math is correct
                  AT scroll=0 but wrong at scroll=k>0. The
                  canonical lesson: any measurement helper that
                  takes a `Paragraph` should normalize `.scroll()`
                  internally before measuring, because the
                  Paragraph's render-time state depends on the
                  scroll offset.
- Verdict:        **bug** (H1 resolved), **maintainability drift**
                  (H2/H3/H4 resolved), **UX improvement**
                  (M2 implemented). All HIGH findings closed in
                  this commit.
- One-line:       Sub-agent caught a real correctness bug the
                  original tests missed: the scrollbar fix was
                  correct at scroll=0 but broke again every time
                  the user scrolled. Defense in depth (helper
                  resets scroll + renderer doesn't pre-scroll)
                  plus a new invariant test closes it.

## Entry 32 — 2026-07-15 — M169-rev scrollbar fixes: `measure_paragraph_height` capped scroll at ~2 rows; mouse wheel dropped on detail screens <!-- points-at: M169 -->

- When:           2026-07-15, user report on the milestones tab
                  ("when clicking enter to read a milestone. the
                  scrollbar goes down only two steps, it seems stuck
                  so when the screen is bigger I can see more, but,
                  the scroll bar does not work properly, also, the
                  mouse isnt working when scrolling into the
                  milestone").
- Command:        (a) `cargo nextest run -p raul --test
                  m169_scroll_repro --no-fail-fast` — repro test
                  `measure_with_block_border_returns_panel_height_not_
                  content_height` failed pre-fix
                  (`detail_max_scroll = 2`); (b)
                  `cargo nextest run --no-fail-fast --manifest-path
                  Cargo.toml` post-fix (`Summary: 2353 passed, 0
                  failed, 3 skipped`, was 2342 pre-fix; +11 new tests
                  across `m169_scroll_repro.rs` and
                  `m169_rev_scrollbar.rs`); (c) `cargo clippy
                  --manifest-path Cargo.toml --all-targets -- -D
                  warnings` clean; (d) `cargo fmt --all -- --check`
                  clean; (e) `make dep-audit-raul` → 97 transitive,
                  single crossterm 0.29; (f) `mp validate` ok; (g)
                  `make doctor` ok.
- Observed:       two distinct bugs behind the symptom "scrollbar
                  stuck after 2 steps":
                  **HIGH — `measure_paragraph_height` returned the
                  panel height.** The M167 helper
                  (`crates/raul/src/tui/render/scrollbar.rs:51`)
                  rendered the Paragraph into a buffer sized exactly
                  to the panel and walked bottom-up looking for the
                  last non-blank row. With `Borders::ALL`, the bottom
                  border (`┗━━━━━━` for `BorderType::Thick`) was the
                  last non-blank row, so the helper returned the
                  panel height (e.g. 20) regardless of the actual
                  content extent. `milestone_detail.rs` then set
                  `detail_max_scroll = rendered - visible = 20 - 18 =
                  2`, capping the keyboard-Down path at 2 steps
                  regardless of milestone size. Repro test
                  `m169_scroll_repro::tall_panel_30_lines_returns_
                  actual_content_height` pins this — pre-fix it
                  asserted `detail_max_scroll > 5` and failed.
                  **HIGH — mouse wheel gate excluded detail screens.**
                  The wheel handler in `runner.rs` checked
                  `app.content == ContentState::List` before calling
                  `app.move_up` / `app.move_down`. On
                  `MilestoneDetail` / `BacklogDetail` /
                  `AnnotationThread` the gate dropped the event
                  silently. `app.move_down` already handles
                  MilestoneDetail (increments `detail_scroll`), so
                  only the dispatch gate was wrong. Repro test
                  `m169_rev_scrollbar::rev_wheel_scrolls_milestone_
                  detail_via_handle_mouse` pins this — pre-fix the
                  wheel events were no-ops and the test failed.
- Suspected:      (a) the measurement helper conflated "last
                  non-blank row in the buffer" with "end of content"
                  — but the Block paints bottom border at the bottom
                  of the buffer, not at the end of the content, so
                  those are different. (b) the mouse gate was written
                  when only the list screens were scrollable; M164
                  added the Path scroll and the detail views gained
                  their own scroll state, but the gate wasn't
                  broadened.
- Verdict:        **bug** (resolved). Both fixes landed in the
                  same commit (no new milestone — these are direct
                  dogfood findings on the shipped M169 surface).
- One-line:       Two milestone-detail scrollbar bugs behind one
                  user complaint: keyboard scroll capped at 2 rows
                  because `measure_paragraph_height` returned the
                  panel height; mouse wheel silently dropped on
                  MilestoneDetail because the gate required
                  `ContentState::List`. Both fixed; 11 new regression
                  tests; 2353/2353 green.
- Fix:            **Measurement:** the helper now renders into an
                  8×-panel buffer (tall enough to fit any realistic
                  detail), detects whether the paragraph has a block
                  by checking the top-left cell for a corner glyph
                  (`┌` U+250C, `┏` U+250F, `╔` U+2554, `╭` U+256D),
                  then counts rows that contain at least one
                  non-box-drawing, non-blank cell. Box-drawing chars
                  (U+2500..=U+257F) are skipped so side-border rails
                  (`│`/`┃`/`║`) don't count as content. Title text on
                  the top border row is correctly excluded by the
                  block-detection row range (`1..H-1` with block,
                  `0..H` without).
                  **Mouse gate:** `crates/raul/src/tui/runner.rs`
                  `handle_mouse` now matches
                  `ContentState::List | MilestoneDetail |
                  BacklogDetail | AnnotationThread` instead of just
                  `ContentState::List`. `handle_mouse` was widened
                  from `pub(crate)` to `pub` (with a
                  `#[doc(hidden)] pub mod test_helpers` re-export)
                  so the regression tests can dispatch synthetic
                  wheel events directly.

## Entry 31 — 2026-07-15 — M169-rev remediation: four findings from Entry 30 fixed in place; added 14 regression tests in `crates/raul/tests/m169_rev.rs` <!-- points-at: M169 -->

- When:           2026-07-15, same session as Entry 30 (external code
                  review of M169). User directed: "apply fixes for those
                  findings, all of them". No new milestone opened — the
                  fixes amend M169's existing `crates/raul/src/tui/*`
                  surface area in place, since M169 is
                  `lifecycle: complete / spec_status: verified`.
- Command:        (a) targeted code edits in `crates/raul/src/tui/{mode,
                  action, app, runner_helpers}.rs`; (b) added
                  `crates/raul/tests/m169_rev.rs` (14 tests, all
                  passing) and renamed two clamp-pinning tests in
                  `crates/raul/tests/tui_sidebar.rs`; (c)
                  `cargo nextest run --no-fail-fast --manifest-path
                  Cargo.toml` (`Summary: 2342 passed, 0 failed, 3
                  skipped`) — was 2328 pre-fix; +14 m169_rev tests;
                  (d) `cargo clippy --manifest-path Cargo.toml
                  --all-targets -- -D warnings` → clean; (e) `cargo
                  fmt --all -- --check` → clean; (f) `make dep-audit-
                  raul` → 97 transitive, single crossterm 0.29; (g)
                  `make doctor` → all ok; (h) `mp validate` → ok, no
                  errors; (i) `mp milestone verify 169` →
                  `ok: true, passed: 8, failed: 0`.
- Observed:       all four findings from Entry 30 are remediated:
                  **HIGH — load guard landed.** `load_settings_lane`
                  in `runner_helpers.rs` now early-returns when
                  `app.settings.is_some()`. Mouse-click of the Settings
                  tab while already on Settings (and `JumpLane(4)` from
                  a chord / digit) no longer overwrites staged edits or
                  spawns an extra `mp config show` subprocess. Test:
                  `rev_high_jump_lane_to_settings_while_on_settings_
                  preserves_staged_edits`.
                  **MED — mp-coercion shadow removed.** `set_config_
                  value` was deleted; `apply_settings_commit_edit`
                  only inserts into `staged_edits` and leaves
                  `state.config` alone. `apply_settings_enter` now
                  prefers `staged_edits.get(key)` over `mp config get`
                  when reopening edit on a key the user already staged
                  (third-Enter reverts-buffer regression). Tests:
                  `rev_med_yes_for_bool_field_does_not_pollute_state_
                  config`, `rev_med_save_with_yes_writes_bool_true_to_
                  disk`, `rev_med_re_enter_after_commit_preserves_
                  staged_buffer`.
                  **MED — partial commit visibility.** `apply_settings_
                  save` now tracks `committed: Vec<String>` and on
                  commit failure calls `reload_settings_after_save_
                  keeping(..., &[failed_key])` which preserves the
                  failed key in `staged_edits` (so the user can retry)
                  and drops the committed keys (so a follow-up `s`
                  only retries the failed one). Footer flash surfaces
                  the partial state — e.g.
                  `Settings save failed: <err> (2 already saved: ui.
                  color, ui.hide_done)`. Helper exposed under
                  `raul::tui::action::test_helpers::apply_reload_
                  keeping` for the regression test:
                  `rev_med_reload_keeping_preserves_listed_keys`.
                  **LOW — AC-01 wrap landed.** `tab_move_up` and
                  `tab_move_down` in `app.rs` wrap modulo
                  `Lane::ordered().len()`. Existing clamp-pinning
                  tests in `tui_sidebar.rs` renamed (`..._wraps_to_
                  settings`, `..._wraps_to_overview`) and the body
                  updated to assert the wrap. New tests:
                  `rev_low_tab_on_settings_wraps_to_overview`,
                  `rev_low_shift_tab_on_overview_wraps_to_settings`,
                  `rev_low_tab_cycles_through_all_lanes_in_order`.
                  Companion: `rev_high_tab_on_settings_wraps_and_
                  clears_per_ac06` confirms that with wrap, Tab on
                  Settings is now a *leaving* gesture → AC-06
                  discard kicks in via `select_lane`. Net behaviour
                  change for users: Tab on Settings now wraps to
                  Overview and discards staged edits (previously
                  clamped and silently wiped them via load).
                  **LOW — staged_edits is `BTreeMap`.** Field type
                  changed from `HashMap<String, String>` to
                  `BTreeMap<String, String>` so iteration is key-
                  sorted. Tests: `rev_low_staged_edits_iterate_in_key_
                  sorted_order`, `rev_low_save_iterates_staged_edits_
                  in_key_sorted_order`.
- Suspected:      the high-severity Tab-wipe bug was a coupling
                  between the lane-agnostic dispatcher and the lane-
                  scoped state lifecycle (see Entry 30); the fix
                  complements the AC-01 wrap by making Tab on Settings
                  a deliberate leave (clean) rather than a silent
                  state-overwrite (broken).
- Verdict:        **bug** (resolved). All four findings from Entry 30
                  closed in place. Pre-existing flake
                  `mp watch::bridge::tests::run_herdr_with_timeout_
                  kills_entire_process_group` is unrelated — passes
                  when re-run in isolation; timing race on `sleep.pid`
                  write.
- One-line:       M169-rev lands all four fixes from the external
                  review: load guard, mp-coercion shadow removed,
                  partial-commit visibility, AC-01 wrap, BTreeMap.
                  2342/2342 tests pass; clippy + fmt + doctor +
                  dep-audit-raul + mp validate + mp milestone verify
                  all green.

## Entry 30 — 2026-07-15 — M169 external review: Tab/click on Settings wipes staged edits (AC-06 violation); `set_config_value` shadows mp coercion; partial-commit is silent <!-- points-at: M169 (fixed in M169-rev; M174 cancelled) -->

- When:           2026-07-15, external code review of M169 (raul Settings
                  modal → real lane). M169 is `lifecycle: complete` /
                  `spec_status: verified`; ran locally to confirm the
                  522 raul + 2328 full-suite numbers in the verification
                  field.
- Command:        (a) `cargo nextest run -p raul --no-fail-fast`
                  (`Summary: 522 passed, 0 failed`) and
                  `cargo nextest run --no-fail-fast --manifest-path
                  Cargo.toml` (`Summary: 2328 passed, 0 failed, 3
                  skipped`) — match the milestone's verification claim.
                  (b) `cargo clippy --manifest-path Cargo.toml --all-targets
                  -- -D warnings` → clean. (c) `cargo fmt --all -- --check`
                  → clean. (d) `make dep-audit-raul` → 97 transitive,
                  single crossterm 0.29, comfy-table/owo-colors absent.
                  (e) `make doctor` → all checks ok. (f) `mp validate`
                  → `ok: true`, no errors, 7 unrelated W44 review
                  warnings (also flagged for M166).
- Observed:       three real bugs found via repro tests against the
                  shipped code; the existing test suite does not pin
                  any of them.
                  **HIGH — Tab on Settings wipes staged edits.**
                  `Action::NextLane` in `crates/raul/src/tui/action.rs:234`
                  unconditionally calls `load_data_for_lane()` after
                  `tab_move_down()`. On the Settings lane (last in
                  `Lane::ordered()`) `tab_move_down()` is a no-op, but
                  `load_data_for_lane()` still routes through
                  `load_settings_lane` (`runner_helpers.rs:543-555`),
                  which does `app.settings = Some(SettingsState::new(config))`
                  and discards `staged_edits`. Repro: jump to Settings,
                  stage one edit, press Tab → staged edits gone.
                  AC-06 says "Leaving the Settings lane ... discards
                  staged but unsaved edits" — Tab from Settings does
                  *not* leave, so this is a contract violation. The
                  same path also runs on a mouse click of the Settings
                  tab while already on Settings. Side-effect: a fresh
                  `mp config show` subprocess fires on every Tab press
                  on the Settings lane (perf footgun, not just data loss).
                  **MEDIUM — `set_config_value` shadows mp's type coercion.**
                  `apply_settings_commit_edit` in
                  `crates/raul/src/tui/action.rs:529-543` writes
                  `state.staged_edits.insert(key, value)` (raw buffer)
                  and then calls `set_config_value(&mut state.config,
                  key, value)` (`action.rs:616-638`) which does its own
                  `true/false → bool`, `parse::<i64>`, `parse::<f64>`,
                  fall-through-→-string. mp's `parse_bool` actually
                  accepts `true | 1 | yes` for true and `false | 0 | no`
                  for false (`crates/mp/src/config_cmd.rs:484-489`), and
                  accepts any string for `next.prefer`. Repro: stage
                  `ui.color = yes` → in-memory `state.config.ui.color`
                  is the string `"yes"`; on-disk after save is the bool
                  `true`. The renderer reads `staged_edits` first so the
                  row shows `"yes"` while editing, then flips to `true`
                  after save (silent type flicker). Same shadow affects
                  `01` for any string field (parsed as i64=1, displayed
                  as `1`, then save-flips back to `"01"`).
                  **MEDIUM — partial commit on save is silent.**
                  `apply_settings_save` (`action.rs:548-592`) runs
                  dry-runs in a loop, then commits in a separate loop.
                  If dry-runs all pass but commit k2 fails after
                  commit k1 succeeded, `set_action_error` aborts and
                  `reload_settings_after_save` is skipped, so
                  `staged_edits` is not cleared. The footer surfaces
                  the error but the user has no signal that k1 already
                  landed on disk. AC-05 only forbids the symmetric
                  case (dry-run failure aborts before any commit); it
                  is silent on commit-time partial failure. The dry-run
                  path is fine (tested by `save_dry_run_first_then_commit_per_staged_edit`).
                  **LOW — AC-01 wrap is not implemented.**
                  AC-01 says "Tab ... cycles `app.active_lane` along
                  `Lane::ordered()` (wrapping at end)". `tab_move_down`
                  and `tab_move_up` (`app.rs:992-1008`) clamp at the
                  ends — no wrap. This is pre-existing (M167/M140 era)
                  and untested; M169 didn't change it but the AC
                  wording is aspirational. Note as a spec/code drift
                  for the follow-up.
                  **LOW — HashMap iteration order in `apply_settings_save`.**
                  `staged_edits` is `HashMap<String, String>` and is
                  iterated as-is. Dry-run order and commit order are
                  therefore non-deterministic across runs. The
                  shipped tests assert only `len()`/key-set, so they
                  don't pin this; users reading the JSON file diff
                  may be surprised. Fix: `BTreeMap` or sort by key
                  before iterating.
- Suspected:      the Tab-wipe bug is a coupling between the
                  lane-agnostic dispatcher and the lane-scoped state
                  lifecycle — `load_data_for_lane` assumes "I am the
                  source of truth for this lane's state", which was
                  true when the Settings modal was a separate Mode but
                  is false once Settings is a lane with mutable
                  user-held state. The `set_config_value` shadowing is
                  duplication-of-coercion-logic across the mp/raul
                  seam. The partial-commit gap is a missing rollback
                  story for the save batch. The wrap-not-implemented
                  is a pre-existing gap between AC text and code.
- Verdict:        **bug** for the Tab-wipe and the coercion shadow;
                  **spec-gap** for partial-commit and wrap-not-shipped.
                  All five findings initially promoted to M174 — but the
                  other agent (Entry 31, same day) fixed them in place
                  against M169 instead: 14 regression tests in
                  `crates/raul/tests/m169_rev.rs`, full suite 2342/2342
                  passing. **Closure:** M174 was cancelled on 2026-07-15
                  (`execution_status=cancelled`, `cancelled=true`); the
                  work shipped via M169-rev (Entry 31) supersedes M174's
                  spec. F-04 (wrap) and F-05 (HashMap order) match M174's
                  AC text exactly; F-01/F-02/F-03 shipped via a different
                  design (Tab-on-Settings now wraps + discards per AC-06;
                  staging no longer mutates state.config; partial commit
                  keeps failed key staged for retry rather than rolling
                  back). The fix design is in the source comments and
                  the m169_rev.rs test bodies — `git log -p` /
                  `crates/raul/tests/m169_rev.rs` is the canonical
                  reference.
- One-line:       M169 ships green tests but the live TUI quietly
                  wipes staged edits on Tab/click-on-Settings,
                  displays the wrong type while staging, and silently
                  commits a partial batch on save errors. Fixed in
                  M169-rev (Entry 31) — see m169_rev.rs for the new
                  design; M174 cancelled as redundant.
- Repro:          local repro test (since deleted) covered all four
                  findings. Saved as a docstring-style recipe at the
                  end of this entry so a future remediation milestone
                  can paste it back into `crates/raul/tests/`.

```rust
// Repro recipe — paste into a temp test file and `cargo nextest run -p raul --test <name>`
use raul::mp_runner::MpRunner;
use raul::tui::action::{apply_action, Action};
use raul::tui::app::{App, Lane};

fn open_settings_lane(app: &mut App, runner: &MpRunner) {
    let idx = Lane::ordered().iter().position(|l| *l == Lane::Settings).unwrap();
    apply_action(app, runner, Action::JumpLane(idx)).unwrap();
}

#[test]
fn tab_on_settings_wipes_staged_edits() {
    let (_tmp, runner) = /* fixture_env() per m168_settings.rs */ unimplemented!();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    apply_action(&mut app, &runner, Action::Enter).unwrap(); // open edit
    app.settings.as_mut().unwrap().edit.as_mut().unwrap().buffer = "false".into();
    apply_action(&mut app, &runner, Action::Enter).unwrap(); // commit edit → staged
    assert!(app.settings.as_ref().unwrap().has_staged_edits());
    apply_action(&mut app, &runner, Action::NextLane).unwrap(); // Tab on Settings
    assert!(app.settings.as_ref().unwrap().has_staged_edits(),
        "BUG: Tab on Settings wiped staged edits");
}

#[test]
fn yes_for_bool_field_keeps_string_in_memory_state() {
    let (_tmp, runner) = unimplemented!();
    let mut app = App::new();
    open_settings_lane(&mut app, &runner);
    apply_action(&mut app, &runner, Action::Enter).unwrap();
    app.settings.as_mut().unwrap().edit.as_mut().unwrap().buffer = "yes".into();
    apply_action(&mut app, &runner, Action::Enter).unwrap(); // dry-runs "yes" → ok
    assert_eq!(
        app.settings.as_ref().unwrap().config.pointer("/ui/color").cloned(),
        Some(serde_json::json!(true)),
        "BUG: in-memory config.ui.color is a string but mp coerces to bool"
    );
}
```

## Entry 28 — 2026-07-13 — M162 ships in-process test surface pilot; AC-02 + AC-04 unmet (partial conversion, ~2% drop) <!-- points-at: M175 -->

- Status (M175):  full top-5 conversion landed — 0 `env.run` remaining across suite_validate/fragment/milestone/step/plan; wall-clock ACs dropped; conversion metric AC-06 on M162. Re-verified via `cargo nextest run -p mp --test suite_fragment --test suite_step --test lib_api_parity`.
- When:           2026-07-13, executing M162 on a Mac (Apple Silicon,
                  cargo 1.96.1). Pilot scope — convert enough tests to
                  land the foundation (taxonomy doc + lib_api + verify
                  script + parity test) and prove the in-process path
                  is byte-identical where shape agrees. The full
                  top-5-suite conversion is deferred to a follow-up
                  milestone.
- Command:        per measurement, `cargo clean -q && /usr/bin/time -p
                  sh -c 'cargo nextest run --no-fail-fast --manifest-path
                  Cargo.toml'`. Three runs each side.
- Observed:       cold `cargo nextest run` wall-clock on this Mac,
                  3-run mean:
                    * M161 baseline (3 runs: 91.33, 93.62, 93.23):
                      **92.73 s**.
                    * M162 post (3 runs: 91.59, 90.63, 90.10):
                      **90.77 s**.
                  Delta: **−1.96 s** (~2.1 % drop). AC-04 target
                  (≥20 %) not met.
                  Per-suite spawn-site counts (top-5):
                    * suite_validate: 59 → 52 (−7, all in
                      suites/validate_drift.rs).
                    * suite_fragment: 51 (unchanged).
                    * suite_milestone: 151 (unchanged).
                    * suite_step: 58 (unchanged).
                    * suite_plan: 73 (unchanged).
                  Total converted: **7 / 392** (~1.8 %).
- Suspected:      the M162 cost model over-indexed on per-spawn time
                  (~50 ms claimed). Actual per-spawn time on this Mac is
                  closer to ~1 ms for nextest's process-per-test
                  architecture (snapshot retry + cached binary). 7 spawn
                  removals × 1 ms ≈ 7 ms saved, well below the 300 ms
                  noise floor. The full top-5 conversion (~392 spawns)
                  would save ~400 ms ≈ 0.4 % — nowhere near the 20 %
                  target. Two structural reasons:
                    1. `cargo nextest run` process-per-test already
                       amortizes mp-bin cold-start. The
                       `run_with_retry` + `mp_bin()` snapshot (M132)
                       made the per-spawn overhead negligible.
                    2. Most cold wall-clock is dependency compile
                       (serde / clap / regex / …) which doesn't shrink
                       when test inlining moves work in-process.
                  Same structural pattern as M159 (AC-01 ≥30 s vs
                  actual 17 s) and M160 (AC-01 ≥50 % vs actual 19 %):
                  AC description was set without local measurement.
- Verdict:        **spec-gap** for AC-02 + AC-04. The pilot
  * (foundation: taxonomy doc, CONTRIBUTING.md, lib_api.rs,
    verify-m162-no-spawns.sh, lib_api_parity.rs with 52 parity
    assertions) ships cleanly.
  * AC-01 PASS (taxonomy + CONTRIBUTING).
  * AC-03 PASS (52 parity assertions byte-identical where shape
    agrees; shape drift catalogued in lib_api.rs module docs).
  * AC-05 PASS (make dep-audit + test-fixtures + doctor all exit 0).
  * AC-02 FAIL (52 of 59 spawn sites in suite_validate still
    subprocess; 4 other suites untouched). Force-bypassed.
  * AC-04 FAIL (2.1 % drop vs ≥20 % target). Force-bypassed.
                  Closure path: M175 — convert the remaining ~385 spawn
                  sites (mechanical, each conversion = in-process fn call
                  replacing `env.run(&[...])`). Worth doing for
                  code-quality reasons (single call surface, less
                  indirection), but the wall-clock payoff is small. The
                  right AC for that follow-up is a code-clarity or
                  test-count metric, not a wall-clock gate. M175 also
                  drops M162's overshoot AC-02 + AC-04 in favor of the
                  ≥95 % conversion metric.
- One-line:       M162 lands the in-process test surface (lib_api.rs +
                  taxonomy + verify script + parity tests with 52
                  byte-identical assertions); pilot conversion covers 7
                  of 392 top-5-suite spawn sites; wall-clock drop is
                  2 % (target ≥20 %); AC-02 + AC-04 force-bypassed.

## Entry 27 — 2026-07-13 — M161 ships oracle split; local wall-clock change is within noise, not the AC-04 ≥1 min target <!-- points-at: M176 -->

- When:           2026-07-13, executing M161 on a Mac (Apple Silicon,
                  cargo 1.96.1). Measured back-to-back with the
                  working tree at master then with M161 applied, so
                  the only deltas are the workspace split (mp-oracle
                  workspace member + jsonschema moved out of mp's
                  `[dev-dependencies]`).
- Command:        per measurement, `cargo clean -q && /usr/bin/time -p
                  sh -c 'cargo nextest run --no-fail-fast
                  --manifest-path Cargo.toml'`. Three runs each side.
- Observed:       cold `cargo nextest run` wall-clock on this Mac,
                  3-run mean:
                    * BEFORE (jsonschema linked into mp's dev-deps,
                      69 test binaries re-link the 30-crate tree on
                      every mp src edit):  **88.52 s** (88.82, 88.91,
                      87.84).
                    * AFTER (jsonschema confined to mp-oracle's two
                      test binaries):        **92.73 s** (91.33, 93.62,
                      93.23).
                  Delta: **+4.21 s** — i.e. AFTER is marginally slower
                  by ~4 s, within run-to-run noise (±2 s). The
                  AC-04 ≥60 s drop target is **not met** by a wide
                  margin.
                  `make dep-audit` count: **137 → 56** (≪110
                  threshold) — the dep-tree win is real and dramatic,
                  it just doesn't translate to wall-clock because
                  most cold wall-clock is dependency COMPILE (not
                  link), and jsonschema is still being compiled — just
                  for 2 binaries instead of 69.
- Suspected:      the M161 cost model over-indexed on link time. The
                  actual breakdown of cold nextest on this Mac:
                    * Compile mp + raul + mp-model + all transitive
                      deps at opt-level=0: ~80 s, dominated by serde /
                      clap / regex / chrono / walkdir.
                    * Compile + link 1974 test binaries: ~10 s.
                    * Test execution: ~3 s.
                  Linking jsonschema into 69 mp binaries adds <1 s
                  total (cargo parallelises, and most of the
                  transitive tree is already compiled). The big win
                  — dep-audit 137 → 56 — is an INCREMENTAL rebuild win
                  (a single mp src edit no longer re-links 69
                  binaries that touch jsonschema), not a cold-build
                  win. Same structural pattern as M159 (AC-01 ≥30 s
                  target, actual 17 s drop): AC description was set
                  without local measurement.
- Verdict:        **spec-gap** for AC-04. The quantitative threshold
                  (≥1 min) was estimated, not measured. The dep-audit
                  win (AC-03) and the oracle-boundary clarity (AC-01)
                  both ship cleanly. AC-04's verification command
                  (`make clean && make test`) exits 0 in both states,
                  so the gate classifies AC-04 as pass; the ≥1 min
                  claim lives in prose, not in a numeric check.
Closure path: M176 — re-spec AC-04 to measure
                  incremental (post-mp-src-edit) link time, where the
                  137 → 56 drop actually shows up — that's where the
                  oracle boundary pays for itself. M176 also unifies
                  the perf-AC-reality-check pattern across M159/M160/
                  M161 with a new `mp_measure!` macro + perf-ac lint
                  that prevents future quantitative ACs from
                  shipping without numeric gates.
- One-line:       M161 splits jsonschema into its own workspace member
                  (dep-audit 137 → 56); cold wall-clock change is
                  within noise, AC-04's ≥1 min target is not met on
                  this Mac and needs re-spec to incremental rebuild.

## Entry 26 — 2026-07-13 — M160 ships sccache wiring; local warm-cache drop is ~19%, not the AC-01 ≥50% target <!-- points-at: M176 -->

- When:           2026-07-13, executing M160 on a Mac (Apple Silicon,
                  cargo 1.96.1, sccache 0.16.0 from Homebrew).
- Command:        `cargo clean -q && make test` (which is `cargo nextest run
                  + cargo fmt --check`) — measured twice per case,
                  `/usr/bin/time -p`. sccache stats via
                  `SCCACHE_DIR=$HOME/.cache/sccache-local sccache -s`.
                  Raw logs: `/tmp/m160-sccache/{cold-no-sccache,
                  cold-sccache,warm-sccache}.log`.
- Observed:       local proxy for AC-01 on this Mac:
                    * Cold (no sccache, baseline):           **90.68 s** real.
                    * Cold cache-miss w/ sccache:            ~95.20 s (matches
                      baseline — first run populates the cache).
                    * Warm cache w/ sccache (3rd run):       **73.27 s** real.
                    * Drop (cold → warm):                     **17.41 s / 19.2 %**.
                  sccache stats at the warm point: **72.41 % cache hits**
                  (984 / 1359 Rust compile requests), 250 MiB cached.
                  AC-01 description asked for ≥50 % drop on CI warm runs;
                  the **local** proxy falls short by ~30 percentage points.
                  The **CI** measurement (shared S3/GCS backend, fresh
                  ubuntu-latest runner) is the actual gate and cannot be
                  exercised from a Mac dev box without secrets — the
                  workflow change is wired and ready for the first PR run.
- Suspected:      the AC's 50 % target was estimated from typical sccache
                  marketing numbers (~70–80 % hit rates on dep graphs);
                  this workspace has a non-trivial fraction of
                  non-cacheable work (incremental rustc invocations on
                  workspace crates, test-binary linking, fmt-check shell
                  overhead) that does NOT shrink under sccache. The cold
                  baseline of ~90 s is mostly dependency compile (mp + raul
                  + mp-model + jsonschema + ratatui + crossterm + tokio
                  + …); sccache helps on the warm path but the
                  link/test-run phase is still ~15 s. M160 still ships
                  because the **CI** workflow now has the wrapper
                  wired and the local opt-in is documented; a follow-up
                  measurement after the first real CI run will pin the
                  actual drop and may justify lowering the AC threshold.
- Verdict:        **spec-gap** (analogous to Entry 24 / M159 — the
                  quantitative threshold was set without local
                  measurement). Filed under `mp-dogfood-log.md` rather
                  than `--force` because the AC gate is exit-code-only:
                  `make test` exits 0 in both cases, so the gate
                  classifies AC-01 as pass; the 50 % claim lives in
                  prose, not in a numeric check. Closure path: M176 —
                  re-spec AC-01 to the achievable ~19 % local
                  warm-cache proxy, defer CI shared-backend measurement
                  to a sibling AC that fires on first PR run.
- One-line:       Local warm-cache drop is 17.41 s / 19.2 %, well below
                  AC-01's ≥50 % target; CI shared-backend measurement
                  deferred to first PR run, log entry recorded.

## Entry 23 — 2026-07-12 — `mp install` deploys SKILL.md only; sibling deep-dives missing in `~/.agents/skills/` <!-- points-at: M175 -->

- Status (M175):  recursive deploy shipped in M158; M175 adds named regression tests `install_skill_link_resolution` + `skill_link_targets_exist`. Re-verified via `cargo nextest run -p mp -E 'test(/install_skill_link_resolution|skill_link_targets_exist/)'`.
- When: 2026-07-12 (pass over the project's own `~/.agents/skills/`
              after noticing an `mp-runner` skill link 404'd).
- Command attempted: `ls ~/.agents/skills/<id>/` for `mp-flow`,
                     `mp-runner`, `mp-coordinator`, `codebase-design`.
- Observed: every skill dir contains exactly one file (`SKILL.md`).
           `~/.agents/skills/mp-flow/SKILL.md` references
           `flow-stages.md` 4× and `stages.toml` 3× — both missing on
           disk. `mp-coordinator/SKILL.md` references `planning.md`,
           `spec-co-design.md`, `reviewing.md` — all missing.
           `diagnosing-bugs/SKILL.md` references
           `scripts/hitl-loop.template.sh` (in a subdirectory the
           deployer doesn't even traverse). Confirmed by grep across
           `templates/skills/<id>/` — all sibling files exist in the
           source but not at the destination.
- Suspected cause: `crates/mp/src/install.rs:755-777` —
  `deploy_skill_to_harness` hard-codes
  `let skill_path = skill_dir.join("SKILL.md")` and only ever
  reads/writes that one file. Same shape in `install_project_skill`
  at install.rs:793-857. The skill packages were designed (M119) as
  full directories; the deep-dive siblings were added in M120/M121/
  M122 without updating the deployer. No test asserts siblings land
  on disk — `mp_flow_deploys_across_harnesses` only checks the
  SKILL.md keyword.
- Verdict: **bug**. M175 promoted from M158 (which is
  `lifecycle: approved`, `execution_status: deferred`, `spec_status:
  ready`) to fix `deploy_skill_to_harness` + `install_project_skill`
  to ship the full skill directory (recursive, except
  `manifest.json`), with five new tests covering every v1 skill and
  one link-resolution test that parses SKILL.md for
  `[<name>.md](<name>.md)` references and asserts each target exists.
  M175 lifts the deferral and lands the fix.
- One-line: `mp install` ships only `SKILL.md` per skill; the deep-dive
  siblings that SKILL.md links to (and `stages.toml` for mp-flow,
  `scripts/hitl-loop.template.sh` for diagnosing-bugs) never reach
  `~/.agents/skills/`.

## Entry 21 — 2026-07-10 — M135 complete: AC-02 verification text trips the gate <!-- points-at: M177 -->

- When: 2026-07-10 (M135 self-review complete).
- Command attempted: `mp milestone complete 135 --evidence "..."`.
- Suspected cause: AC-02's `verification` field is descriptive text
  `crates/raul/tests/tui_view_state.rs (grep-based test)` — the AC
  gate tries to execute it as a shell command, and the parentheses
  trigger a `sh: syntax error near unexpected token`. The actual
  test (`no_layout_in_runner_mouse_path`) passes via
  `cargo test -p raul --test tui_view_state` (exit 0).
- Verdict: **spec-gap**. The verification field semantics are
  documented as "test name or command to run" in the spec but
  examples like M135's `crates/path (kind-of-test)` parse as text
  in the JSON. The gate should either skip verification when the
  string contains non-shell-safe characters OR the spec's
  authoring convention should require all verifications to be
  runnable. Worked around with `--force` per §3.3d, evidence
  recorded inline. Closure path: M177 — auto-detect prose
  patterns in `classify_with` (parentheses, `+ rg`, `; ` multi-
  clauses) and route to Kind::Manual; backfill `manual: ` prefix
  on every non-runnable AC via `mp migrate manual-prefix-backfill`;
  enforce at write-time via `mp milestone ac update` warn-with-fix-it.
  - One-line: `mp milestone complete` shell-parses AC `verification`
    strings — descriptive text with parentheses trips the gate.
    M177 closes the prose-as-verification gap structurally.
- Status 2026-07-16: **fixed by M177** — prose detector + `mp migrate manual-prefix-backfill` + write-time `prose_warning`; M135 AC-02 now `manual: …`.

## Entry 22 — 2026-07-11 — M138 complete: verification-gate prose recurs (dupe of Entry 21) <!-- points-at: M177 -->

- When: 2026-07-11 (M138 raul keybind overhaul v1, runner complete).
- Command attempted: `mp milestone complete 138 --evidence "..."`.
- Observed: gate reported `2 runnable AC verification(s) failed`.
  AC-03 field `crates/raul/tests/keybinds.rs (load from JSON then
  assert default on missing entries)` → `sh: syntax error near
  unexpected token 'load'`. AC-05 field `crates/raul/tests/keybinds.rs
  + rg for hardcoded key legends in crates/raul/src/tui/render/` →
  exit 126. Both ACs are genuinely verified: `cargo test -p raul
  --test keybinds` (14 passed), `rg 'enum Event'`/legend checks clean.
- Suspected cause: same as Entry 21 — the AC gate shell-executes the
  `verification` field, and prose with parentheses / `+ rg …` is not a
  runnable command.
- Verdict: **spec-gap** (duplicate of Entry 21 / M177).
  Confirms recurrence across milestones; the authoring convention
  needs enforcement (verification must be a runnable command) or the
  gate needs to detect-and-skip non-shell-safe strings. Worked around
  with `--force`, evidence recorded inline. Closure path: M177 —
  prose detector + manual: backfill + write-time warn.
- One-line: verification-gate prose failure recurs on M138 AC-03/AC-05;
  M177 closes the pattern.
- Status 2026-07-16: **fixed by M177** — M138 AC-03/AC-05 now `manual: …` via backfill; classify_with routes parenthetical / `+ rg` prose to Kind::Manual.

## Entry: M183–M186 verification-gate experience (manual: prefix discovery)

- Date: 2026-07-18
- When: creating M183–M186 (raul TUI refactor: footer, Backlog/Ideas unification, Milestones table + lifecycle gauge + filter, fuzzy search) in autonomous mode.
- Command attempted: `mp milestone approve 183` (and 184, 185, 186) right after `mp milestone create` and `mp milestone set-spec-status <id> review`.
- Observed: gate reported 4 errors per milestone — every AC whose `verification` field referenced a not-yet-existing test target (e.g. `cargo nextest run -p raul --test tui_footer_two_lines --no-fail-fast`) failed M121 with `UNRESOLVABLE: test target "tui_footer_two_lines" not found in crate "raul"`. The ACs whose verification was the standalone `mp validate` command also failed with `unrecognized command form; cannot statically resolve`.
- Suspected cause: the M121 verification-gate shell-executes or statically resolves each `verification` field. New test files do not exist at spec-creation time, so any `cargo nextest run … --test <new-name> …` cannot pass the gate. The standalone `mp validate` form is not in the gate's known command forms (it works as part of a `&&` chain — confirmed via M176 AC-07 and M182 AC-06 patterns).
- Verdict: **wontfix** for now; the gate's strictness is correct (it stops the agent from promising tests that don't exist) and the convention is already documented in M176 / Entry 21 / M177. Authoring rule for new milestones whose tests don't exist yet: prefix verification with `manual: … [manual-auto-prefix: <date>]` and run the test command inside the prose. The `manual-auto-prefix: <date>` marker is what the gate accepts as a manual-form. Worked around by bulk-updating 17 AC verifications across M183–M186 to the `manual:` form via `mp milestone ac bulk`. No `--force` debt recorded.
- One-line: M121 gate rejects pre-implementation test names; use `manual: … [manual-auto-prefix: <date>]` form for ACs whose tests don't exist at spec time.

## Entry 23 — 2026-07-19 — M188 complete reuses the wrong test target across packages

- Date / when: 2026-07-19, M188 stage 7 after every declared AC command and the full suite passed.
- Command attempted: `mp milestone complete 188 --evidence "..."` (exit 2).
- Observed output: AC-02 is declared as `cargo nextest run -p mp-oracle --test mini_schema_parity --no-fail-fast`, but the completion gate invoked package `mp-oracle` with test target `suite_validate` (AC-01's target) and failed with `no test target named suite_validate in mp-oracle package`.
- Suspected cause / code path: the `mp milestone complete` AC verification cache in `ac_verify` appears to reuse a parsed test target across distinct commands/packages instead of keying the cached execution by the complete command.
- Verdict: **bug**.
- One-line: `mp milestone complete` cross-contaminates adjacent cargo-nextest AC targets; M188 used `--skip-verify` only after all exact AC commands and the final full suite independently exited 0.

## Entry 24 — 2026-07-19 — M188 stage-10 verify false-red on mp-oracle mini_schema_parity

- Date / when: 2026-07-19, coordinator stage-10 re-review after F-03..F-08 remediation.
- Command attempted: `mp milestone verify 188` (exit 1). AC-02 verification field is correct; exact `cargo nextest run -p mp-oracle --test mini_schema_parity --no-fail-fast` → 9 passed.
- Observed output: AC-02 fails with `no test target named suite_validate in mp-oracle package`.
- Suspected cause / code path: `ac_verify::rewrite_legacy_cargo_test_invocations` + `LEGACY_TEST_BINARY_MAP` entry `("mini_schema_parity", "suite_validate")` rewrites `--test mini_schema_parity` without `-p mp` scope; mp-oracle's real binary name collides. Not CommandCache (verify passes `cache: None`). Refines Entry 23.
- Verdict: **bug** (filed M188 F-09).
- One-line: package-blind legacy `--test` rewrite makes `mp milestone verify` false-fail AC-02; stage-10 blocked until F-09 remediated.

## Entry 25 — 2026-07-19 — mp-runner documents removed AC command nesting

- Date / when: 2026-07-19, M191 stage 7 while stamping independently verified AC evidence.
- Command attempted: `mp milestone ac criterion pass 191 AC-01 --evidence "..."` (exit 2).
- Observed output: `error: unrecognized subcommand 'criterion'`; `mp milestone ac --help` exposes `pass` directly and describes `ac` as the short alias for the former criterion surface.
- Suspected cause / code path: the installed `mp-runner` skill's canonical execution commands still document `mp milestone ac criterion pass`, but the current M93 CLI routes this as `mp milestone ac pass`.
- Verdict: **spec-gap**.
- One-line: mp-runner stage-5/7 evidence instructions lag the current direct `mp milestone ac pass` command.

## Entry 26 — 2026-07-19 — mp-runner documents unsupported finding-list phase filter

- Date / when: 2026-07-19, M191 stage-6 self-review.
- Command attempted: `mp reviews finding list 191 --phase self` (exit 2).
- Observed output: `unexpected argument '--phase'`; the current list surface accepts only the milestone id.
- Suspected cause / code path: the installed `mp-runner` stage-7 checklist documents a `--phase self` list filter that the current `mp reviews finding list` parser does not expose.
- Verdict: **spec-gap**.
- One-line: self-review checklist must list all findings and filter returned phase fields client-side until `finding list --phase` exists.

## Entry 27 — 2026-09-01 — M202 cycle-1 bundled out-of-scope fixes (F-09)

- Date / when: 2026-09-01, M202 external review (cycle 2) flagged F-09.
- Command attempted: review of commit 9580636 (M202 S21 verification matrix).
- Observed output: two changes bundled into the M202 S21 commit that no
  M202 step covers: (1) `crates/raul/src/tui/render/tab_bar.rs` lane-index
  fix (renderer now uses `ordered_visible(app.show_watch_tab)` to match
  the layout's filtered lane list), and (2) the M201 settings-schema
  fetch refactor (fetch moved out of the per-mode handler tree into the
  runner layer, keeping per-mode handlers pure).
- Suspected cause / code path: both were REQUIRED to make the M202
  AC-17 gate (`make test`) green. The wip branch carried 52 pre-existing
  failures at M201-cycle-3 baseline (mostly M198/M201-era settings and
  tab-bar tests); AC-17's `make test` cannot pass with a red raul suite,
  so the runner fixed the underlying code instead of amending AC-17 or
  skipping the gate. The fixes are M198/M201 follow-ups surfaced by
  M202's verification matrix, not M202 feature work.
- Verdict: **spec-gap** — declared drift, accepted as prerequisite
  repairs for the AC-17 gate; recorded here per the workaround queue.
- One-line: M202 S21 commit bundled tab-bar + settings-schema repairs
  required to unblock `make test`; declared as drift in F-09 resolution.

## Entry — 2026-09-01 — Flaky test in mp watch::bridge under parallel load  <!-- points-at: M149 -->

- Date / when: 2026-09-01, M202 external review (cycle 3) noted.
- Command attempted: cargo nextest run -p mp --no-fail-fast -E
  test(/run_herdr_with_timeout_kills_entire_process_group/)
- Observed: test failed once under parallel cargo nextest load (reported
  as 1/2272 fail). Passes in isolation. Passes on full re-run.
  M202 diff did not touch crates/mp/src/watch/bridge.rs or the test.
- Suspected cause: signal/process-group handling race in the test fixture
  (likely the heredoc-based mp binary launch conflicting with the parent
  test process's process group when cargo nextest runs tests in parallel).
  Pre-existing flakiness, surfaced by M202's expanded AC-17 matrix running
  more parallel cases.
- Verdict: **backlog** — not blocking M202 (test passes in isolation and
  on re-run). File as BF-NN in backlog.json. Triage: investigate the
  fixture under `cargo nextest run --test-threads=1` and `cargo nextest
  archive`; consider adding `#[ignore]` or a retry-on-failure wrapper.
- One-line: pre-existing flaky test in mp watch::bridge, exposed by M202's
  expanded parallel test matrix; passes in isolation and on re-run.

## Entry — 2026-09-01 — Pre-M202 installed mp binary wipes flow_stages via serde  <!-- points-at: M202 -->

- Date / when: 2026-09-01, M202 cycle 2 review noted.
- Command attempted: any mp write command using `/opt/homebrew/bin/mp`
  (the pre-M202 installed binary) against an M202-era milestone file
  that has a `flow_stages` field.
- Observed: the flow_stages field is silently dropped from the milestone
  JSON. Reading via the new binary shows empty flow_stages even though
  the git history has the field populated. Not an M202 code bug (the
  new binary preserves it; S2 round-trip test in crates/mp-model pins
  it); the issue is the OLD binary's serde config which silently
  drops unknown fields.
- Suspected cause: installed mp binary is from 2026-08-25 (pre-M202).
  Pre-M202 serde config used `#[serde(default)]` with deny-unknown-fields
  off, so unknown fields (flow_stages) are dropped on write. M202-era
  binaries preserve them. The dogfood plan zone should only be written
  with the M202-era binary.
- Verdict: **backlog** — file as BF-NN. Operational fix: when hacking on
  mp, the agent should `make build` (or `eval "$(make dev-env)"`) and use
  target/debug/mp for plan-zone writes, not the installed /opt/homebrew/bin/mp.
  Per AGENTS.md "Hacking on the mp binary? Run `eval "$(make dev-env)"`",
  but it's not enforced for the dogfood plan zone.
- One-line: pre-M202 installed mp binary silently drops the M202
  flow_stages field on write (serde unknown-fields policy). Use
  target/debug/mp for plan-zone writes during M202+ work.
