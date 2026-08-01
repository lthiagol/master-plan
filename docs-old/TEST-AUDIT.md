# Test Inventory & Audit

**Authoring context:** produced during M106 (test infrastructure refactor) as
S10/S11 deliverable. Captures the current state of the test corpus *as of
2026-07-04* with keep/investigate/drop classifications. The next dogfood pass
should review this and convert "investigate" items into backlog tasks or
greenlit drops; M106 itself does not delete or rewrite any existing test.

## Headline numbers

| Bucket | Count | Net verdict |
|--------|-------|-------------|
| `crates/mp/tests/*.rs` integration files | 144 | KEEP, with 3 investigate flags |
| `crates/raul/tests/*.rs` integration files | 46 | KEEP, with 1 pre-existing-failure flag |
| Library unit tests (inside `src/*.rs`) | a few hundred | KEEP, theme-by-theme coverage |
| `tests/scenarios/` golden scenarios | ~16 across 16 dirs | KEEP |
| `tests/fixtures/projects/<name>/master-plan/` | 9 fixtures + 1 testing-audit fixture | KEEP, with 2 overlap flags |

## Recently added coverage (M104 / M106)

Already shipped, recent, and the regression net for the rc-blocker sweep +
testing refactor:

| Test | Covers | Defends against |
|------|--------|-----------------|
| `crates/mp/tests/sort_regression.rs` | M104 AC-01 | Lex-id sort at milestone id >= 100 |
| `crates/mp/tests/lifecycle_gates_post_migration.rs` | M104 AC-02 | Gate reads becoming dead branches after legacy-shape migration |
| `crates/mp/tests/ac_verify_pipe_deadlock.rs` | M106 AC-01 | Pipe-buffer deadlock in `ac_verify::execute` |
| `crates/mp/tests/ac_verify_shapes.rs` | M106 AC-02 | Envelope JSON wire-format drift across the unified-runner refactor |
| `crates/mp/tests/verify_lint_scope.rs` | M106 AC-03 / M110 S2 | `mp plan verify-lint` scope-discipline regression |
| `crates/mp/tests/verify_lint_broad_scope.rs` | M110 AC-02 | Per-milestone affected-crate derivation |
| `crates/mp/tests/verify_lint_portability.rs` | M110 AC-03 | macOS-portability pattern WARNs |
| `crates/mp/tests/milestone_complete_gate_caching.rs` | M110 AC-01 | Gate command cache within one `mp milestone complete` |
| `tests/fixtures/projects/sort_regression/` | M104 AC-01 fixture | 6 milestones spanning the lex/numeric boundary at 100 |
| `tests/fixtures/projects/verify-lint-broad/` | M110 scenario | Synthetic broad-scope milestone for golden scenario |
| `tests/fixtures/ac_verify_shapes.json` | M106 AC-02 golden | Field-set stability for AcResult / StepTestsResult |
| `mp plan verify-lint` (Rust) | M110 AC-02/03 | Soft WARN-only scope + portability scanner (`make verify-lint`) |

## Themed tour of the existing corpus

### `crates/mp/tests/ac_verify_*` (4 files)

Small, focused, exactly the right shape.

- `ac_verify.rs` — manual unit tests for the runner machinery (classify, command_for_execution).
- `ac_verify_timeout.rs` — hung commands time out within bounded wall clock (uses `sleep 30` + lowered `MP_VERIFY_TIMEOUT_SECS=2`).
- `ac_verify_pipe_deadlock.rs` — >64KB producer does not deadlock (M106/S3).
- `ac_verify_shapes.rs` — golden snapshot for envelope JSON (M106/S6).

Verdict: **KEEP all**. The four files split along clean seams (unit, timeout,
regression, shape stability) and the bumps M106 added are deliberate.

### `crates/mp/tests/fragment_*` (~9 files)

The M93 agent-friendly fragment-first reads/writes path:
`ac`/`ac_id` selectors, `step_outline_id_selectors`, `step_*_read`/`write`/`update_guard`,
`wp_write`, etc.

Verdict: **KEEP all**. Each file is small and the M93 surface is explicitly
tested by name (`mp milestone ac show` reads, `mp milestone step update`
guards, etc.). Removing any one would erode coverage of the M93 contract.

### `crates/mp/tests/init_*` (~6 files)

Bootstrap tests: `init_empty_collections`, `init_from_repo_markdown`,
`init_json`, `init_plan_dir`, `init_root_agents`, `init_transactional`.

Verdict: **KEEP all**. Each covers a distinct `mp init` flow; some have dead-code
warnings (unused imports) that should be cleaned up — see "Investigate" below.

### `crates/mp/tests/p1* … p12*` (~9 files)

Phase-prefixed batch tests (`p1.8` through `p12`). They cover the per-phase
contracts (e.g., `p11_sync_split.rs`, `p6_install_bootstrap.rs`,
`p9_backlog_promote.rs`).

Verdict: **KEEP**. These are dated feature gates; the prefix is the spec
phase. Cross-consolidation with other tests is *not* obvious because each
phase has its own invariants.

INVESTIGATE (deferred):
- `p6_install_bootstrap.rs` — has an `isolated_harness_env` unused-import
  warning (dead code; trim in a follow-up).
- `p7_optional_batch.rs` — has a `std::fs` unused-import warning.

### `crates/mp/tests/validate_*` (~4 files)

Validation tests: `validate_drift`, `validate_fixture`, `validate_readiness`,
`verify_field_validate`. These cover gate behavior (`G3`, `G4`, `G6`, `G7`,
`W40–W43` etc.) and the per-AC `verification` field semantics.

Verdict: **KEEP all**. **Two of these** (`validate_fixture.rs` and
`make test-fixtures`) are the primary defense against gate regressions;
the others cover edge cases in field validation.

### `crates/mp/tests/track_*` (~5 files)

Track lifecycle/listing tests, including `tracks.rs`, `track_lifecycle.rs`,
`track_listing.rs`, `track_json_parity.rs`.

Verdict: **KEEP all**. The track/track-item contract is a multi-crate
invariant; collapsing tests risks hiding one branch.

### `crates/mp/tests/lifecycle_*` (~2 files)

`lifecycle_migration.rs` and `lifecycle_gates_post_migration.rs`
(M106/sort/lifecycle work). Both covered above in the headline section.

Verdict: **KEEP**.

### `crates/mp/tests/agent_*` (~3 files)

Agent discovery/projection filters (`agent_filters.rs`, `agent_projection.rs`,
`agent_summary.rs`).

Verdict: **KEEP**. These gate the M80/M93 agent-discovery surface that downstream
harnesses consume.

### `crates/mp/tests/inbox*`, `code_review_gate.rs`, `state_tracking.rs`, `status_readiness.rs`

Lighter-weight surface tests. Each is short and tightly scoped.

Verdict: **KEEP**. None of these are large enough to be a maintenance burden.

### `crates/mp/tests/milestone_*` (~10 files)

`milestone_archive.rs`, `milestone_bulk.rs`, `milestone_create.rs`,
`milestone_create_example.rs`, `milestone_create_stdin.rs`,
`milestone_from_handoff.rs`, `milestone_priority.rs`, `milestone_trace.rs`,
`milestone_update_conflict.rs`, `milestone_verify.rs`.

Verdict: **KEEP all**, but flag for review:
- `milestone_create.rs` and `milestone_create_stdin.rs` and
  `milestone_create_example.rs` overlap on what they test (create via
  inline / stdin / example). The split predates M106; consolidating into
  one parameterized test would clean up.

INVESTIGATE (deferred): consolidate the three create tests.

### `crates/mp/tests/concurrency.rs`, `install_*`, `workflow_gates.rs`

Edge-case tests around plan-json concurrent writes, install flows, and
workflow metadata.

Verdict: **KEEP**. The concurrency.rs test is the existing "no concurrent
`mp milestone wp|step add`" regression net; it's noisy but valuable.

### `crates/raul/tests/*` (46 files)

TUI + CLI tests for `raul`. Each file is small (<200 lines usually).

INVESTIGATE (pre-existing failure):
- `crates/raul/tests/explain_impact.rs::raul_explain_impact_runs` — fails
  on `master` as of M104 work. The fixture's milestone file lacks `spec_status`
  field after `mp milestone create` migration; out of scope for M106.
  Recommend a follow-up milestone.

KEEP all other 45 files. TUI tests have a stable layout, no current flake.

### `tests/scenarios/`

Golden CLI scenarios driven by `cargo test --test scenarios_runner`. Each
scenario is a self-contained `tests/scenarios/<id>/main.sh` (or similar)
that runs the binary against a staged plan.

Verdict: **KEEP all**. These are the integration smoke net — most useful when
verifying behavior across the whole CLI surface.

### `tests/fixtures/projects/<name>/`

Hand-crafted plan fixtures, used by `make test-fixtures`.

| Fixture | Role |
|--------|------|
| `brownfield-api` | brownfield delta milestone example |
| `gate-g1-fail` | G1-violating milestone for gate tests |
| `hybrid-work` | `.mp/` hybrid layout |
| `linear-deps` | dependency chain |
| `m43-annotation-fixture` | annotation/approval flow |
| `minimal-ready` | smallest passing plan |
| `sort_regression/` | M104 sort regression |
| `walkthrough-oauth` | full-spectrum demo plan |
| `write-blank/`, `write-post-step-add/` | write-flow tests |

Verdict: **KEEP all**.

INVESTIGATE (deferred, redundancy):
- `tests/scenarios/p0-validate-g1-fail/`, `p0-validate-ok/`, `p0-status-minimal/`
  may overlap with `tests/fixtures/projects/{gate-g1-fail,minimal-ready}/`.
  Consolidating duplicates is a future-pass decision; both currently pass.
- `tests/scenarios/walkthrough-validate-ok/` overlaps with
  `tests/fixtures/projects/walkthrough-oauth/`. Same note.

## Investigate (deferred to a follow-up dogfood pass)

These were observed during the M106 audit. **M106 does not act on them;**
they're tagged for the next pass to either fix, drop, or document as known-noise.

1. **Pre-existing `raul` failure:** `crates/raul/tests/explain_impact.rs::raul_explain_impact_runs` fails on `master` (independent of M106). The `mp milestone create` flow now writes milestones without `spec_status` field; a migration-path test or a schema-allowance fix is needed.
2. **Three create-path tests** (`milestone_create.rs`, `milestone_create_stdin.rs`, `milestone_create_example.rs`) have overlapping coverage of `mp milestone create`. Consider a single parameterized test.
3. **`p6_install_bootstrap.rs` and `p7_optional_batch.rs`** have dead-code warnings (unused imports). Cosmetic but visible in test output.
4. **Historical broad-scope AC verification strings** flagged by `make verify-lint`:
   - M42 (lines 15, 29)
   - M99 (lines 52, 80)
   - M105 (post-rc polish, line 75)
   - (M104 itself is now clean after M106 work.)
   These are not in active execution but the lint surfaces them for opt-in
   cleanup; future passes can resolve.
5. **Scenario/fixture overlap** (see test-fixtures section above) — both pass
   today; future consolidation is editorial, not bug-fix.

## Drop (deferred to a follow-up dogfood pass)

**None from M106's audit pass.** All classified tests had a defensible
keep/investigate rationale; bulk deletion was deliberately *not* in scope.

## Coverage assessment (post-M106)

- ✅ Sort regression at milestone id >= 100
- ✅ Lifecycle migration parity (gate parity for VALID legacy shape + normalize behavior)
- ✅ ac_verify pipe-buffer deadlock (the M104 blocker root cause)
- ✅ ac_verify envelope shape stability across the WP2 unification
- ✅ Scope-discipline soft lint (lives in Makefile + script + 2-test regression)
- ✅ Step-testing unification (run_one shared between AC + step verifiers; goldens lock the shape)

## What this audit does NOT cover

- Performance benchmarks (none ship today; left for a future milestone).
- TUI accessibility or visual regression tests.
- Cross-crate `mp` ↔ `raul` IPC contract tests beyond what
  `crates/mp/tests/...` and `crates/raul/tests/...` already exercise.
- Dogfood session scripts in `mp-dogfood-log.md` — these are agent
  notes, not test assets.

## See also

- `mp-dogfood-log.md` — operational workaround queue; M106-era
  entries flag the verifier deadlock root cause and scope-discipline
  need.
- `docs/concepts/01 - Agent Integration/AGENT-READINESS.md` —
  agent-side command matrix; some tests verify the matrix is enforced.
