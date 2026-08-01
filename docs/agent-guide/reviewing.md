# Reviewing (independent review)

You are the **reviewer** — a different session/context than the executor. Your
job is to verify the executor's *claims* against the actual diff and test
output, then either pass the milestone or file findings. The executor can mark
work complete; it is only considered **shipped** after your independent review
records a verdict via `mp reviews pass`.

## Pick up the queue

```bash
mp reviews pending                       # the queue
mp reviews pending --summary             # steps done/total + open findings per item
mp reviews sweep                         # triage into risk buckets first
```

## Read the claims first, then verify them

```bash
mp execution report <id>                 # the executor's claimed evidence
```

The report and per-AC evidence are **claims**. Verify each by running the named
command, not by trusting the string:

- A claim "test X passes" → run test X.
- "clippy clean" → run clippy.
- "AC-02 verified" → run the AC's `verification` field.

Confirm the diff matches what the steps say they did. Drift hides in reports.

## Pass or file

### Pass (→ terminal `complete`)

```bash
mp reviews pass <id> --verdict ok --reviewer alice --notes "all claims verified"
```

When the spec is also `verified`, a `--verdict ok` auto-promotes the milestone
`done → complete` (terminal). `--verdict changes-needed` does **not** promote.

Batch the queue when appropriate:

```bash
mp reviews pass --all --verdict ok --reviewer alice --filter force-bypassed
```

### File a finding (→ remediation)

```bash
mp reviews finding add <id> \
    --severity high --category correctness \
    --desc "AC-02 verification never actually runs; marked pass by inspection" \
    --author reviewer \
    --phase external \
    --file crates/mp/src/foo.rs --line 42 --side new
```

- Filing an **open external-phase** finding on a `done`/`reviewed` milestone
  auto-enters **`remediation`** and captures the pre-state.
- Filing an open **self-phase** finding blocks `milestone complete`.

### Threaded comments

```bash
mp reviews comment add <id> --author reviewer --body "…" --finding F-01 --file … --line …
mp reviews comment list <id>
```

## The remediation loop

When you've filed findings, the executor fixes them and the milestone comes back
to you:

```bash
# 1. You filed findings → milestone is in remediation
# 2. Executor (different session) fixes each, re-verifies, resolves:
mp milestone set-status <id> executing
# … fix in the code zone …
mp milestone step done <id> <step>
mp reviews finding resolve <id> F-01 --commit <sha>
mp milestone complete <id> --evidence "…"        # re-enters the review queue
# 3. You re-review:
mp reviews pending
mp reviews pass <id> --verdict ok --reviewer alice
```

Resolving the **last open finding** auto-exits `remediation` back to the
captured pre-state.

## Review state, derived

`review_state` is computed from the milestone + the review registry:

| `review_state` | Means |
|----------------|-------|
| *(empty)* | not `done` yet |
| `pending-review` | `done`, no review recorded |
| `open-findings` | reviewed, has open findings |
| `remediated` | reviewed, every finding was *fixed* |
| `reviewed-clean` | reviewed, no findings (or all dismissed) |

```bash
mp reviews lifecycle            # cross-project rollup by review_state
mp reviews lifecycle --summary  # bucket counts
```

## Hunk export (when `review.hunk = true`)

If the project opts in, export findings + comments as hunk-compatible JSON for a
diff-anchored review tool:

```bash
mp reviews hunk <id>                       # live batch on stdout
mp reviews hunk <id> --file context.json   # agent-context sidecar
mp reviews hunk <id> --apply               # pipe into a running hunk session
mp reviews hunk <id> --strict              # drop unanchored findings
```

## Hand-off audit (optional rigor)

For high-stakes work, record and audit coordinator↔runner hand-offs:

```bash
mp reviews handoff <id> --from-role coordinator --to-role runner --data "…" --evidence "…"
mp reviews l5-check <id>                   # audit: same-session / role-inversion violations
```

## The trust invariant

The executor and the reviewer must be **different** agents/sessions. That
independence is the entire point — it is what makes the `complete` verdict worth
something. If you executed the milestone, do not review it.
