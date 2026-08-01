# mp — Schema and Storage Migrations

This file records every schema-level change to the `mp` toolchain's data model.
Each entry includes the commit(s) that introduced the change, what was before,
what is after, and the rationale.

Entries are ordered chronologically (newest first).

---

## 2026-07 — Milestone ID schema widening (2-or-more digits, no upper bound)

- **Commits:** `3ebaee4`, `dc6052e`
- **Before:** `^[0-9]{1,2}$` — milestone IDs were capped at 2 digits (range 00–99).
- **After:** `^[0-9]{2,}(\.[0-9]+)*$` — milestone IDs must be at least 2 digits, with
  no upper bound. Optional sub-ids (e.g. `100.1`) are supported for split milestones.
- **Rationale:** The 2-digit cap is a development-era artifact. As the project
  crosses milestone 100, the schema must accommodate 3+ digit IDs without a code
  change. The `2,` quantifier in `{2,}` means "2 or more digits" — we never
  revert to 1-digit IDs (they were a source of ambiguity early on), so the floor
  stays at 2. The optional `(\.[0-9]+)*` segment supports milestone splits
  (e.g. M100.1, M100.2) without an ad-hoc convention.
- **Impact:** Validation schemas, ID parsing in `store.rs`, and any regex-based
  milestone ID filters must use the new pattern. Milestones with 1-digit IDs
  (00–09) are no longer valid; project migration (`mp edit migrate-lifecycle` or
  equivalent) handles these cases.

---

## Template for future entries

```markdown
## YYYY-MM — Short title

- **Commits:** `<sha1>`, `<sha2>`
- **Before:** `<old state>`
- **After:** `<new state>`
- **Rationale:** `<why>`
- **Impact:** `<what downstream code/docs need updating>`
```
