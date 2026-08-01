# Verifier cancellation (M107 + M117)

**Status:** M117 follow-up to M107 (B-52 / ER-5 closure).
**Owner:** pi.
**Companion code:** `crates/mp/src/commands/milestone.rs`, `crates/mp/src/ac_verify.rs`.

The cancellation contract has two parallel paths, both pinned by
integration tests in `crates/mp/tests/ac_verify_per_ac_timeout_killpg.rs`:

| Path | Trigger | Mechanism | Test |
|------|---------|-----------|------|
| Cooperative (global-deadline) | Orchestrator flips `cancelled: Arc<AtomicBool>` | killpg + child.kill() + child.wait() + bounded-join drains | `cooperative_cancel_path_also_uses_killpg` |
| Per-AC timeout (`MP_VERIFY_TIMEOUT_SECS`) | Verifier exceeds its per-AC deadline | **killpg + child.kill() + child.wait() + bounded-join drains** (M117 S1) | `per_ac_timeout_killpg_kills_child_process_group` |

**Belt-and-suspenders rationale.** M107 S3 added killpg to the global-
deadline path. M117 S1 extends the same killpg contract to the per-AC
timeout path because the M107 external review (ER-5, dogfood-log entry
17 sub-2 → B-52; the review notes themselves were removed in the wip
cleanup — see git history) flagged that the per-AC path was using
single-pid `child.kill()` only. Single-pid SIGKILL reaches the leaf
but leaves forked subprocesses (cargo, rsrc) orphaned when those
subprocesses didn't propagate signals. The same review noted that
prior fix to lib was unkillable without `killpg(pgid, SIGKILL)`
on the entire process group. Both paths now share the same dance.

**Per-AC timeout is the most common runaway in practice.** Steps with
`tests: "make test"` or `cargo test --all` are not malformed per se, but
on slow CI the per-AC timeout (default 300s) fires faster than the
global deadline (default 1800s). M117 ensures the orphaned-subprocess
failure mode can't happen on the per-AC path either.

## 1. Problem

`commands::milestone::run_milestone_complete` (M106/S15 path) spawns a worker
thread (`mp_complete_verifier`) that calls `ac_verify::verify_milestone_in` and
`ac_verify::verify_step_tests_in`. The orchestrator waits up to
`MP_COMPLETE_GLOBAL_DEADLINE_SECS` (default 1800s) via `rx.recv_timeout`. On
the timeout branch it currently executes:

```rust
// commands/milestone.rs:223
std::mem::forget(verifier_handle);
```

The worker thread is **detached, not joined**. It will eventually exit
because `ac_verify::execute` enforces its own per-AC `MP_VERIFY_TIMEOUT_SECS`
(300s default) and calls `child.kill()` + `child.wait()`; but between the
global-deadline firing and the per-AC timeout landing there is a window
during which:

1. The verifier thread is consuming CPU and memory.
2. The verifier's child process (the `sh -c "<user's verification command>"`
   subprocess) is **still running**. `sh` does not `setsid`, but
   `cargo test …` inside it inherits the verifier's process group, which
   equals the orchestrator's process group. The child outlives the
   `Command::wait()` because the orchestrator never calls it.
3. Inside `ac_verify::execute`, the two pipe-drain threads (stdout + stderr)
   are also detached via `bounded_join`'s overflow branch on macOS read-
   ready/close race (`ac_verify.rs:399`). They exit when the pipe closes,
   but no one joins them.

**Net effect on global-deadline abort:** 1 verifier thread, 1 verifier
subprocess tree, up to 2 drain threads — all detached. Under
`make test-fixtures`-style verification (broad-scope AC/step strings) this
fires reliably, which is exactly the M104-gating scenario.

## 2. Cancellation strategy (defense in depth)

Two independent layers; either alone improves things, both together are
robust against the macOS read-ready / close race and against verifier
subprocesses that ignored SIGTERM.

### 2a. Cooperative layer (in-process)

- Add `cancelled: Arc<AtomicBool>` to `ac_verify::execute`'s signature, plumbed
  through from `verify_milestone_in` / `verify_step_tests_in`.
- The orchestrator flips `cancelled` BEFORE calling `mem::forget` on the
  verifier handle. (Actually replacing `mem::forget` with a `.join()` since
  the verifier will return promptly once it sees the flag.)
- `ac_verify::execute` checks `cancelled` between `try_wait()` polls and
  between stdout/stderr drain loops. When set:
  - Call `child.kill()`; `child.wait()`. The `Child` struct's `Drop` then
    closes the pipe FDs (the kernel-side pipe write ends have already been
    closed by the kill); drain threads see EOF and exit; `bounded_join`
    joins them promptly. No explicit pipe-handle close is needed.
  - Return `Err("cancelled by global-deadline".into())` or a structured
    `VerificationOutcome::Cancelled`.

> **M108 (S4) ER-3** (audit log): an earlier draft of this section listed
> three explicit steps including an in-line pipe-close call. The
> implementation in `ac_verify::execute` (cancel branch, ~line 543)
> instead relies on the `Child` struct's `Drop` to close the pipe FDs
> once `wait()` returns. The effect is identical (drain threads see
> EOF) but the description above matches the actual call sequence so
> future readers do not reproduce the missing third line as a
> copy-paste regression.

### 2b. Process-group layer (kernel)

- In `ac_verify::execute`, before `command.spawn()`:
  - `.process_group(0)` (stable since Rust 1.64; puts the child in its own
    process group with pgid == child_pid).
  - Register `child.id()` (or its pgid, which equals its pid) in an
    `Arc<Mutex<Vec<u32>>>` returned alongside the spawn result.
    (M117 CR: the original draft of this section specified `HashSet<i32>`,
    but the implementation settled on `Vec<u32>` — see
    `crates/mp/src/ac_verify.rs` `execute` for the actual type.)
  - `Child::id()` returned `u32` directly on stable Rust ≥ 1.74; the
    pre-1.74 `Option<u32>` API is documented but no longer relevant on
    the current toolchain. (M117 CR.)
- The orchestrator saves the set alongside the verifier handle.
- On global-deadline timeout, in addition to the cooperative signal, iterate
  the set and call `libc::killpg(pgid, libc::SIGKILL)` (note: pass the
  positive `pgid`; the libc binding handles the internal negation that
  translates to `kill(-pgid, sig)`). Each child dies; its remaining
  subprocess tree dies transitively because `sh -c` does not `setsid`,
  so cargo's pgid == sh's pgid.
- This requires adding `libc = { version = "0.2", default-features = false }`
  to `crates/mp/Cargo.toml`. `libc` adds zero non-default transitive deps;
  current mp transitive count is 137, budget is 150.

### 2c. `mem::forget` removal

The line 223 `std::mem::forget(verifier_handle)` is **removed**. The worker
thread is `join()`ed instead. Because the cooperative flag is flipped first,
the verifier will return within a bounded time, and `join()` will not block
indefinitely. If for any reason the verifier truly fails to observe the
flag, the killpg pass on the registered children guarantees the subprocess
tree dies; the verifier thread then dies when its `child.wait()` returns.

## 3. Test assertion surface

`commands::milestone::tests::global_deadline_cancels_worker` (new `mod tests`
block — `commands/milestone.rs` currently lacks one, EOF is a free function
`global_complete_deadline_secs()`).

Test scenario:

1. Set `MP_COMPLETE_GLOBAL_DEADLINE_SECS=1` via `std::env::set_var` (must
   serialize because env mutation is process-global — use a `serial_test`
   macro or a mutex; the test suite does not yet use `serial_test`, so we
   add it as a dev-dep, OR we refactor `global_complete_deadline_secs` to
   take a `Duration` arg and call it directly. Prefer the latter — it
   removes one env var and one dep. The function signature change is
   internal-only.)

2. Construct a `CommandArgs` that points to a deliberately hanging
   verifier: a fixture `verifier_hangs.json` whose `step_tests[i].tests`
   is `sh -c 'sleep 30'` instead of `cargo test`. Run the verifier
   path directly (not through the CLI), so we don't need a full `mp
   milestone complete` invocation that touches the plan on disk.

3. Assert within the wall-clock budget (cleanup < 1s; reap < 2s):
   - `rx.recv_timeout(global_deadline)` returns `Err(Timeout)`.
   - The orchestrator handler returns the `gate: "global-deadline"`
     payload.
   - `verifier_handle.is_finished() == true` (the join succeeded; the
     worker exited cleanly under the cooperative flag) within
     `Duration::from_secs(1)` of the cancel flip.
   - The hang subprocess is reaped via `waitpid(pid, &status,
     WNOHANG)` in a loop bounded by `Duration::from_secs(2)`. The
     authoritative "child is gone" signal is `WIFSIGNALED(status)`
     returning `true` with `WTERMSIG(status) == SIGKILL` — NOT a
     `kill(pid, 0)` ESRCH probe, which is unreliable because macOS
     and Linux both return 0 for zombies whose `wait()` has not yet
     been called. After the cooperative flag flip and the join the
     worker drops its `Command::Child` without `wait()`-ing, so the
     child is signal-terminated but still tracked as a zombie until
     the orchestrator reaps it via the `waitpid` probe.

4. Failure mode the test catches: if the cooperative flag is wired but not
   read, the join hangs past 1s and the test fails with "cleanup took
   {elapsed:?}; expected sub-second". If killpg is wired but
   `process_group(0)` isn't, the child is not the process-group leader,
   the killpg returns `ESRCH` for an inert pid, and the
   `WIFSIGNALED(status)` / `WTERMSIG(status) == SIGKILL` assertion
   fails because the child exits via the normal sleep completion
   path (`exit 0`) rather than via signal.

> **M124 (M107 ER-2)** (audit log): an earlier draft of §3 item 3
> described a `kill(-pid, 0)` ESRCH probe and a 3s budget, matching
> the M107 S3 first-cut test. The post-F-2 contract replaces both:
> the probe is `waitpid` (because `kill` ESRCH cannot distinguish a
> live process from a zombie), and the budget tightens to 1s for the
> cooperative-cleanup join plus 2s for the `waitpid` reap loop.
> Implementation matches the test assertions on
> `commands::milestone.rs` `tests::global_deadline_cancels_worker`
> (see line ~1110 onward for the `waitpid` + `WIFSIGNALED` +
> `WTERMSIG` block).

## 4. Scope discipline

In scope for S3 (M107):

- `crates/mp/src/commands/milestone.rs` (orchestrator)
- `crates/mp/src/ac_verify.rs` (worker; threaded `cancelled` flag)
- `crates/mp/Cargo.toml` (`libc` dep)
- New `mod tests` block in `commands/milestone.rs`

In scope for M117 (per-AC hardening, B-52 / ER-5 closure):

- `crates/mp/src/ac_verify.rs::execute` — add `killpg_child` call on
  the per-AC `MP_VERIFY_TIMEOUT_SECS` timeout branch (mirrors the
  cooperative cancel path); broaden to `killpg + child.kill() +
  child.wait() + bounded-join drains`.
- `crates/mp/tests/ac_verify_per_ac_timeout_killpg.rs` — new
  integration test pinning both the per-AC timeout killpg contract
  (`per_ac_timeout_killpg_kills_child_process_group`) and the
  cooperative cancel killpg contract
  (`cooperative_cancel_path_also_uses_killpg`).
- This doc — extend the cancellation-path table to cover the per-AC
  row (killpg + child.kill() + child.wait() + bounded-join drains).

Out of scope (deliberately, post-M117):

- The `bounded_join` overflow branch remains a detached thread.
  The `killpg` pass on the child plus `child.wait()` guarantee the
  child's pipe closes, so the drain thread sees EOF and exits; the
  race window shrinks from "until the orchestrator's process dies"
  to "until the SIGKILL arrives at the child". That's an operational
  improvement, not a structural fix to `bounded_join`. Tracked as a
  follow-up only if a future test surfaces a residual leak.
- The per-AC timeout default (300s) is unchanged. M117 only hardened
  cancellation; bumping the default is a separate config decision.

## 5. Risk register

| # | Risk | Mitigation |
|---|------|------------|
| R1 | `process_group(0)` requires Unix. Windows builds break. | `#[cfg(unix)]` gate the process-group path; on non-Unix fall back to cooperative-only (already what we have, just remove the `mem::forget`). mp targets Unix primarily; Windows would degrade gracefully. |
| R2 | `libc::killpg` returns ESRCH if process group is empty (child already exited). | Treat ESRCH as success — the desired state is "process dead"; it doesn't matter whether we caused it. |
| R3 | `std::env::set_var` on `MP_COMPLETE_GLOBAL_DEADLINE_SECS` is not safe under parallel test runs. | Refactor `global_complete_deadline_secs()` to take a `Duration` and inject from tests; remove env mutation in tests. Same applies to `verify_timeout_secs` in `ac_verify.rs`. |
| R4 | `Command::id()` may return `None` if the child was reaped already. (M117 CR: stale — `Child::id()` returns `u32` directly on stable Rust ≥ 1.74; the pre-1.74 `Option<u32>` API is no longer relevant on the current toolchain. The new `killpg_child` uses `i32::try_from(pid)` for the safety case that u32 > i32::MAX, but real-world Linux PIDs are bounded by `pid_max` (default ~4M, well under i32::MAX).) | If a future Rust release reintroduces `Option<u32>`, ignore `None` entries; the orchestrator's join already covers that case. |
| R5 | The verifier might block inside an FFI call (e.g., `setpgid` itself) on a future rust version. | Unlikely; killpg is a thin `kill(-pgid, SIGKILL)` syscall. If it ever hangs, the global deadline's outer `rx.recv_timeout` still fires and `cmd.wait()` is OS-level reaping. |
| R6 | Adding `libc` dep pushes transitive count over 150. | Measured: `libc` 0.2 adds 0 non-default transitive. Current 137 + 1 = 138, well under budget. |
