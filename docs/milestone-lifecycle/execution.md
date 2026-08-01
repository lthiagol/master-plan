# Lifecycle: execution (`in-progress → executed`)

Once a milestone is `approved`, it can be executed. Execution moves the milestone
to `in-progress`, walks its steps to `done`, verifies every acceptance criterion,
and then `complete`s.

> **Vocabulary note:** the executor's end-state was renamed from
> `done` to `executed` so the "work finished" state is unambiguously
> distinct from the terminal-reviewed `complete` state. Step status
> remains `done` (a step is finished; the milestone is executed).

## States

### `in-progress`
Work is actively under way. Setting this requires the spec to be `approved` or
later (gate **G1**) and every `depends_on` milestone to be `executed` (gate **G8**).

### `executed`
All steps are `done`, every AC is verified, and (for self-phase) no open findings
remain. The milestone now waits in the review queue. (Legacy alias: `done`.)

## The implementation plan (phase 2)

Implementation planning happens **after** spec approval. Don't scaffold steps at
spec-creation time.

```bash
mp milestone approve <id>
mp milestone decompose <id>            # scaffold work packages + steps
mp milestone wp add <id> --name "Data layer" --goal "…" --rollback "…"
mp milestone step add <id> --wp WP1 \
    --action "Add config struct" \
    --files crates/mp/src/config.rs \
    --tests "cargo nextest run -p mp --test config_set" \
    --done-when "Config round-trips through mp config set" \
    --covers-ac AC-01
mp validate
```

Each step records:

- **`action`** — what to do.
- **`files`** — the files it touches (bare paths or comma-separated).
- **`tests`** — the command that proves it. Prefer observable commands
  (`cargo nextest run -p mp --test config_set`), not prose.
- **`done_when`** — the human-readable success condition.
- **`covers_ac`** — which acceptance criteria this step advances.

## The execution loop

```bash
mp execution check                     # are milestones ready? what's blocking?
mp milestone set-status <id> in-progress   # first step on this milestone only

# Repeat per step:
mp milestone step set-status <id> S1 in-progress   # BEFORE code changes
# … implement in the code zone (not the plan zone) …
mp milestone step done <id> S1
mp validate

# When all steps are done, verify each AC:
mp milestone criterion pass <id> AC-01 --evidence "cargo nextest run -p mp --test config_set --no-fail-fast  exit 0"
mp milestone criterion pass <id> AC-02 --evidence "cargo clippy -p mp --tests -- -D warnings  exit 0"

# Complete (runs the gates again, then flips to done):
mp milestone complete <id> --evidence "all ACs green; clippy clean"
```

## Evidence is test output, not prose

When you pass an AC, record **what ran and its exit code**, e.g.
`cargo nextest run -p mp --test config_set --no-fail-fast  exit 0`. "Test X
verifies Y" is a claim, not evidence — a reviewer verifies it by running the
command, not by trusting the string.

## Completion gates

`milestone complete` refuses to flip to `executed` unless:

1. **Every step is `done`.**
2. **Every AC is `pass`ed** (or `fail`ed with a reason). ACs that can't honestly
   pass must be blocked (`milestone block`) and escalated — never faked.
3. **No open self-phase findings.**

Escape hatches (use sparingly, both record visible debt):

- `--force` — bypass the AC gate; writes `[force-bypassed]` into evidence.
- `--skip-verify` — skip AC *and* step verification; writes `[skip-verify]`.

A force-bypassed milestone cannot reach `complete` until the bypass is resolved
or explicitly accepted by a reviewer.

### The review gate

`mp milestone complete` on a non-track milestone with no recorded
`mp reviews pass --verdict ok` row ends the milestone at `executed`
(the executor's end-state), NOT terminal `complete`. The terminal
state requires an independent review pass.

Three exits:

- **`change_kind: track`** auto-skips the review gate (the track fast-path).
- **`mp reviews pass --verdict ok <id>`** promotes `executed` → `complete`.
- **`mp milestone complete <id> --skip-review`** is the recorded-debt
  escape hatch — it bypasses the gate but writes `[skip-review: ...]`
  into evidence so the bypass is auditable.

`--force` does NOT bypass the review gate (per F-01). `--force` only
skips the AC verification gate; even a `--force`-bypassed milestone
still needs a review or `--skip-review` to reach terminal `complete`.

## Blocked? Defer, don't fake

If you cannot complete honestly:

```bash
mp milestone block <id> --reason "AC-05 blocked: flaky test on CI" --by runner
mp execution pause
# … escalate to the user; do not call `complete` …
# Later, when unblocked:
mp milestone unblock <id>
```

Never fake completion, silently defer a step, or mark a step `done` that wasn't.

## After `executed`

`executed` is **not** terminal — it means "work finished, awaiting review." The
milestone enters the review queue. Reaching the terminal `complete` state
requires an independent review pass. Continue to [`review.md`](./review.md).
