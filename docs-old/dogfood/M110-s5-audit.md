# M110 S5 — Dogfood-log audit (entries 22+)

> **Source log:** [`mp-dogfood-log.md`](../../mp-dogfood-log.md) (entries 22–36)
> **Audit date:** 2026-07-16
> **Scope:** Every entry in the dogfood log since the M110 ship
> (post-RC tail of agent-automation work). Per M173 S5, this is a
> one-shot triage pass: each entry is classified as `wontfix` /
> `backlog` / `spec-gap` / `bug` / `doc`, and any open items are
> candidates for follow-up backlog promotion.

The triage captures the current verdict (from the log itself when
present), the closure path (where it landed if known), and the
promotion decision (whether to file a new backlog item, close, or
leave for a sibling milestone).

---

## Triage table

| Entry | Date | Title (short) | Closure milestone | Current verdict | Audit verdict | Action |
|-------|------|---------------|-------------------|-----------------|---------------|--------|
| 22 | 2026-07-11 | M138 complete: verification-gate prose recurs (dupe of Entry 21) | M177 | spec-gap (fixed) | **doc** | log only — M177 closed |
| 21 | 2026-07-10 | M135 complete: AC-02 verification text trips the gate | M177 | spec-gap (fixed) | **doc** | log only — M177 closed |
| 23 | 2026-07-12 | `mp install` deploys SKILL.md only; sibling deep-dives missing | M175 | bug (fixed) | **doc** | log only — M175 closed |
| 26 | 2026-07-13 | M160 ships sccache wiring; local warm-cache drop is ~19%, not ≥50% | M176 | spec-gap (re-spec) | **backlog** | re-spec AC-01 to incremental proxy |
| 27 | 2026-07-13 | M161 ships oracle split; local wall-clock change within noise, not ≥1 min | M176 | spec-gap (re-spec) | **backlog** | re-spec AC-04 to incremental rebuild |
| 28 | 2026-07-13 | M162 ships in-process test surface pilot; AC-02 + AC-04 unmet (~2%) | M175 | spec-gap (re-spec) | **backlog** | re-spec to ≥95% conversion metric |
| 30 | 2026-07-15 | M169 external review: Tab/click on Settings wipes staged edits | M169-rev | bug (fixed in M169-rev) | **doc** | log only — M169-rev closed |
| 31 | 2026-07-15 | M169-rev remediation: four findings from Entry 30 fixed | M169-rev | bug (resolved) | **doc** | log only |
| 32 | 2026-07-15 | M169-rev scrollbar fixes: `measure_paragraph_height` capped at ~2 rows | M169-rev | bug (resolved) | **doc** | log only |
| 33 | 2026-07-15 | Sub-agent code review of M169-rev: one HIGH + 3 docs + 2 mediums | M169-rev | bug (HIGH resolved) | **backlog** | M4 render-path test → follow-up milestone |
| 34 | 2026-07-15 | Sub-agent L3a + M4: hash-keyed cache + render-path partial-scroll test | M169-rev | bug (resolved) | **doc** | log only |
| 35 | 2026-07-15 | M169-rev2: scroll still doesn't reach bottom; `h` triggers `PreviousLane` | M169-rev2 | bug (resolved) | **doc** | log only |
| 36 | 2026-07-16 | `mp changelog add --version unreleased` wipes CHANGELOG history | M170 | bug | **backlog** | promote to B-81 (regression test) |

**Summary:** 13 entries triaged. **8 closed by sibling milestones** (21,
22, 23, 30, 31, 32, 34, 35). **4 backlog candidates** (26, 27, 28, 36) —
three are spec-gap re-specs that landed in M176 / M175, one is an open
bug from M170 changelog work. **1 follow-up unfixed MEDIUM** from
Entry 33 (M4 render-path test) — already partially closed by Entry 34.

---

## Per-entry notes

## Entry 22 — 2026-07-11 — M138 verification-gate prose (dupe of Entry 21) <!-- points-at: M177 -->

- **Verdict (audit):** `doc` — the log entry is a duplicate of Entry 21
  with the same M177 closure path. M177 fixed both (prose detector +
  `mp migrate manual-prefix-backfill` + write-time `prose_warning`).
- **Action:** log only. No new backlog item.

## Entry 21 — 2026-07-10 — M135 complete: AC-02 verification text trips the gate <!-- points-at: M177 -->

- **Verdict (audit):** `doc` — fixed by M177.
- **Action:** log only.

## Entry 23 — 2026-07-12 — `mp install` deploys SKILL.md only; sibling deep-dives missing <!-- points-at: M175 -->

- **Verdict (audit):** `doc` — M175 closed it via
  `deploy_skill_to_harness` + `install_project_skill` recursive
  deploy + 5 new tests + link-resolution test. Re-verified via
  `cargo nextest run -p mp -E 'test(/install_skill_link_resolution|skill_link_targets_exist/)'`.
- **Action:** log only.

## Entry 26 — 2026-07-13 — M160 ships sccache wiring; local warm-cache drop is ~19%, not ≥50% <!-- points-at: M176 -->

- **Verdict (audit):** `backlog` — M176 re-specs AC-01 to incremental
  proxy. Follow-up: pin a numeric gate via the `mp_measure!` macro.
- **Action:** covered by M176. No new backlog item; log + M176
  carry the structural fix.

## Entry 27 — 2026-07-13 — M161 ships oracle split; local wall-clock change within noise <!-- points-at: M176 -->

- **Verdict (audit):** `backlog` — M176 re-specs AC-04 to incremental
  rebuild (where the 137 → 56 dep-audit drop actually shows up). The
  same `mp_measure!` macro + perf-ac lint closes the pattern across
  M159/M160/M161.
- **Action:** covered by M176.

## Entry 28 — 2026-07-13 — M162 ships in-process test surface pilot; AC-02 + AC-04 unmet (~2%) <!-- points-at: M175 -->

- **Verdict (audit):** `backlog` — M175 lifts the deferral and lands
  the full top-5 conversion (~385 remaining spawn sites). AC-02 + AC-04
  re-spec to a ≥95% conversion metric (code-clarity, not wall-clock).
- **Action:** covered by M175.

## Entry 30 — 2026-07-15 — M169 external review: Tab/click on Settings wipes staged edits <!-- points-at: M169 (fixed in M169-rev; M174 cancelled) -->

- **Verdict (audit):** `doc` — fixed in M169-rev (see Entry 31); M174
  cancelled as redundant.
- **Action:** log only.

## Entry 31 — 2026-07-15 — M169-rev remediation: four findings from Entry 30 fixed <!-- points-at: M169 -->

- **Verdict (audit):** `doc` — 14 regression tests added in
  `crates/raul/tests/m169_rev.rs`; all green.
- **Action:** log only.

## Entry 32 — 2026-07-15 — M169-rev scrollbar fixes: `measure_paragraph_height` capped at ~2 rows <!-- points-at: M169 -->

- **Verdict (audit):** `doc` — fixed; 11 new regression tests in
  `m169_scroll_repro.rs` + `m169_rev_scrollbar.rs`.
- **Action:** log only.

## Entry 33 — 2026-07-15 — Sub-agent code review of M169-rev: one HIGH + 3 docs + 2 mediums <!-- points-at: M169 -->

- **Verdict (audit):** `backlog` for M4 (render-path partial-scroll
  test). M4 is partially closed by Entry 34 (hash-keyed cache + the
  render-path test) — that entry fixes M4 explicitly. The remaining
  unfixed LOW (8× buffer allocation) is acceptable given M134's rate
  cap.
- **Action:** covered by Entry 34.

## Entry 34 — 2026-07-15 — Sub-agent L3a + M4: hash-keyed cache + render-path partial-scroll test <!-- points-at: M169 -->

- **Verdict (audit):** `doc` — L3a + M4 closed; +3 new tests in
  `m169_rev_scrollbar.rs`.
- **Action:** log only.

## Entry 35 — 2026-07-15 — M169-rev2: scroll still doesn't reach bottom; `h` triggers `PreviousLane` <!-- points-at: M169 -->

- **Verdict (audit):** `doc` — fixed; 8 new tests in
  `m169_rev2_h_fix.rs` + 2 more in `m169_scroll_repro.rs`.
- **Action:** log only.

## Entry 36 — 2026-07-16 — `mp changelog add --version unreleased` wipes CHANGELOG history <!-- points-at: M170 -->

- **Verdict (audit):** `backlog` — open bug from M170 changelog work.
  Promote to **B-81** for follow-up regression test (the wipe path
  needs a fixture-based test that exercises append-only behavior).
- **Action:** promote to B-81 (recommended next session).

---

## Backlog promotion candidates

| New id | Source | Title | Priority |
|--------|--------|-------|----------|
| B-81 | Entry 36 | `mp changelog add --version unreleased` wipes CHANGELOG history | medium |

(All other open items are already covered by M175 / M176 / M177 — no
duplicate backlog promotions needed.)

---

## Triage methodology

1. **Read the entry's own verdict** (the "Verdict:" line) and the
   "Status (Mxxx):" line where present — closure status is the source
   of truth.
2. **Cross-reference the pointed-at milestone** (`points-at: M###` in
   the heading) — confirm the closure milestone is `lifecycle: complete`
   or has otherwise absorbed the work.
3. **Check for new findings** — if a follow-up sub-finding emerged
   from the original entry (e.g. Entry 33's M4 → Entry 34), track the
   follow-up's resolution.
4. **Promote when** the entry points at a milestone that hasn't landed
   OR the closure is partial. Promote with priority `medium` by
   default; `high` if the bug is reproducible in the shipped binary;
   `low` if it requires unusual setup.
5. **Don't promote duplicates** — entries 21 and 22 are the same bug
   recurring; both stay `doc` because M177 is the single closure.

---

## See also

- [`mp-dogfood-log.md`](../../mp-dogfood-log.md) — the source log
- [`code-review-lessons.md`](../code-review-lessons.md) — lesson
  patterns L1–L63 (L6, L8, L13, L14, L15 now have runnable
  Pattern: blocks per M173 S1)
