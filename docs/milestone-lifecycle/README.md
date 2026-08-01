# The milestone lifecycle

A milestone is in **exactly one** lifecycle state at any time. Lifecycle is a
single linear field (`milestone.lifecycle`); a few orthogonal *overlays*
(blocked, deferred, cancelled) ride alongside it without being lifecycle values
themselves.

This document is the state machine at a glance. Stage detail lives in:

- [`planning.md`](./planning.md) — `draft → groomed → approved`
- [`execution.md`](./execution.md) — `in-progress → done`
- [`review.md`](./review.md) — `self-reviewed → reviewed → complete`, plus
  `remediation`, `block`/`unblock`, `defer`, and `cancel`

## Lifecycle states

| State | Meaning | Who puts it here |
|-------|---------|------------------|
| `draft` | Spec exists but is incomplete/rough | `milestone create` (default) |
| `groomed` | Spec is interview-checked and gap-free | `milestone set-spec-status review` |
| `approved` | Spec is approved; ready to implement | `milestone approve` |
| `in-progress` | Actively being implemented | `milestone set-status in-progress` |
| `done` | All steps done + ACs verified | `milestone complete` |
| `self-reviewed` | Executor self-reviewed their work | review registry |
| `reviewed` | Independent review passed, no findings | `reviews pass --verdict ok` |
| `complete` | **Terminal.** Verified + reviewed | `reviews pass` auto-promotes `done → complete` |
| `remediation` | Review found issues; re-opened to fix | filing an open external finding (auto) |

> `complete` and `cancelled` are the only terminal states. A terminal milestone
> cannot transition further except in narrow, deliberate cases.

## The forward path

```
draft ──► groomed ──► approved ──► in-progress ──► done ──► (review) ──► complete
   ▲          │           │             │            │
   │          │           │             │            └─► self-reviewed ──► reviewed
   └──────────┘           │             │                     │                  │
   (needs-regrooming)     │             │                     └──► remediation ◄─┘ (open finding)
                           │             │
                           │             └──► blocked (overlay) ──► unblock ──► (resume)
                           │
                           └──► deferred (overlay) ──► reopen
```

The review sub-path (`done → self-reviewed → reviewed → complete`) is driven by
the **review registry**, not by the plain lifecycle setters. In practice:

1. The executor runs `milestone complete` (after self-verifying all steps and
   ACs) → the milestone is complete and enters the review queue.
2. An **independent** reviewer (a different session/context than the executor)
   verifies the claims against the diff and tests.
3. `reviews pass --verdict ok` records the verdict and promotes the milestone to
   `complete` (terminal) when verified. A finding opened in review routes the
   milestone into `remediation`.

See [`review.md`](./review.md) for the full review + remediation flow.

## Orthogonal overlays

These are separate boolean fields on the milestone, not lifecycle values. They
can coexist with any lifecycle state.

| Overlay | Set by | Cleared by |
|---------|--------|------------|
| `blocked` (+ `block_reason`, `blocked_by`, `blocked_at`) | `milestone block --reason …` | `milestone unblock` |
| `deferred` (+ `deferred_reason`) | `milestone defer --reason …` | `milestone reopen` |
| `cancelled` (terminal) | `milestone set-status cancelled` | — (terminal; restore from archive instead) |
| `needs_regrooming` | validation when a previously-approved spec drifts | re-running `groom` |

## Legacy fields (spec_status / execution_status)

Older plans carry two legacy fields alongside `lifecycle`. They are read-only
views derived from the canonical lifecycle:

| `lifecycle` | legacy `spec_status` | legacy `execution_status` |
|-------------|----------------------|---------------------------|
| `draft` | `draft` | `planned` |
| `groomed` | `review` | `planned` |
| `approved` | `ready` | `planned` |
| `in-progress` | `ready` | `in-progress` |
| `done` / `self-reviewed` / `remediation` | `implemented` | `done` / `done` / `in-progress` |
| `reviewed` / `complete` | `verified` | `done` |

Setters that write `lifecycle` keep the legacy aliases in sync automatically, so
both views agree. Prefer reading `lifecycle` in new code/scripts; use the legacy
fields only when an older consumer expects them.

## Gates that guard transitions

`mp validate` (and the mutation commands themselves) enforce these gates. They
exist to stop inconsistent plans before they're written.

| Gate | Rule |
|------|------|
| **G1** | `in-progress` requires `spec_status` of `ready` or later (i.e. the spec is approved before work starts) |
| **G3** | Promoting to `review` requires at least one acceptance criterion |
| **G4** | Promoting to `review` requires the configured minimum out-of-scope items (`full` = 2, `hybrid` = 1) |
| **G8** | Before execution, every `depends_on` milestone must be `done` |
| **G14** | Pending `approval-request` annotations block the milestone |

Gate minimums are configurable: `planning.require_min_out_of_scope` and
`planning.require_min_acceptance_criteria`. See
[`../mp/config.md`](../mp/config.md).

## What "done" actually requires

`milestone complete` is an honest gate, not a label flip:

1. **Step gate** — every step must be `done`.
2. **AC gate** — every acceptance criterion must be `pass`ed with non-prose
   evidence (a test name + exit code, a command), or `fail`ed with a reason.
   `--force` records a bypass in evidence (creates visible debt); `--skip-verify`
   skips both AC and step verification entirely (records `[skip-verify]`).
3. **Self-finding gate** — no open *self-phase* findings may remain.

Honest completion is what makes the review queue trustworthy. See
[`execution.md`](./execution.md).
