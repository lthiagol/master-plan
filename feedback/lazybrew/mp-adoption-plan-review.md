# mp Adoption — Migration Review (lazybrew)

> **Purpose:** Independent review of the `master-plan/` → mp CLI migration captured in
> `mp-adoption-plan-log.md`. This file ships **alongside** the log to the mp repository
> as real-world adoption feedback.
>
> **Reviewer:** opencode (review pass, separate from the executing agent).
> **Reviewed commit:** `83aeeeb` on `chore/mp-adoption` (+ working-tree state at review time).
> **mp version:** 1.4.0 · **Date:** 2026-06-26.

---

## 1. Verdict

**Migration is functionally correct and the data fidelity is high.** All plan content
(brief, charter, 4 milestones, backlog) was faithfully imported; `mp doctor`,
`mp validate`, and `mp status` are green and mutually consistent; history was preserved
in `master-plan-archive/`.

**It is NOT fully "done" / PR-ready yet.** There are **4 concrete defects** (2 dangling
doc refs, 1 stale index file, 1 leftover scaffold) and **3 pending items** (uncommitted
cleanup, unmerged branch, no CI). None are data-loss; all are cheap to fix.

| Area | Status |
|---|---|
| Plan data fidelity (brief/charter/milestones/backlog) | ✅ correct |
| mp health (`doctor`/`validate`/`status`) | ✅ green |
| History preservation (`master-plan-archive/`) | ✅ complete |
| Operational-doc relocation (`docs/operational/`) | ✅ relocated |
| Cross-references after cutover | ⚠️ 2 dangling (§3.2, §3.3) |
| `plan.toml` index vs milestone files | ⚠️ stale (§3.1) |
| Scaffold cleanup | ⚠️ incomplete (§3.4) |
| Commit / merge / CI | ⏳ pending (§4) |

---

## 2. What was verified (claims in the log checked against the tree)

| # | Log claim | Verified how | Result |
|---|---|---|---|
| V1 | Brief T01–T08 filled, `status=done` | `brief.toml` read | ✅ 8/8 `filled`, `done` |
| V2 | Charter: 2 goals, 4 non-goals | `plan.toml [charter]` | ✅ matches |
| V3 | 4 milestones M01–M04, all `spec_status=ready`, deps M04→[01,02,03] | `milestones/*.toml` + `mp list milestones` | ✅ confirmed |
| V4 | M01 has 5 steps, M02 2, M03 3, M04 6; every step has `tests` + `covers_ac` | step counts in TOML | ✅ exact match; G10 empty-tests gaps closed |
| V5 | Backlog: 6 active + 1 resolved duplicate | `backlog.toml` vs archived `backlog.md` | ✅ all 6 active old IDs (B-01,B-07,B-09,B-11,B-12,B-13) imported; 7 historical resolved items correctly **not** imported |
| V6 | B-07 duplicate resolved `wont-fix` | `backlog.toml` B-07 | ✅ `resolution="wont-fix: Duplicate of B-01…"` |
| V7 | Operational docs moved to `docs/operational/` | `ls docs/operational/` | ✅ coverage-audit, release-checklist, review-template, smoke-checklist present |
| V8 | Old plan archived to `master-plan-archive/` | dir listing | ✅ status.md, backlog.md, milestones/ (M01–M28 .md), archive/, templates/ |
| V9 | `.mp/` → `master-plan/` rename clean, no `.mp/` left | `ls .mp` → missing (good) | ✅ |
| V10 | `config.toml` redirect removed; new config `location="master-plan"` | `master-plan/config.toml` | ✅ |
| V11 | `AGENTS.md` Planning Rules rewritten for mp | `git show 83aeeeb -- AGENTS.md` | ✅ mp session-start sequence + new rules added |
| V12 | Adoption ADR row in `DESIGN.md` | `DESIGN.md:118` | ✅ D1–D6 row present |
| V13 | `mp doctor` ok, `mp validate` ok | re-run both | ✅ `ok:true`, 0 errors / 0 warnings |

---

## 3. Defects found (should fix before merge)

### 3.0 Root-cause classification — mp CLI gap vs missed migration step

| Defect | Root cause | Why |
|---|---|---|
| 3.1 `plan.toml` stale `spec_status` | **mp CLI gap** (I-09) | Agent ran the *correct* documented command (`mp milestone approve`); mp updated the authoritative milestone files but not the `plan.toml` index. `mp validate` passes despite the drift and there is no `mp plan sync`/`reindex` to force a refresh — no step was skipped, no signal was missed. |
| 3.2 `AGENTS.md:85` → `coverage-audit.md` | **Missed migration step** | Phase-5 plan listed "update AGENTS references" as a cutover task and several links were fixed; this one line in the Testing Rules section was skipped. mp has no knowledge of links inside the repo's `AGENTS.md`. |
| 3.3 `DESIGN.md:86` → `milestones/18-…` | **Missed migration step** | The cutover link audit was scoped to `AGENTS.md` only; `DESIGN.md` was never in the audit set. Same class as 3.2. |
| 3.4 empty placeholders in `decisions.toml`/`ideas.toml` | **Hybrid: mp-initiated (I-10) + incomplete cleanup** | `mp init` emitted the empty `[[decisions]]`/`[[ideas]]` records (origin = mp). The agent cleaned the identical artifact out of `backlog.toml` but did not repeat it for the other two (persistence = execution). |

**Net:** the only defect whose *origin* is purely an mp bug is **3.1**. **3.2/3.3** are an
under-scoped manual link audit (the agent looked only at `AGENTS.md`, not repo-wide).
**3.4** originated in `mp init` but persisted through incomplete cleanup. That said, mp
*could* have prevented 3.2/3.3 as well with a cutover/link-lint helper or `mp plan
relocate` — that is the process-level suggestion in I-11.

### 3.1 ⚠️ `plan.toml` index is stale — NEW mp issue (I-09)
**The most significant finding, and it's an mp-side bug, not a lazybrew data error.**

- `master-plan/plan.toml` records `spec_status = "review"` for **all 4** milestones.
- The authoritative `milestones/*.toml` files say `spec_status = "ready"`.
- `mp status --format json` and `mp list milestones --format json` both report
  `ready: 4` — i.e. **mp recomputes from the milestone files and ignores the stale index**.

So the runtime is correct, but the on-disk `plan.toml` snapshot drifted after
`mp milestone approve` ran. A human (or any tooling) reading `plan.toml` directly is
misled. Worse: `mp validate` passes despite the drift — there is no guard detecting
index/file inconsistency.

**Suggested mp improvement (I-09):** either (a) make `mp milestone approve` (and every
other mutation) re-sync the `plan.toml` index, or (b) have `mp validate` flag
index↔file drift. Prefer (a) so the file is always authoritative.

> **Lazybrew-side note:** no data fix needed here — once mp re-syncs the index it will
> read `ready`. Optionally `mp` could expose `mp plan sync`/`mp plan rebuild` to force a
> refresh. Until then the file can be left as-is (mp ignores it on read).

**Resolution:** **Left as-is (do not hand-fix).** This is an mp bug with no CLI remediation
today (`mp plan` exposes no `sync`/`reindex`), and hand-editing `plan.toml` would (a)
violate the project's "never hand-edit plan files" rule and (b) destroy the on-disk
evidence of I-09 that this review ships to the mp maintainers. The stale index has no
functional impact — `mp status`/`mp list` already report `ready: 4` — so leaving it is
safe and keeps the bug reproducible upstream.

### 3.2 ⚠️ Dangling reference: `AGENTS.md:85`
```
- New brew command support requires: … a row in `master-plan/coverage-audit.md`.
```
`master-plan/coverage-audit.md` no longer exists — it was relocated to
`docs/operational/coverage-audit.md` in Phase 5. The log's Phase 5 notes claim
"AGENTS.md references … all needed updating" — this one was missed.

**Fix:** `master-plan/coverage-audit.md` → `docs/operational/coverage-audit.md`.

**Resolution:** ✅ Fixed 2026-06-26 — `AGENTS.md:85` now points to `docs/operational/coverage-audit.md`.

### 3.3 ⚠️ Dangling reference: `DESIGN.md:86`
```
See [config ADR in M18.9](master-plan/milestones/18-documentation-and-project-hygiene.md).
```
That milestone `.md` was moved to `master-plan-archive/milestones/18-…md` by the cutover.
Link now 404s.

**Fix:** `master-plan/milestones/18-…` → `master-plan-archive/milestones/18-…`.

**Resolution:** ✅ Fixed 2026-06-26 — `DESIGN.md:86` now points to `master-plan-archive/milestones/18-…`.

### 3.4 ⚠️ Leftover init-scaffold placeholders in `decisions.toml` / `ideas.toml`
Both files still contain the empty `[[decisions]]` / `[[ideas]]` template rows shipped by
`mp init` (e.g. `id = ""`, `summary = ""`, `title = ""`). The log records that the
**same** class of empty placeholder was cleaned out of `backlog.toml` by hand, but the
equivalent cleanup was not applied to `decisions.toml` / `ideas.toml`.

`mp validate` accepts them, and worse — `mp decision list` / `mp idea list` **surface the
empty rows as real records** (one empty decision, one empty idea with `status="open"`),
so the noise is visible to any mp consumer, not just dead TOML. There is no CLI way to
remove them: `mp decision` exposes only `add`/`list` (no `remove`), and `mp idea`'s
`dismiss`/`archive` require an `id` (the placeholder's is `""`).

**Fix:** either drop the placeholder `[[…]]` blocks (leave a header comment only), or
let `mp` own it — see I-10.

**Resolution:** ✅ Fixed 2026-06-26 — dropped the empty `[[decisions]]` / `[[ideas]]`
blocks from `decisions.toml` / `ideas.toml` (header comments retained), matching the
cleanup already applied to `backlog.toml` during the migration. This was a hand-edit
because mp offers no removal command for decisions/ideas — tracked as I-12. Verified post-fix:
`mp validate` still green, and `mp decision list` / `mp idea list` now return empty arrays.

---

## 4. Pending / not-yet-done (log slightly overstates "PR-ready")

The log's Final Outcome says `☑ success … PR-ready`. Strictly that is premature:

| # | Item | Evidence | Action |
|---|---|---|---|
| P1 | `mp-adoption-plan.md` **deletion** + final `mp-adoption-plan-log.md` edits are **uncommitted** | `git status`: modified log, deleted plan | Stage + commit (the plan template says "the plan is deleted at the end" — this is the intended final cleanup, just not committed) |
| P2 | Branch `chore/mp-adoption` is **5 commits ahead of `main`**, not pushed, no PR | `git log main..chore/mp-adoption` | Push + open PR (or merge) per repo git rules |
| P3 | **CI not run** on the branch | log Phase 6 "CI result: Not run on this branch" | Run CI before merge |

Also: the log's own "Logging checklist (for the executing agent)" at the bottom is still
all unchecked `[ ]` — cosmetic, but worth ticking before this ships as mp feedback.

---

## 5. New mp improvement items (for the mp repository)

Extends the log's I-01…I-08 table.

| # | Tag | Area | Observation | Suggested improvement |
|---|---|---|---|---|
| **I-09** | `[IMPROVE]` | plan-index / validate | After `mp milestone approve`, the per-milestone `spec_status` flips to `ready` in `milestones/*.toml`, but `plan.toml` still says `review`. `mp validate` passes despite the index↔file drift; only `mp status`/`mp list` (which recompute) are correct. | Mutations should re-sync `plan.toml`; or `mp validate` should detect index drift; or expose `mp plan sync` to force a rebuild. Highest-value item in this review. |
| **I-10** | `[IMPROVE]` | cli/init | `mp init` writes empty placeholder `[[decisions]]` / `[[ideas]]` / `[[items]]` rows with `id=""`. lazybrew had to hand-edit all three to remove the noise (backlog at migration time; decisions/ideas during this review). | `init` should emit `decisions = []` / `ideas = []` (or omit the `[[…]]` block) rather than a single empty record. Consistent scaffold across all collection artifacts. |
| **I-11** | `[IMPROVE]` | docs/cutover | No mp guidance for the "rename plan dir" / "cutover from parallel bootstrap" workflow. The lazybrew adoption invented it (mv + `mp config set location`) and hit I-08 (config not auto-updated) plus the dangling doc refs in §3.2/§3.3. | Add a `docs/CUTOVER.md` recipe (or `mp plan relocate <old> <new>`) covering: rename dir, update `workflow.plan.location`, and a post-cutover link-lint step. |
| **I-12** | `[IMPROVE]` | cli/decision-idea | No CLI path to delete a decision or idea record. `mp decision` exposes only `add`/`list` (no `remove` at all); `mp idea` has `dismiss`/`archive` (soft) but no hard delete, and both require an `id` — so the `id=""` init placeholders (I-10) are untargetable. The only remediation was a hand-edit of plan TOML, which violates the "never hand-edit plan files" rule. | Add `mp decision remove <id>` and `mp idea remove <id>` (hard delete, distinct from soft `dismiss`/`archive`); and/or give `dismiss`/`archive` a way to target scaffold rows. This is the direct reason §3.4 needed a hand-edit. |

> **Evidence backing:** I-09 (stale `plan.toml` left on disk as live repro), I-10, and
> I-12 are directly backed by on-disk evidence in this repo; I-11 is the process-level
> lesson behind the cutover defects (§3.2/§3.3). I-09 and I-12 also document the missing
> CLI remediation paths (`mp plan sync`, `mp decision/idea remove`) that forced hand-edits.

---

## 6. Corrections / nuances to the log

These don't invalidate the log, but should be truthed-up if it's republished:

1. **Phase 5 friction note is incomplete.** It lists config-location and AGENTS/template
   references as the only cutover follow-ups. It missed the two dangling links in §3.2
   and §3.3 (`AGENTS.md:85`, `DESIGN.md:86`). Add them.
2. **"Artifacts mp now owns" (Final Outcome) is accurate** — spot-checked every path
   listed; all exist. No correction needed.
3. **Phase 3 "Approve → decompose"** is correct at the milestone-file level (`ready`),
   but did not refresh `plan.toml` (§3.1). Worth adding a one-liner so mp maintainers see
   the index drift happened at the `approve` step specifically.
4. **"PR-ready" overstatement** — see §4. Three items still open.
5. **Bottom logging checklist** still unchecked `[ ]`. Cosmetic.

---

## 7. Recommended next actions (lazybrew side)

Ordered, cheap → done. Items 1–2 are now complete (this review pass):

1. ~~Fix `AGENTS.md:85` and `DESIGN.md:86` dangling refs (§3.2, §3.3)~~ ✅ done.
2. ~~Clear the empty placeholder rows in `decisions.toml` and `ideas.toml` (§3.4)~~ ✅ done.
3. Commit the pending working-tree changes: `mp-adoption-plan.md` deletion, final log
   edits, this review, and the §3.2/§3.3/§3.4 fixes (§4 P1). Tick the log's bottom
   checklist while there.
4. Push `chore/mp-adoption`, run CI (§4 P2/P3), then open PR / merge.
5. **Do not** hand-edit `plan.toml` (§3.1) — leave it as live I-09 evidence for mp.
6. (mp-side) File I-09 / I-10 / I-11 against the master-plan repo with this log + review
   attached.

---

## 8. Bottom line

The migration did the hard part right: **no plan data was lost or corrupted**, mp's own
health checks are green, and the historical record is intact. What remains is small
cutover hygiene (2 doc links, 2 scaffold files, a commit, a PR) plus one genuinely
useful mp bug report — the `plan.toml` index drift (I-09) — which is the single most
valuable piece of feedback in this whole exercise.
