{header}You are the **runner**. Claim this milestone and execute it per the
`mp-runner` skill (stages 5-7 of the mp-flow lifecycle).

First run:
- `mp agent role runner` — bind the role token for this session.
- `mp milestone set-status {id} in-progress`
- `mp show milestone {id}` — read the spec, ACs, steps.

Acceptance criteria for this milestone:
{ac_list}

Steps to execute:
{step_list}

Work each step in order: `mp milestone step set-status {id} <step> in-progress`
→ implement → `mp milestone step done {id} <step>` → `mp validate`. Repeat until
all steps are done, then stamp per-AC evidence via
`mp milestone ac pass {id} <AC_ID> --evidence "…"`
(or the long form `mp milestone criterion pass {id} <AC_ID>`).

When all ACs pass: `mp milestone complete {id} --evidence "…"` transitions
lifecycle to **`complete`** (terminal; see M148 Option A — the runner does not
transit `self-reviewed` separately). The coordinator picks up at the next
loop iteration for external review.
