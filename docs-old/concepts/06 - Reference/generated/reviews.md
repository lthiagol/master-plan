# `mp reviews`

**Usage:**

```text
Usage: reviews <COMMAND>
```

**Subcommands:**

| Name | Description |
|------|-------------|
| `status` | Unified review discovery: execution-review queue + spec-review milestones |
| `pending` |  |
| `pass` |  |
| `list` |  |
| `show` |  |
| `sweep` | Classify the pending review queue into risk buckets (alias: triage) |
| `lifecycle` | Cross-project rollup of milestones by review_state |
| `finding` | Manage structured findings on milestones |
| `comment` | M133 AC-01: add or list threaded review comments on a milestone |
| `handoff` | M133 AC-02: record a coordinator/runner hand-off on a milestone. The persisted shape mirrors the hand-off protocol documented in `mp-flow`'s Hand-off protocol section (from/to direction, data, session-boundary, evidence) |
| `l5-check` | M142 AC-01..AC-05: run the L5 evidence audit on a milestone's hand-off records. Detects three violation classes: `same_session_across_role_boundary`, `missing_session_identity`, `role_inversion`. Output is JSON: `{ok, violations, summary}`. Exit code is 0 in both clean and violation cases (advisory, not blocking) |
| `hunk` | M154: export the milestone's findings + comments as hunk- compatible JSON. The default channel is the live `comment apply` batch on stdout (pipe to `hunk session comment apply --stdin`). `--file <path>` switches to the agent-context sidecar (loaded at startup by `hunk diff --agent-context <path>`). `--apply` pipes the batch into a live session when one is running; without a live session it prints the batch and a hint instead of erroring (per AC-04) |

