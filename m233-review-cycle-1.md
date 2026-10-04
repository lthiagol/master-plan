# M233 Review cycle 1 findings

- **F-01 (medium, spec-drift)** — AC-01 names three paths the tests must
  exercise: "the setsid detach, state-file write, **and shutdown paths**".
  The runner's `autopilot_drive_detach.rs` covers only two of the three
  named paths:
    - setsid detach → covered by `detached_pid_in_state_matches_response_pid`
      (test 3, asserts state.pid ≠ parent pid, which is the observable
      consequence of the `cmd.pre_exec(libc::setsid)` fork)
    - state-file write → covered by `state_file_written_to_autopilot_run_state_json_path`
      (test 2, asserts path/shape)
    - shutdown (SIGTERM / killpg) → **not covered**. The test file's
      own comment block (lines 22-34) explicitly acknowledges the
      exclusion: "widening it would require either a child that
      survives long enough to receive a signal or a production-code
      change to expose a test-only entry point (out of M233 scope)."
  The runner's third test (`preflight_refuses_with_exit_code_2_when_no_harness`)
  covers a real behavior of the detach path but it is not one of the
  three AC-named paths. The strict reading of the AC title — three
  tests, three paths — is unmet. The runner's defense (M233 is
  test-only, so adding a killpg test would require production-code
  changes) is reasonable, but the AC is the contract; the runner
  should either add the shutdown coverage (which would require
  extending the M233 scope to allow the supporting production-code
  touch-up) or revise the AC to drop the shutdown-path clause.

- **F-02 (medium, spec-drift)** — AC-06 names four files where the
  negative tests should drive error paths: "config.rs **/ git.rs** /
  archive.rs / digest.rs". The runner covered only three:
    - config_cmd_negative.rs (3 tests, config_cmd.rs bail sites) ✓
    - archive_negative.rs (3 tests, archive.rs bail sites) ✓
    - digest_negative.rs (3 tests, digest.rs bail sites) ✓
    - **git.rs is not covered**. `crates/mp/src/git.rs` has
      user-reachable `bail!("not a git repository")` sites at lines 76
      (`git_suggest_message`), 96 (`git_commit`), and 148 (`git_push`)
      — all trivial to drive from an integration test against a fresh
      non-git `TestEnv`. The runner's commit message (`bd40a2d`)
      explicitly names "config/archive/digest" — git.rs is an
      intentional scope cut, not an oversight. The strict reading of
      the AC's "config.rs / git.rs / archive.rs / digest.rs" list is
      that all four files should be covered; "at least three" applies
      to the test count (runner has 9, well over 3) but the file list
      reads as the set of sources to exercise.

No HIGH findings. What I verified independently on a fresh `cargo clean -p mp`:

- **Production code untouched**: `git diff --stat HEAD~9..HEAD --
  crates/mp/src/ crates/raul/src/` is empty. M233 stays in test-only
  scope.
- **No G6 escape in gate_matrix.rs**: `rg 'contains\("G6"\) \|\|
  !out.status.success' crates/mp/tests/suites/gate_matrix.rs` returns
  no matches. The previous F-XX escape is gone.
- **G6 / G14 gate_matrix tests pass** (4/4):
  `g6_fires_when_ac_not_passed_at_verified`,
  `g6_clears_when_all_acs_passed`,
  `g14_fires_when_approval_request_pending`,
  `g14_clears_when_approval_resolved`.
- **All four AC wrapper scripts** are executable, have
  `set -euo pipefail`, and cover the full sub-check set:
  - `verify-m233-ac05.sh`: rg escape check + 4-test nextest run for
    G6/G14
  - `verify-m233-ac06.sh`: 3 cargo commands (config_cmd_negative,
    archive_negative, digest_negative)
  - `verify-m233-ac07.sh`: 3-iteration for-loop with the new/changed
    test filter + full mp nextest
  - `verify-m233-step-s1.1.sh`: same shape as ac06
- **AC-07 flakiness check independently re-run, 3/3 pass** (84/84
  tests each run, ~7-8s per run). New tests are not flaky.
- **AC-04 track_archive test** (`track_lifecycle.rs:11`) is real:
  archives a track item via `mp track archive track-item`, asserts
  status="archived" on disk and that the item no longer appears
  under pending/in-progress/blocked in `mp track list --items`.
- **AC-02 breaking_release::apply** tests are real: bailout branch
  asserts non-zero + stderr contains "breaking-release preflight
  refuses to apply"; write-marker branch asserts
  `<plan_dir>/.mp/breaking_release.json` is written with
  `ok=true / applied_at / target_version / evidence_releases`.
- **AC-03 metrics tests** are real: round-trip test sets 4 fields
  then shows them; non-numeric rejection test asserts clap's
  value-parser rejects `--lines-of-code abc` with exit 2 and
  surfaces the field name in stderr.
- **CI wiring**: `make test` runs `cargo nextest run --manifest-path
  Cargo.toml`, which discovers every `crates/mp/tests/*.rs` binary
  automatically. New top-level binaries (`autopilot_drive_detach`,
  `breaking_release_apply`, `config_cmd_negative`, `archive_negative`,
  `digest_negative`) and new suite files (`metrics.rs` via
  `suite_misc.rs:35`, `track_lifecycle.rs` via `suite_track.rs:6`)
  are all picked up by `make ci`'s test step. CI runs the new
  tests.

Verdict: **changes-needed**