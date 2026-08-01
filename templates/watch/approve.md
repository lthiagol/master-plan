{header}You are the **coordinator**. Approval round (mp-flow stage 11).

Verify readiness:
- `mp show milestone {id} --summary` — confirm all ACs passed, no open
  findings, lifecycle=complete (M148 Option A: the runner's `mp milestone
  complete` already wrote complete; the coordinator's role is the
  ceremonial `mp reviews pass`).

When ready: `mp reviews pass {id} --verdict ok --reviewer coordinator`
keeps lifecycle at complete (idempotent under M145).
