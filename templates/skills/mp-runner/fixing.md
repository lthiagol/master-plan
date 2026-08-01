# Fixing sub-mode — stage 9

The fixing sub-mode covers the runner's remediation domain after the
coordinator's round-2 external review at stage 8. The fix cycle is:
read findings → patch → resolve → re-verify.

## Session-boundary rule

The author should not be the only reviewer — same-session self-review
is unreliable: the runner session that produced the code at stages 5-7
is NOT the same session that fixes the coordinator's findings at stage
9. A fresh runner session picks up the findings.

The (c) hand-off defines the session boundary between stage 8 (coordinator
filing findings) and stage 9 (runner remediation). The coordinator's session at
stage 8 closes; the runner's fresh session opens at stage 9.

## The fix cycle

### Step 1: Read findings

```
mp reviews finding list <id>
```

The coordinator's external findings (phase `external`) are the target. Self-findings
(phase `self`) may also appear if the runner filed any at stage 6.

Filter by severity to prioritize: `blocker` and `major` first, then `minor`, then
`low`.

### Step 2: Patch the code

For each finding, patch the code per the finding's description. The finding's
`category` field (correctness, security, performance, maintainability) guides
the remediation approach.

### Step 3: Mark findings resolved

```
mp reviews finding resolve <id> <finding-id>
```

One resolve command per finding. After resolution, the finding's status
transitions to `fixed` and its `resolved` timestamp is set.

### Step 4: Re-verify

```
mp milestone verify <id>
```

Runs all AC verifications (same as `mp milestone complete` without the
lifecycle transition). If any AC fails, the fix is incomplete — return to
step 2.

### Hand-off back to coordinator

When all findings are resolved and `mp milestone verify` passes, the fix
cycle is complete. The runner does NOT call `mp milestone complete` at
stage 9 — that is only for stage 7. The (d) hand-off passes the
remediated code + finding resolutions to the coordinator for re-review
at stage 10. The coordinator's fresh session picks up the milestone and
runs `mp milestone verify` again.

## Loop-back

If the coordinator's re-review at stage 10 finds new findings, they file them
and the cycle loops back to stage 9. A fresh runner session picks up the new
findings and runs the fix cycle again.

## See also

- `mp-flow` — stages 9 and 10 in the 12-stage timeline; Hand-off protocol
  section documents points (c) and (d): coordinator→runner, runner→coordinator.
- `mp-coordinator` — coordinator's external review at stage 8.
- The session-boundary discipline (the author should not be the only reviewer).
- [executing.md](executing.md) — executing sub-mode for stages 5-7.
- [atomic-writes.md](atomic-writes.md) — advisory flock lock + per-AC killpg contracts.
