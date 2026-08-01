# Lifecycle: review (`executed → complete`) and remediation

`executed` means "the work is finished." Work is considered **shipped** only when
it has been independently reviewed: a different session/context than the
executor verifies the claims before they count. This is the trust model — the
review registry (`mp reviews …`) records who reviewed, the verdict, and any
findings.

> **Vocabulary note:** the executor's end-state was renamed from
> `done` to `executed` so the "work finished" state is unambiguously
> distinct from the terminal-reviewed `complete` state. The legacy
> aliases `done` (lifecycle) and `self-reviewed` (review-flow) parse
> to the same `Executed` phase during the migration window; new writes
> always emit `executed`.

## The review sub-path

```
executed ──► (independent review) ──► reviewed ──► complete
                                        │                │
                                        └────────────────┴──► remediation  (open finding)
                                                                  │
                                                                  └─► (last finding resolved) ──► self-reviewed
```

`self-reviewed`, `reviewed`, and `remediation` are review-flow states owned by
the **review registry** (`mp reviews …`), not by the plain lifecycle setters.

## The review flow

```bash
mp reviews pending                      # find review-ready milestones
mp execution report <id>                # read the executor's CLAIMS first
# … verify claims against the diff + actual test output …
mp reviews pass <id> --verdict ok --reviewer alice      # → complete (terminal)
#   — or —
mp reviews finding add <id> --severity high --category correctness --desc "…"   # → remediation
```

**Verify, don't trust.** The execution report and per-AC evidence are *claims*.
Read them, then confirm against the real diff and by actually running the named
tests. A claim that "test X passes" is verified by running test X.

### Verdicts

- `--verdict ok` — review passes. When the spec is also `verified`, this
  auto-promotes `executed → complete` (terminal).
- `--verdict changes-needed` — review does not pass; the milestone is not
  promoted.

### Batch review

```bash
mp reviews pass --all --verdict ok --reviewer alice --filter force-bypassed
```

Resolves the whole (optionally filtered) queue in one call.

## Findings & remediation

A **finding** is structured review feedback with a severity, category, optional
code anchor, and a phase (`self` or `external`).

```bash
mp reviews finding add <id> \
    --severity high --category correctness \
    --desc "AC-02 verification never actually runs; marked pass by inspection" \
    --author reviewer \
    --phase external \
    --file crates/mp/src/foo.rs --line 42 --side new
```

### Auto-remediation

Filing an **open external-phase finding** on an `executed`/`reviewed` milestone
auto-enters `remediation` and captures the pre-state so the exit restores it
exactly. Filing an open **self-phase finding** blocks `milestone complete`
until resolved.

### The remediation loop

```bash
# 1. Reviewer files findings → milestone enters remediation
# 2. Executor fixes each finding:
mp milestone set-status <id> executing
# … address each finding in the code zone …
mp milestone step done <id> <step>      # re-verify the affected step(s)
mp reviews finding resolve <id> F-01 --commit <sha>
# … re-run self-verification (ACs green) …
mp milestone complete <id> --evidence "…"
# 3. The milestone re-enters the review queue; reviewer re-reviews.
```

Resolving the **last open finding** auto-exits `remediation` back to the
captured pre-state. A finding is closed with
`mp reviews finding resolve <id> <F-NN> --commit <sha>`.

## Review state, at a glance

`review_state` is derived from the milestone + the review registry:

| `review_state` | Means |
|----------------|-------|
| *(empty)* | not yet `executed` — not in the review flow |
| `pending-review` | `executed`, no review recorded yet |
| `open-findings` | reviewed, but has open findings |
| `remediated` | reviewed, and every finding was *fixed* |
| `reviewed-clean` | reviewed, no findings (or all dismissed) |

Roll it up across the project with `mp reviews lifecycle` (or
`mp reviews lifecycle --summary` for bucket counts).

## Triage & export

- `mp reviews sweep` (alias `triage`) — classify the pending queue into risk
  buckets before diving in.
- `mp reviews hunk <id>` — export findings + comments as hunk-compatible JSON
  (requires `review.hunk = true` in config). `--file <path>` writes an
  agent-context sidecar; `--apply` pipes a live batch into a running session.
- `mp reviews comment add/list <id>` — threaded comments, optionally anchored to
  a file/line and linked to a finding.
- `mp reviews handoff <id> …` / `mp reviews l5-check <id>` — record and audit
  coordinator↔runner hand-offs (session identity, role boundaries).

## When to skip external review

Independent review is **mandatory for milestones** before they are considered
shipped — run `mp reviews pass` from a different session than the executor.
**Tracks** (small bugfixes/tweaks) skip it: they go `start → done` directly
because their blast radius is small. If a track grows complex, promote it
(`mp track promote --to-milestone`) and it inherits the full review flow.
