# Executing

You are the **runner**: an approved, decomposed milestone is moving through
`in-progress → done`. Implement in the **code zone** (project source); record
progress in the **plan zone** through `mp`.

## Before you start

```bash
mp execution check                 # is it ready? what's blocking?
mp milestone set-status <id> in-progress    # first step on this milestone only
```

`set-status in-progress` requires the spec to be `approved` or later (**G1**) and
every `depends_on` milestone to be `done` (**G8**).

## The per-step loop

```bash
# Repeat per step:
mp milestone step set-status <id> S1 in-progress   # BEFORE code changes
# … implement in the code zone (not the plan files) …
mp milestone step done <id> S1
mp validate
```

Track the order with `depends_on_steps`; claim/release for parallel work:

```bash
mp milestone step claim <id> S1 --by runner --lease 30m
mp milestone step release <id> S1
```

## Verifying acceptance criteria

When a step is done, record per-AC evidence — **what ran and its exit code**:

```bash
mp milestone criterion pass <id> AC-01 \
    --evidence "cargo nextest run -p mp --test config_set --no-fail-fast  exit 0"
mp milestone criterion pass <id> AC-02 \
    --evidence "cargo clippy -p mp --tests -- -D warnings  exit 0"
mp milestone criterion fail <id> AC-03 --reason "blocked: flaky on CI"
```

Evidence is the run record, not a claim. If you didn't run it, don't assert it.
(See [`core-principles.md`](./core-principles.md) §4.)

## Completing

Once all steps are `done` and ACs verified:

```bash
mp milestone complete <id> --evidence "all ACs green; clippy clean"
```

`complete` re-runs the gates, then flips lifecycle to `done` and the milestone
enters the review queue. It refuses to complete unless:

1. every step is `done`,
2. every AC is `pass`ed (or `fail`ed with a reason), and
3. no open **self-phase** findings remain.

Long evidence can come from a file:

```bash
mp milestone complete <id> --evidence-file ./run.log
```

## Force / skip-verify are debt, not shortcuts

```bash
mp milestone complete <id> --force          # bypass AC gate; stamps [force-bypassed]
mp milestone complete <id> --skip-verify    # skip AC + step verification; [skip-verify]
```

Each records visible debt in evidence, and a force-bypassed milestone **cannot
reach `complete`** until the bypass is resolved or accepted by a reviewer. Prefer
honest completion. If you can't complete honestly, **block** instead:

```bash
mp milestone block <id> --reason "AC-05 blocked: flaky test on CI" --by runner
mp execution pause
# escalate; do NOT call complete
```

Resume later:

```bash
mp milestone unblock <id>
```

## Record drift honestly

If implementation diverged from the spec — a step was skipped, an AC relaxed, a
test made source-grep instead of behavioral — **declare it**, don't hide it:

```bash
mp reviews finding add <id> --severity low --desc "drift: AC-02 relaxed to a grep check"
```

A reviewer who finds undeclared drift loses trust in the whole submission; a
declared drift is a decision they can evaluate.

## After `done`

`done` is **not** terminal — it means "finished, awaiting review." The milestone
moves to the review queue. An **independent** agent/session then reviews it
([`reviewing.md`](./reviewing.md)). You (the executor) should not review your
own work.
