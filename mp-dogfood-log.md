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
- Update 2026-09-27 (grooming): `complete` and `reviews pass` already emit lifecycle events; the real gap is `step done` / `criterion pass`. M237 re-scoped to those writers plus `mp activity reconcile <id>` (top-level, not under `autopilot`).
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
- Update 2026-09-27 (grooming): M238 cancelled — slug drift is cosmetic (ids are the key) and `parse_rfc3339_ms` has a single definition (`autopilot/cycle.rs`); `cycle_stale_state_timeout.rs` no longer exists. Verdict → **wontfix**.

---

## Entry — 2026-09-26 — mp-herdr-log.txt + mp-herdr-log2.txt still at the repo root (R15 fragility unfixed)  <!-- points-at: M239 -->

- Date / when: 2026-09-26, audit pass — R15 in mp-herdr-log.txt:328-339 documented that the repo-root log is fragile; M207-M229 consumed the lessons, but the durable record was never built.
- Command attempted: `ls mp-herdr-log*`.
- Observed output: both files still at the repo root, ~2800 lines combined.
- Suspected cause / code path: the future Drive milestone design input (R1-R15) was consumed by M207-M229 in practice, but no `mp autopilot design-input` migration command was ever shipped.
- Verdict: **spec-gap** (architecture gap; logs are not discoverable via `mp`).
- One-line: per R15, mp autopilot design-input should land in master-plan/ and the repo-root logs should go away; M239 closes the loop.
- Update 2026-09-27 (grooming): the logs were already trimmed in commit 438b093 and now falsely claim "migrated via M239". M239 re-scoped: recover R1-R15 + session lessons from `git show 438b093^:mp-herdr-log*.txt` into `mp decision add` (summaries `herdr-log R<n>:` / `herdr-log L<n>:`), then delete both files. No new CLI.
- Status: spec-gap.

---

## Entry — 2026-09-27 — `mp scratch new` prints JSON, AGENTS.md recipe expects a bare path

- Date / when: 2026-09-27, grooming pass over M230-M246.
- Command attempted: `SCRATCH=$(mp scratch new m-update)` then `cat > "$SCRATCH/payload.json"` (the AGENTS.md "Temporary workspace" recipe).
- Observed output: exit 0; stdout is `{"path": ".../.mp-scratch/m-update-<ts>", "scratch_dir": ...}`, so `$SCRATCH` holds JSON and the redirect fails.
- Suspected cause / code path: `mp scratch new` follows the JSON-by-default read contract; the recipe predates it. Either add a `--path-only`/raw mode or fix the recipe to `jq -r .path`.
- Verdict: **bug** (docs ↔ CLI mismatch).
- Status: bug.

---

## Entry — 2026-09-27 — `mp list milestones --include intent` silently ignored

- Date / when: 2026-09-27, grooming pass (M240 premise check).
- Command attempted: `mp list milestones --include intent`.
- Observed output: exit 0; rows carry no `intent` and no warning is emitted for the unsupported include value.
- Suspected cause / code path: `crates/mp/src/commands/list.rs` accepts arbitrary `--include` values without validation. M240 adds `intent.outcome` to list rows unconditionally; the silent-ignore stays.
- Verdict: **bug** (unknown --include values should error like unknown --fields paths).
- Status: bug.

---

## Entry — 2026-09-27 — `mp plan verify-ac` misparses `python3 -c` verifications

- Date / when: 2026-09-27, grooming pass.
- Command attempted: `mp plan verify-ac <id>` on an AC whose verification was `python3 -c '...'`.
- Observed output: `python script not found: 3`.
- Suspected cause / code path: the verify-ac executor's interpreter detection treats `python3` as `python` + script `3`. Workaround used: express counts with `rg | wc -l` / `awk` instead of `python3 -c`.
- Verdict: **bug**.
- Status: bug.

---

## Entry — 2026-09-27 — W43 false positives for dotted step ids (`S1.2` → `S12`)

- Date / when: 2026-09-27, grooming pass (`mp validate` after M232/M243 edits).
- Command attempted: `mp validate`.
- Observed output: `W43 step S2.1 action references non-existent step S12` (M232) and `... S22` (M243), although S1.2 / S2.2 exist.
- Suspected cause / code path: `crates/mp/src/validate/milestone_warnings.rs::extract_milestone_refs` builds `cleaned` with `is_alphanumeric()` (dropping `.`) before the S-ref check that intends to allow `.` — the dot branch is dead.
- Verdict: **bug**.
- Status: bug.

---

## Entry — 2026-09-27 — no writer for `cancel_reason`

- Date / when: 2026-09-27, cancelling M238.
- Command attempted: `mp milestone set-status 238 cancelled` (no reason flag exists).
- Observed output: cancelled; `cancel_reason` cannot be set by any command. Workaround: `mp note add --milestone-id 238 --title ... --body <reason>`.
- Suspected cause / code path: `set-status` has no `--reason`; `deferred` has `deferred_reason` but cancel has no counterpart.
- Verdict: **spec-gap**.
- Status: spec-gap.

---

## Entry — 2026-09-27 — approved milestones can be re-scoped with no re-approval signal

- Date / when: 2026-09-27, grooming pass.
- Command attempted: `mp milestone update <id> --json @payload` (title / problem / scope) plus AC/step rewrites on milestones in `lifecycle: approved`.
- Observed output: all writes succeed; `needs_regrooming` stays false, lifecycle stays approved, no warning.
- Suspected cause / code path: fragment and metadata writers don't consult lifecycle; there is no "spec changed since approval" marker.
- Verdict: **spec-gap** (a material rewrite of an approved spec should at least be flagged).
- Status: spec-gap.

---

## Entry — 2026-09-27 — step authoring ergonomics (`--order`, `step add --depends-on-steps`)

- Date / when: 2026-09-27, grooming pass.
- Command attempted: `mp milestone step add <id> ... --depends-on-steps S1.1.1`; reordering steps via `step update --order`.
- Observed output: `unexpected argument '--depends-on-steps'` on `step add` (only `step update` has it); no `--order` anywhere, so ordering is expressed only through dependencies. Also `step remove` refuses when split children exist, so duplicate parents must be repurposed instead of removed.
- Suspected cause / code path: `mp milestone step add` flag set is narrower than `step update`.
- Verdict: **backlog**.
- Status: backlog.
- Update 2026-09-27: `step add --depends-on-steps` scheduled in M247. `--order` → **wontfix** (dependencies already express order). `step remove` refusing with split children is correct behavior.

---

## Entry — 2026-09-27 — `set-spec-status ready` jumps a bare draft straight to `lifecycle: approved`

- Date / when: 2026-09-27, creating M247.
- Command attempted: `mp milestone create --json @…` (lifecycle draft), then `mp milestone set-spec-status 247 ready` because `mp milestone wp add` refuses below spec_status ready.
- Observed output: exit 0; lifecycle went draft → approved in one call while the milestone had zero work packages and zero steps, with no groom/specify stages and no approve gate.
- Suspected cause / code path: set-spec-status maps `ready` onto `lifecycle: approved` without running the approve gate, and the WP/step writers require `ready`, so the only way to decompose a new milestone is to approve it first.
- Verdict: **spec-gap** (ordering deadlock: decomposition requires approval, approval should require decomposition).
- Status: spec-gap.
- Update 2026-09-27: scheduled in M248 (approval integrity).

---

## Entry — 2026-09-27 — `milestone create --json` rejects `work_packages` / `steps`

- Date / when: 2026-09-27, creating M248.
- Command attempted: `mp milestone create --json @create-c.json` with a full spec (ACs + work_packages + steps).
- Observed output: exit 1, `milestone create JSON contains unsupported field(s): 'steps'; 'work_packages'`.
- Suspected cause / code path: `CreateMilestoneInput` carries both fields and the create path applies them (`milestone/spec.rs` ~463-470), but the `CREATE_MILESTONE_KEYS` allow-list (~190) omits them, so the JSON path can never create a decomposed draft.
- Verdict: **bug**.
- Status: scheduled in M248 (S1).

---

## Entry — 2026-09-28 — `milestone complete` leaves the index `spec_status` out of sync with the file

- Date / when: 2026-09-28, closing M235 after a herdr-orc cycle.
- Command attempted: `MP_VERIFY_ALLOW_SHELL=1 mp milestone complete 235 --evidence "…"` (ACs passed, all steps done, zero open findings).
- Observed output: `complete` exits 0 and reports `lifecycle: complete`, `spec_status: verified`, `execution_status: done`. A following `mp validate` then emits `W03: milestone 235 index spec_status="implemented" does not match file spec_status="verified"`.
- Observed scope: W03 fires on M235 only; no other milestone in the plan reports it.
- Suspected cause / code path: the `complete` writer derives the index entry's `spec_status` from the execution transition (yielding `implemented`) while the milestone file's own `spec_status` is derived from the canonical `lifecycle: complete` (yielding `verified`). Two derivations of the same legacy alias disagree, and `plan.json` is written without a consistency check against the file it mirrors.
- Workaround: none found. `mp validate` does not self-heal, and no reindex command was located. The mismatch is only visible as a warning, so it does not block work — but it makes the plan permanently non-clean until someone hand-edits `plan.json`, which the plan zone forbids.
- Verdict: **bug**.
- Status: backlog.
