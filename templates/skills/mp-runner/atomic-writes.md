# Atomic-writes discipline (cross-cutting for executing + fixing)

Both the executing sub-mode (stages 5-7) and the fixing sub-mode (stage 9) write
to the plan through `mp` commands. This file documents the two contracts that
govern those writes: the advisory flock lock and the per-AC killpg timeout.

## Advisory flock lock

The plan directory has a `.mp-write.lock` file. `mp` acquires an advisory
`flock` on this file before writing any milestone JSON.

**Contract: no parallel mp invocations on the same milestone.** If two `mp`
processes try to write the same milestone concurrently, the second one waits
for the lock (no deadlock — `flock` is released on process exit or when the
write completes).

### What this means for the runner

- `mp milestone step done` goes through the lock. If a parallel session is also
  writing, the command blocks briefly but will not corrupt the file.
- `mp milestone complete` goes through the lock. The AC verification shell
  commands run while the lock is held; if the verify phase takes long, the lock
  is held for that duration.
- **Do NOT run two `mp` commands against the same milestone in parallel**
  (e.g., `step done` in one terminal while `complete` is running in another).
  The lock prevents corruption but doesn't make the commands transactionally
  consistent — the second writer sees the first writer's changes only after
  the first writer releases the lock.

## Per-AC killpg timeout contract

`mp milestone complete` runs each AC's verification command in a child process.
If a verification command hangs (infinite loop, waiting for input, etc.), the
per-AC timeout kicks in.

**Contract: per-AC timeout sends SIGKILL to the process group, not a single
pid.** The verification command and all its children are killed atomically.

### What this means for the runner

- If a verification command hangs, `mp milestone complete` will time out after
  the configured duration (default: 300s per AC). The timeout is per-AC, not
  per-milestone — a milestone with 5 ACs gets up to 5 × 300s.
- Use `--timeout-secs <n>` to lower the timeout for commands you know should
  be fast. Use `--cooperative` for commands that need to clean up before exit
  (SIGTERM before SIGKILL with a grace period).
- If verification is hanging because of an interactive prompt, fix the
  verification command to be non-interactive. The runner should never leave a
  verification command that requires human input.

## See also

- Plan I/O integrity — full design of the advisory lock and dry-run gate.
- Verifier timeout hardening — per-AC timeout, `--timeout-secs`,
  `--cooperative`, killpg pattern.
- [executing.md](executing.md) — executing sub-mode (uses these contracts at stages 5-7).
- [fixing.md](fixing.md) — fixing sub-mode (uses these contracts at stage 9).
