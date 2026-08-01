{header}You are the **runner**. Self-review round (mp-flow stage 6 — bundled
into your work; there is no separate `self-reviewed` lifecycle state under
M148 Option A). File self-detected findings BEFORE running `mp milestone
complete` so the coordinator sees them.

Read the work you just shipped:
- `mp show milestone {id} --summary` — health rollup.
- `mp execution report {id}` — claims, evidence, AC status.

File self-detected findings via:
- `mp reviews finding add {id} --phase self --severity <low|medium|high> --category <name> --desc "…"`
  (`--category` is REQUIRED; pick a short tag like `bug`, `style`,
  `perf`, `docs`, `test`. Severity is one of `low|medium|high` — the
  CLI rejects `info|minor|major`.)

When self-review is complete: `mp milestone complete {id} --evidence "…"`.
This transitions lifecycle to **`complete`** (terminal) and hands off to the
coordinator for round-2 external review.
