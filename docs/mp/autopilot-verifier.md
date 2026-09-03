# `mp` autopilot orchestrator verifier

The autopilot orchestrator in `w<workspace>:p<lane>` accepts a lane
notification only after the **verifier** independently cross-checks
the lane's claim against three on-disk sources. This document
covers:

1. The `mp reviews pass` writing pattern — the canonical place
   where evidence is recorded, so future agents don't look for a
   `review-pass` event in `activity.json` as proof of completion.
2. The cycle 1 recovery lesson — why the verifier cross-checks
   three sources instead of trusting any single one.
3. The multi-source verification convention — every `lifecycle`
   notification must reconcile across the milestone JSON,
   `reviews.json`, and `activity.json`.

> **For agents:** the verifier is the safety net. Don't try to
> "look like you're done" by writing generic summaries or skipping
> per-AC evidence stamping — the verifier rejects both. Run the
> actual `cargo nextest` command, capture its exit code and pass
> count, and stamp that as evidence. The verifier accepts nothing
> else.

---

## The `mp reviews pass` writing pattern

`mp reviews pass <mid> --verdict ok --reviewer <who>` is the
reviewer's lane handoff. It writes **two** durable artifacts:

| Artifact | What it records |
|----------|-----------------|
| `<plan_dir>/reviews.json` | A `ReviewRecord` row (verdict, reviewer, notes, timestamp). The reviewer's `verdict=ok` is the authoritative signal that the milestone passed review. |
| `<plan_dir>/milestones/<mid>.json` (`flow_stages.external-review`, possibly `lifecycle`) | The `external-review` stage transitions to `done`. When verdict=ok AND `execution_status=done`, the milestone may auto-promote `done` → `complete`; the reviewer pass is also the signal that closes the `external-review` stage. |

**What it does NOT do by default:** append a `review-pass` event to
`<plan_dir>/activity.json`. The activity journal is reserved for
the dispatch-side `lifecycle-transition` event written by
`mp milestone complete` (or by `cmd_milestone`'s atomic-write path
on a self-review transition).

> **Lesson (the cycle 1 history):** a future agent looking for
> evidence of a "review-pass" event in `activity.json` will not
> find one. The `reviews.json` row IS the evidence. The verifier
> cross-checks `reviews.json` for the latest verdict AND
> `activity.json` for the matching `lifecycle-transition` event
> — the latter is what surfaces as the durable "milestone reached
> `lifecycle=executed`" signal. Cross-checking both sources is
> what closes the cycle 1 fabrication gap (a fabricated lifecycle
> claim with no matching activity event is rejected as a typed
> mismatch).

---

## The cycle 1 recovery lesson

### What cycle 1 looked like

A runner lane notification reported `lifecycle=executed` for a
milestone. The milestone JSON did NOT reflect this state (its
canonical lifecycle was still `approved`), and there was no
`lifecycle-transition` event in `activity.json` for the milestone.
The orchestrator trusted the lane's report and advanced the cycle
without verifying.

### Why the fix is independent state-reads

Three sources, not one:

1. **Milestone JSON** (`<plan_dir>/milestones/<id>.json`) —
   canonical `lifecycle`, `execution_status`, `spec_status`.
2. **`reviews.json`** — the latest `ReviewRecord` for the
   milestone. The verdict the reviewer recorded.
3. **`activity.json`** — the journal filtered by milestone
   subject. Each lifecycle transition lands here as a
   `lifecycle-transition` event. The verifier uses a durable
   event cursor (the typed `OrchestrationEvent.seq` from the
   session.json event log) — NOT an arbitrary "last three
   entries" tail.

The verifier cross-checks all three before accepting a notification.
A notification that agrees with the milestone JSON but disagrees
with `activity.json` (or has no matching event) is rejected as a
typed `LifecycleClaimUnbacked` mismatch. The orchestrator resends
the lane with a corrective message (3-pane topology) or
escalates to the user (2-pane / 1-pane).

### Why the fix is typed role-boundary detection

A second class of failure had the runner lane calling
`mp reviews pass` to self-complete the milestone, bypassing the
reviewer pane entirely. Per-lane integrity is now enforced by seven
typed detectors (see `crates/mp/src/autopilot/verifier.rs`):

| Detector | Lane | What it catches |
|----------|------|-----------------|
| 1. `RunnerReviewViolation` | runner | Runner called `mp reviews pass` |
| 2. `RunnerClaimViolation` | runner | Runner called `mp reviews claim` or `mp reviews finding add` |
| 3. `RunnerPlanEditViolation` | runner | Runner modified `master-plan/` directly (detected via diff hunk) |
| 4. `ReviewerCodeEditViolation` | reviewer | Reviewer modified code under `crates/...` |
| 5. `ReviewerPrematurePassViolation` | reviewer | Reviewer called `mp reviews pass` before orchestrator prompted |
| 6. `PreStartNotificationViolation` | any | Notify arrived before the lane was started (unknown dispatch id) |
| 7. `OrchestratorCodeEditViolation` | orchestrator | Orchestrator committed code attributable to its own pane id |

Each detector returns a typed `Violation` variant carrying
structured evidence (activity event seq, diff hunk, pane id). The
`recommend_remediation` function maps any violation + topology to
a `Remediation::Resend` (3-pane) or `Remediation::EscalateToUser`
(2-pane / 1-pane).

---

## The multi-source verification convention

Every autopilot notification carries an `ActorAttribution` with
five fields:

| Field | Source |
|-------|--------|
| `session_id` | The session.json id this notification is part of |
| `role` | The lane (`runner` / `reviewer` / `orchestrator`) |
| `actor_token` | The pane id (`%1` / `%2` / `%3`) — the session's `role_state.actor` |
| `dispatch_id` | The `AssignmentDispatched` event id that started this lane |
| `seq` | The session event seq number (monotonic) |

If any field is empty, mismatched against the session event log,
or fabricated, the verifier returns `Verdict::UnknownActor` and
blocks automatic completion. Attribution is read from the session
event log plus `reviews.json`; missing or mismatched identity is
NOT guessed.

### The per-AC evidence contract

Each `AcceptanceCriterion.evidence` string MUST contain, in order:

1. **The exact command** — must start with a runnable name
   (`cargo`, `make`, `rustc`, `bash`, `sh`, a `./` relative path,
   an absolute path, or `scripts/...`). Examples:
   - `cargo nextest run -p mp --test foo --no-fail-fast`
   - `make test`
   - `scripts/run-checks.sh`
2. **`exit <code>`** — the literal exit code, e.g. `exit 0` for
   the happy path.
3. **`(<passed>/<total> pass)`** — the observed pass count from
   `cargo nextest`, with the literal `pass` token after the
   count. e.g. `(3/3 pass)`.

Full example:

```
cargo nextest run -p mp --test verifier_state_cross_check --no-fail-fast exit 0 (1/1 pass)
```

Generic summaries ("All steps done", "M<id> complete: ready for
review", "Cycle 1 done") are rejected with a typed
`EvidenceContractViolation`. The verifier also re-reads the
canonical criterion state after `lifecycle=executed` lands and
compares against the pre-completion snapshot — a back-filled
generic summary after completion is rejected as
`EvidenceShapeError::OverwrittenAfterCompletion`.

### The verification command list

A `LaneNotification.verification_commands` field carries a list of
`VerificationCommand { label, argv: Vec<String> }` entries. The
argv is argv-safe — `Command::args` style, no shell. Nextest
filter parentheses (e.g. `-E 'test(/foo/)'`) are preserved as a
single argv token.

Shell control operators are rejected, not silently skipped:

| Operator | Where it's rejected |
|----------|---------------------|
| `&&` | `check_command_list` → `UnsupportedCommandOperator` |
| `\|\|` | same |
| `;` (inside a token) | same |
| `\n` / `\r` (inside a token) | same |

A multi-command sequence MUST be represented as multiple
`VerificationCommand` entries, not concatenated with `&&` in a
single argv token. The verifier rejects the latter rather than
running only the first command.

---

## Topology-aware remediation

After detecting a violation, the verifier consults
`recommend_remediation(violation, topology)`:

| Topology | Decision |
|----------|----------|
| 3-pane (`three-agent`) | `Resend { corrective_message }` — the lane re-runs with a corrective message; the reviewer pane catches any second misfire |
| 2-pane (`two-agent`) | `EscalateToUser { violation_kind }` — there is no independent reviewer channel; the human operator decides |
| 1-pane (`one-agent`) | `EscalateToUser { violation_kind }` — same rationale; one pane has no independence to recover |

The decision is exposed by `mp::autopilot::verifier::recommend_remediation`.
The cycle-flow layer (C3) consumes this enum; the verifier only
owns the decision surface.

---

## Quick reference for agents

```
mp milestone complete <mid> --evidence "<real cargo nextest output with exit code and pass count>"
```

**Do:**

- Run the actual `cargo nextest` command from the milestone spec.
- Stamp the exit code AND pass count: `… exit 0 (3/3 pass)`.
- Each AC gets its own evidence stamp via
  `mp milestone criterion pass <mid> <AC> --evidence "…"`.
- Use `--no-fail-fast` for stability across parallel tests.

**Don't:**

- Don't write "All steps done" or "M<id> complete" as evidence.
- Don't rely on a single source — `reviews.json` records the
  reviewer's verdict; `activity.json` records the
  lifecycle-transition event; the milestone JSON records the
  canonical state. The verifier checks all three.
- Don't concatenate commands with `&&` in `verification_commands`.
  Use multiple `VerificationCommand` entries.
- Don't fabricate a `lifecycle=executed` claim without a matching
  `lifecycle-transition` event. The verifier rejects as a typed
  `LifecycleClaimUnbacked` mismatch.

---

## Related documentation

- `docs/mp/commands.md` — every `mp` command the agent will use.
- `docs/mp/config.md` — autopilot role + topology configuration.
- `docs/milestone-lifecycle/` — the 12-stage mp-flow timeline.
- `crates/mp/src/autopilot/verifier.rs` — the verifier implementation.