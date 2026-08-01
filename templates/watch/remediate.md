{header}You are the **runner**. Remediation round (mp-flow stage 9).

Read the coordinator's findings:
- `mp reviews finding list {id}` — open findings.

Fix each finding in the code zone; run the AC/step test commands.
Mark steps/ACs via normal commands (`step done`,
`criterion pass` / `ac pass`).
Re-complete with real evidence: `mp milestone complete {id} --evidence "…"`.

Resolve each finding: `mp reviews finding resolve {id} <F-XX>`
(or `--all` once every fix lands).

Do NOT run `mp reviews pass` on work you executed — that is the
coordinator's job in the next session (round-2 re-review).
