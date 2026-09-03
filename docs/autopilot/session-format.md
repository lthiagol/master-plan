# M207 — Autopilot session.json schema

Durable per-session state for `mp autopilot` drives. Every autopilot
session lives at `<plan_dir>/autopilot/<id>/session.json` —
self-contained so it can be archived, diffed, and recovered in
isolation.

This document is the canonical surface for downstream autopilot
milestones (M210–M217). The JSON-schema source of truth lives at
[`schemas/autopilot-session.schema.json`](../../schemas/autopilot-session.schema.json);
the Rust typed view lives in
[`crates/mp/src/autopilot/session.rs`](../../crates/mp/src/autopilot/session.rs).

## Top-level shape

```text
{
  "id": "<session-id>",
  "schema_version": 1,
  "herdr_workspace": "<project>-autopilot",
  "topology": { "orchestrator": {...}, "runner": {...}, "reviewer": {...} },
  "roles":    { "orchestrator": {...}, "runner": {...}, "reviewer": {...} },
  "config_overrides": { ... },
  "queue":   [ <queue-item>, ... ],
  "status":  "draft" | "active" | "paused" | "stopped" | "completed" | "failed",
  "terminal_status": "completed" | "failed" | "cancelled",
  "started_at": "<RFC3339>",
  "last_updated": "<RFC3339>",
  "last_state_change_at": "<RFC3339>",
  "role_state": { ... },
  "working_on": { ... },
  "prompt_bundles": { ... },
  "controls": { ... },
  "runner_notes": [ <runner-note>, ... ],
  "events":  [ <event>, ... ],
  "event_cursor": { "last_seq": <int> },
  "ac_projections": { <milestone-id>: { "<AC-id>": <ac-projection>, ... }, ... },
  "queue_cycle_history": [ <cycle-history-entry>, ... ],
  "schema_migrations":   [ <schema-migration>, ... ]
}
```

### Required fields

`id`, `schema_version`, `topology`, `roles`, `queue`, `status`,
`last_updated`. Loaders reject unknown `schema_version` values
(`SessionLoadError::UnknownSchemaVersion`).

## Field-by-field

### `id`

Slug (`^[a-z0-9][a-z0-9-]*$`). Mirrors the path segment under
`master-plan/autopilot/<id>/session.json`.

### `schema_version`

Integer, currently `1`. Bumped on any breaking change. Loaders
reject unknown versions rather than silently load a future-shaped
file.

### `herdr_workspace`

Optional. The herdr workspace name (e.g.
`<project-name>-autopilot`).

### `topology`

Three-pane topology in v1 — `orchestrator`, `runner`, `reviewer`.
Each entry is a `pane_ref` (`pane_id` required; `label` optional).
Future topologies can extend without a `schema_version` bump as long
as the three-pane default remains valid.

### `roles`

Per-role config snapshot. Re-spawn reuses the same
`model` / `harness` / `skill` / `config_hash` so an interrupted
session resumes with the same agent config.

### `queue`

Ordered list of milestones the session is driving. Per-milestone
sub-state is a typed object (not a sparse map) so the verifier sees
a stable shape regardless of which milestone it inspects:

```text
{
  "milestone_id": "207",            // matches ^[0-9]{2,}(\.[0-9]+)*$
  "stage": "pending" | "in-progress" | "executed"
          | "self-reviewed" | "reviewed" | "complete",
  "cycle": 1,
  "last_notify": "<RFC3339>",       // optional
  "verifier_verdict": "<string>",   // optional
  "evidence_refs": {                // optional, but spec-recommended
    "lifecycle":        "in-progress",
    "execution_status": "in-progress",
    "spec_status":      "ready",
    "reviews_verdict":  "<from reviews.json>"
  }
}
```

### `status`

`draft | active | paused | stopped | completed | failed`. The
session driver treats `completed | failed | cancelled` as terminal
(also reflected in `terminal_status`).

### `role_state`

Per-role current state record:

```text
{
  "role": "orchestrator" | "runner" | "reviewer",
  "state": "idle" | "starting" | "working" | "blocked" | "done" | "unknown",
  "since":  "<RFC3339>",
  "actor":  "<actor-token>",
  "working_on": { "milestone_id": "...", "cycle": 1, "role": "runner" }
}
```

Every transition goes through
`mp autopilot session transition --session <id> --role <role>
--state <state> [--working-on <m:n>]`. Direct edits to
`role_state.*.state` are technically possible on disk but the
autopilot driver never does that — the transition table at
[`crates/mp/src/autopilot/transitions.rs`](../../crates/mp/src/autopilot/transitions.rs)
gates every mutation.

#### Transition table

| from      | to         | note                              |
|-----------|------------|-----------------------------------|
| idle      | starting   | normal start                      |
| idle      | working    | fast-path start                   |
| idle      | blocked    | blocked before any work           |
| starting  | working    | normal progression                |
| starting  | blocked    | agent failed to come up           |
| starting  | idle       | cancellation                      |
| working   | done       | finished                          |
| working   | blocked    | self-reported blocker             |
| working   | working    | state-self refresh (allowed)     |
| working   | idle       | forced cancel                     |
| blocked   | working    | resume after blocker resolved     |
| blocked   | idle       | cancellation while blocked        |
| done      | idle       | cycle reset                       |
| done      | working    | new cycle on same role            |
| unknown   | idle       | first observation                 |
| unknown   | starting   | fast-path first observation        |
| unknown   | working    | fast-path first observation        |

Anything else (e.g. `idle -> done` skipping `working`) is rejected
with `TransitionError::InvalidTransition`.

### `working_on`

The (milestone_id, cycle) the runner/coordinator is currently
driving. Cleared on `idle` / `done`; mirrored into `role_state.*.working_on`
while a role is `working`.

### `controls`

```text
{ "paused": false, "pause_reason": "...", "resume_after": "<RFC3339>" }
```

### `runner_notes`

Typed notes the runner leaves for the reviewer. Each note:

```text
{
  "kind": "info" | "warn" | "blocker" | "decision" | "reminder" | "system",
  "body": "<free text>",
  "cycle": <int>,                   // required or derived
  "milestone_id": "<milestone-id>", // optional; defaults to working_on
  "timestamp": "<RFC3339>"
}
```

#### Cycle derivation (`mp autopilot note add`)

1. If `--cycle` is supplied, that value wins.
2. Else if `session.working_on` is set, that cycle is used.
3. Else if the queue has exactly one in-progress / executed item,
   that cycle is used.
4. Else reject with `NoteError::AmbiguousCycle`. No implicit cycle 1.

A cycle of 0 is rejected (`NoteError::ZeroCycle`). An empty body is
rejected (`NoteError::EmptyBody`).

### `events` / `event_cursor`

Append-only sequence-numbered event log. Every orchestration action
(dispatch, transition, review, decision, control, note) records one
event with `seq = cursor + 1`. The cursor is bumped atomically
with each save; recovery (`recover_session`) reconciles a stale
cursor against the surviving tail. `EventCursor::advance_to`
refuses any regression.

Event payload:

```text
{
  "seq":          <int>,
  "kind":         "dispatch" | "transition" | "review" | "decision"
                 | "control" | "note" | "recovery",
  "at":           "<RFC3339>",
  "actor":        "<token>",
  "session_id":   "<id>",            // optional
  "role":         "orchestrator" | "runner" | "reviewer",  // optional
  "milestone_id": "<id>",            // optional
  "cycle":        <int>,             // optional
  "payload":      { ... }            // kind-specific; open object
}
```

### `ac_projections`

Per-milestone map of AC id -> projection. **Milestone criterion
status remains canonical** (`plan.json` / `reviews.json`) — this
is a revisioned *projection*, not a second authority:

```text
{
  "207": {
    "AC-01": {
      "ac_id":           "AC-01",
      "status":          "pending" | "in-progress" | "passed" | "failed" | "blocked",
      "evidence":        "<string>",
      "source_revision": "<revision-key>",   // required
      "projected_at":    "<RFC3339>"
    }
  }
}
```

#### Revision discipline

`source_revision` is the only thing that prevents a stale writer
from clobbering the canonical truth. `project_ac_status` returns:

- `Written` — projection was new or value changed.
- `NoChange` — same `source_revision` + same payload.
- `StaleRevision { stored, attempted }` — different `source_revision`;
  the stored projection is preserved. The caller must reconcile the
  canonical state before re-trying.

`canonical_revision(seed, milestone_id, &[(ac_id, status)])` is a
helper that produces a stable revision key from the canonical
bytes. Two revisions are equal iff their canonical AC state is
byte-equal.

### `queue_cycle_history`

Per-cycle audit:

```text
{
  "milestone_id":  "<id>",
  "cycle":         <int>,
  "started_at":    "<RFC3339>",
  "completed_at":  "<RFC3339>",
  "outcome":       "<string>"
}
```

### `schema_migrations`

Audit of `schema_version` bumps the file has been through:

```text
{ "from_version": <int>, "to_version": <int>, "at": "<RFC3339>" }
```

## Lifecycle

The session has its own lifecycle (`status`) distinct from the
milestone lifecycle inside it. The two do not block each other —
a session can be `paused` while a milestone is still `in-progress`.

```text
draft → active → (paused ↔ active) → completed | failed | cancelled
```

`terminal_status` is set on the transition into a terminal state
and cleared on resume.

## Atomic writes + recovery

Writes go through [`store::atomic_write`](../../crates/mp/src/store.rs)
(temp-file + fsync + rename + parent dir fsync). A crash mid-write
leaves the *previous* valid document on disk; the destination is
either the pre-write or the post-write version, never a hybrid.

Recovery (`mp autopilot session show` → load → reconcile) handles:

- **Schema drift** after a partial write: re-validates every read
  against the embedded schema.
- **Cursor drift**: walks the surviving events and bumps the
  cursor to `max(events.seq)` so the next append does not regress.
- **Torn files**: parse-failure rejects the document non-fatally;
  the next write produces a fresh session.json.

No event is ever deleted or rewritten. Append-only.

## CLI surface

```text
mp autopilot session list
mp autopilot session show <id>
mp autopilot session transition --session <id> --role <r> --state <s> [--working-on <m:n>] [--actor <a>]
mp autopilot note add --session <id> --kind <k> --body <body> [--cycle <n>] [--milestone <id>]
```

## State authority

| Source                              | Authority            |
|-------------------------------------|----------------------|
| `plan.json` (milestone criteria)    | canonical            |
| `reviews.json` (verifier verdicts)  | canonical            |
| `session.json` (this file)          | revisioned projection |
| `session.json` events              | append-only audit    |
| `session.json` role state          | typed state machine  |

Milestone criterion status and review records remain canonical.
`session.json` only stores revisioned projections plus an
append-only event journal of orchestration actions. Stale or
conflicting writes are rejected rather than silently overwriting
the canonical truth.