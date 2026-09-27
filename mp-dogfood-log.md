# mp-dogfood-log

Workaround queue and `mp` issue tracker for this repo (which dogfoods master-plan).
Append one entry per finding. **Process rule (M110):** every new hygiene entry
must map to a queued milestone (`spec_status: planned`) or a new milestone —
never silent. Record the target as `<!-- points-at: M### -->` on the entry.

<!-- planted-milestone-pointer-rule -->

Status legend at the end of each entry:

- `wontfix` — acceptable, documented and moving on.
- `backlog` — file as B-NN / BF-NN in `backlog.json` (or a new milestone) later.
- `spec-gap` — extends the scope of an existing milestone.
- `bug` — defect in `mp`; attach or assign to a milestone's findings.

## Format

```markdown
## Entry N — YYYY-MM-DD — <one-line title>  <!-- points-at: M### -->

- When:           date / phase (e.g. executing M159 on a Mac).
- Command:        (or `Command attempted`) the exact invocation or call.
- Observed:       exit code, stdout/stderr excerpt, agent interpretation.
- Suspected:      (or `Suspected cause`) mp subcommand / Rust module / config involved.
- Verdict:        **bug** | **backlog** | **spec-gap** | **wontfix**.
- One-line:       (optional) one-line summary.
- Status:         wontfix | backlog | spec-gap | bug   # if Verdict not used.
```

---

## 2026-09-26 — Audit pass (M110 hygiene)

Resolved entries trimmed: see `git log` / `mp list milestones` for the
milestones that closed each prior entry. The plan counter advanced past
M235 with the **code-review audit** (M230–M235) and **log-file audit**
(M236–M239) milestones. The remaining entries below are still-open,
active work — do NOT silently remove them. Address each via the pointed-at
milestone (or a successor), not by reverting the work that closed them.

---

## Entry — 2026-09-26 — doc_overindented_list_items clippy lint at crates/mp-model/src/milestone.rs:155-158  <!-- points-at: M236 -->

- Date / when: 2026-09-26, audit pass — 4 prior autopilot milestones (M209, M212, M220, M222) reviewed-and-shipped with this lint still red.
- Command attempted: `cargo clippy --all-targets -- -D warnings` (exit non-zero).
- Observed output: `crates/mp-model/src/milestone.rs:155:9: warning: doc list item which should belong to the previous paragraph has a non-consecutive indentation [clippy::doc_overindented_list_items]` — same line every time.
- Suspected cause / code path: clippy 1.98 added this lint; the file pre-dates that release. A 5-line reformat (doc bullet indentation + continuation alignment) closes it.
- Verdict: **bug** (4 milestone occurrences documented; backlog unaddressed).
- One-line: clippy 1.98 doc-list indentation false-flag in mp-model; recurring across M209/M212/M220/M222; fixed in M236.
- Status: bug.

---

## Entry — 2026-09-26 — mp milestone step done / criterion pass / complete do not write activity.json events  <!-- points-at: M237 -->

- Date / when: 2026-09-26, audit pass — first seen 2026-09-01 in M200 review-pass event loss; reproduced 2026-09-03 in M207 lifecycle-event absence; reaffirmed in 2026-09-04 session-4 closeout (mp autopilot activity reconcile is the R-late answer that M225 only partially delivered).
- Command attempted: `mp show milestone 207 --fields milestone.lifecycle,milestone.execution_status`, then `grep '"subject": "207"' master-plan/activity.json`.
- Observed output: milestone JSON shows lifecycle=complete, execution_status=done, but `grep` returns empty. The phase-5 review cycle's R5 verification handled the absence (the milestone JSON is canonical), but a recovery path is missing.
- Suspected cause / code path: the lifecycle-writing subcommands in `crates/mp/src/milestone/` (or wherever they live now after M228's rename) do not write to activity.json by default; `mp reviews pass` has the same gap. M225's restart + reconciliation was for *partial state* — it didn't include an "activity rebuild" recovery path.
- Verdict: **bug** (cli surface gap; no recovery command).
- One-line: lifecycle writers don't add activity.json events; recovery path fixed in M237 (`mp autopilot activity reconcile`).
- Status: bug.

---

## Entry — 2026-09-26 — M211 milestone file name stale; M213 parse_rfc3339_ms duplicated  <!-- points-at: M238 -->

- Date / when: 2026-09-26, audit pass — M211 stale slug first observed 2026-09-03 in mp-herdr-log2.txt session-1 retrospective; M213 duplication observed in M213 cycle-1 review (F-02 low, backlog).
- Command attempted: `ls master-plan/milestones/211*.json`; `rg -n 'fn parse_rfc3339_ms' crates/mp/src/`.
- Observed output: file is `211-mp-autopilot-lane-notification-wire-format-shell-command-never-printed-text.json` while the title is "mp autopilot typed task assignment — orchestrator dispatch through herdr argv"; `parse_rfc3339_ms` returns 2 definitions (cycle.rs, cycle_stale_state_timeout.rs).
- Suspected cause / code path: 09412e2 renamed the M211 title but not the file; the autopilot/drive/ helpers accreted without consolidation.
- Verdict: **backlog** (cosmetic; reviewer noise).
- One-line: cosmetic hygiene batch from herdr-log backlog; M211 file renamed + parse_rfc3339_ms extracted to autopilot/drive/time.rs in M238.
- Status: backlog.

---

## Entry — 2026-09-26 — mp-herdr-log.txt + mp-herdr-log2.txt still at the repo root (R15 fragility unfixed)  <!-- points-at: M239 -->

- Date / when: 2026-09-26, audit pass — R15 in mp-herdr-log.txt:328-339 documented that the repo-root log is fragile; M207-M229 consumed the lessons, but the durable record was never built.
- Command attempted: `ls mp-herdr-log*`.
- Observed output: both files still at the repo root, ~2800 lines combined.
- Suspected cause / code path: the future Drive milestone design input (R1-R15) was consumed by M207-M229 in practice, but no `mp autopilot design-input` migration command was ever shipped.
- Verdict: **spec-gap** (architecture gap; logs are not discoverable via `mp`).
- One-line: per R15, mp autopilot design-input should land in master-plan/ and the repo-root logs should go away; M239 closes the loop.
- Status: spec-gap.
