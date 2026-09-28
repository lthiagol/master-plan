# M246 Review — cycle 1

Cold build of `-p mp` was clean (11.13s incremental after `cargo clean -p mp`).
Per-AC verification commands were re-run independently against the cold
cache; every command matched the runner's recorded counts:

| AC | Tests | Result |
|-----|-------|--------|
| AC-01 (settle)  | 5/5  | PASS |
| AC-02 (stall)   | 3/3  | PASS |
| AC-03 (wait)    | 20/20 | PASS |
| AC-04 (tail)    | 18/18 | PASS |
| AC-05 (cfg)     | 2/2  | PASS |
| AC-06 (full mp) | 3847/3847 (+ 1 skipped) | PASS |

`cargo clippy -p mp --all-targets --no-deps -- -D warnings` exit 0.
`cargo fmt --all -- --check` exit 0.
`make consumer-surface-lint` clean (5 patterns, allowlist absorbed).
`rg -q 'autopilot wait' docs/autopilot/README.md` YES.
`rg -q 'autopilot tail' docs/mp/commands.md` YES.

## Spec clause-by-clause walkthrough

**AC-01 (readiness gate / settle_ms).** `crates/mp/src/autopilot/drive/herdr.rs`
`wait_for_readiness_with`:
- default 5000 (`ReadinessOptions::default`, line 574),
- `idle_since = None` on any non-idle read resets the window (line 637),
- `timeout_ms` still bounds the whole wait (lines 633–641),
- `settle_ms = 0` short-circuits on the first idle read because
  `Duration::ZERO` always satisfies `>=` (line 624), preserving the
  pre-M246 first-idle escape hatch.
- `--prompt-settle-ms` is plumbed all the way: `cli/autopilot.rs:157` →
  `commands/autopilot.rs:108` → `commands/autopilot_drive.rs:636-637`
  → `state_machine.rs::set_readiness_settle_ms` (line 520) →
  `ReadinessOptions::settle_ms` (herdr.rs:566). Visible in
  `mp autopilot start --help`.

**AC-02 (stall timer / 4× ceiling).** `wait_for_lifecycle_with`
(herdr.rs ~895-995) and the production equivalent in
`drive/state_machine.rs` (~700-725) both implement the same two-timer
rule:
- `non_working_accrued` only grows while `status != "working"`
  (herdr.rs:985-987), so a long build is never flagged.
- `since_progress` (time since the last status or lifecycle change)
  bounds the *total* wait via `hard_ceiling = stall_timeout_ms * 4`
  (herdr.rs:991-994; `HARD_CEILING_STALL_MULTIPLE = 4` at line 878).
- The clock is injected via `now_instant = now()`, so tests use
  `VirtualClock::tick()` for deterministic-clock assertions.

**AC-03 (wait exit codes).** `commands/autopilot_observe.rs::cmd_autopilot_wait`
returns `Err(ExitCode(1))` when `report.reached` is false (line 65-67);
`reached=true` returns `Ok(())`. The three failure reasons are exercised
individually in `crates/mp/tests/autopilot_wait.rs`:
- `wait_times_out_with_reason_timeout` — reason=`timeout`
- `wait_reports_run_failed_on_a_failed_run` — reason=`run-failed`
- `wait_reports_run_stopped_when_the_driver_is_gone` and
  `wait_reports_run_stopped_with_no_state_file` — reason=`run-stopped`

**AC-04 (tail subject/order/since/follow).** `crates/mp/src/autopilot/observe.rs::tail_batch`
filters by `e.subject == id` (line 283), honours `e.timestamp > since`
(line 288), and emits the slice past the in-place `TailCursor` (lines
293-297). Tests in `crates/mp/tests/autopilot_tail.rs`:
- `tail_prints_only_the_requested_subject_oldest_first` — 3 in-scope +
  2 interlopers across 3 subjects; asserts subject filter, oldest-first,
  and timestamp monotonicity.
- `tail_respects_since` — exclusive boundary drops the older event.
- `tail_follow_exits_after_idle_seconds_with_no_new_events` — idle
  exit under `--idle 1` in <20s.
- `tail_follow_picks_up_events_written_while_it_runs` — in-process
  appender proves live events are seen during `--follow`.

**AC-05 (stall_timeout_minutes).** `crates/mp/src/config.rs`:
- `STALL_TIMEOUT_MINUTES_RANGE: RangeInclusive<u32> = 1..=240` (line 253).
- `validate_stall_timeout_minutes` is called by `config_cmd.rs` from
  both `set` (line 934) and `validate` (line 686).
- Precedence: `resolve_stall_timeout_ms(flag_ms, cfg)` returns `flag_ms`
  first, then `cfg.agent.automation.stall_timeout_minutes * 60_000`,
  then `DEFAULT_STALL_TIMEOUT_MINUTES * 60_000` (herdr.rs:864-872;
  `DEFAULT_STALL_TIMEOUT_MINUTES = 30` at config.rs:244).
- Tests pin all of this: round-trip at 1, 240, 45; reject at 0, 241;
  hand-edited validate rejects 0, 241 (with field-level message), -5
  (parse error); non-integer gets a specific "expected integer" error;
  precedence exercised for all three precedence layers.

**AC-06 (docs + lint).** `docs/autopilot/README.md` documents `wait`
(line 13), `tail` (line 43), `--prompt-settle-ms` (line 64),
`agent.automation.stall_timeout_minutes` (line 72), and the precedence
(line 82). `docs/mp/commands.md` covers `wait` (line 235) and `tail`
(line 236) in the table. `CHANGELOG.md` Unreleased section covers the
readiness/settle, stall, wait, and tail changes (lines 3-28). No
internal milestone IDs (`M\d+`), lesson codes (`L\d+`), or
`docs/code-review-lessons.md` / `docs/dogfood/*` pointers leak into the
consumer surface (grep confirmed clean).

## Orchestrator-flagged inconsistencies (not findings)

- `milestone.execution_status: "planned"` while `flow_stages.execute:
  "done"`. Pre-existing CLI tooling gap: `mp milestone update` does
  not expose `--execution-status`. Out of M246 scope; not actionable
  by the runner.
- `milestone.executed_by: ""`. By design — only writable via
  `mp milestone complete --executor` (commands/milestone.rs:271).
  Correct at the execute stage.

## Verdict

**OK.** No findings. Every AC clause is implemented in code, every
verification command passes against a cold build, and the docs/lint
gates hold.