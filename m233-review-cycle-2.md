# M233 Review cycle 2 — verdict ok

Both cycle-1 findings resolved. No new HIGH or MEDIUM blockers.

### F-01 resolved — shutdown path covered

`crates/mp/tests/autopilot_drive_detach.rs:279-451`
`shutdown_signals_recorded_pid_and_persists_terminal_outcome` is real:

- Detaches once via the normal `mp autopilot start --detach` to
  scaffold the v2 state file at `<plan_dir>/.mp/autopilot-run.state.json`.
- Spawns a python3 sleeper via `sh -c '(python3 … &)'` (double-fork
  reparent to launchd/init so init reaps on exit and `kill(pid, 0)`
  returns ESRCH within one poll).
- Reads the sleeper's pid from a pidfile (5s timeout, 100ms backoff).
- Patches the state file to point at the live sleeper (`pid` +
  `run_outcome = null`).
- Runs `mp autopilot stop --timeout-secs 10` and asserts:
  - status success
  - report.stopped == true
  - report.pid == sleeper_pid
  - `kill(sleeper_pid, 0)` returns ESRCH (errno=3)
  - state.run_outcome.kind == "gracefully-stopped"
  - report.elapsed_secs in [0, 10)

The test file's comments explain the python3-on-double-fork design
choice (BSD `sleep` ignores SIGINT; bash non-interactive masks SIGINT;
without reparenting the sleeper becomes a zombie of the test process
and `is_pid_alive` polls true forever). KillOnDrop RAII guard prevents
leak if the test panics.

The clippy fix in c6e213e uses `Range::contains` (Rust 1.85+ idiomatic).

Verified independently on cold build: `cargo nextest run -p mp --test
autopilot_drive_detach --no-fail-fast` → 17/17 pass, including the
new shutdown test.

### F-02 resolved — git.rs negative tests added

`crates/mp/tests/git_negative.rs` — 3 tests covering the two
CLI-reachable `bail!("not a git repository")` sites in `git.rs`:

- Line 76 (`git_suggest_message`) — test 1: stderr carries
  "not a git repository".
- Line 96 (`git_commit`) — test 2: stderr carries the wording AND
  `.git/` is NOT created when the bail fires.
- Line 96 again — test 3: deterministic regression check, the
  wording appears once per run across 2 attempts (catches a layered
  Error-printer regression).

The third site at line 148 (`git_push`) is not CLI-reachable — the
`GitCmd` enum at `crates/mp/src/cli/git.rs:4-11` exposes only Status,
SuggestMessage, Commit. `git_push` is internal to `git_commit`'s flow
(reached only after its own line-96 bail has returned), so the
runner's interpretation that 3 tests covering 2 reachable sites is
defensible (the third test is a regression guard, not a separate site).

Verified independently on cold build: `cargo nextest run -p mp --test
git_negative --no-fail-fast` → 16/16 pass (3 new + 13 shared common
tests).

### Other checks

- All 3 wrappers updated to include `git_negative`:
  `verify-m233-ac06.sh:17`, `verify-m233-step-s1.1.sh:15`,
  `verify-m233-ac07.sh:20`.
- Production code untouched: `git diff --stat HEAD~4..HEAD --
  crates/mp/src/ crates/raul/src/` empty.
- AC-05 wrapper: 4/4 G6/G14 tests pass.
- AC-06 wrapper: 16/16 across 4 negative binaries.
- AC-07 wrapper: full mp suite 3956 passed, 1 skipped.
- Flakiness independently re-run with runner's filter, 3/3 runs
  pass (101/101 each, ~9s each).
- `cargo fmt --all -- --check` exit 0.
- `cargo clippy -p mp --tests --no-deps -- -D warnings` exit 0.

Verdict: **ok**
