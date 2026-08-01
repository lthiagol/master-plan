# ID Strategy — Hierarchical Outline IDs

Canonical rules for milestone, step, and related identifiers across Master Plan.

**Status:** Adopted (documented). Rust implementation follows this spec.

---

## 1. Principles

1. **Stable parent on split.** When work is split, the parent keeps its ID; children get
   decimal suffixes (`S3` → `S3`, `S3.1`, `S3.2`). Siblings are **not** renumbered.
2. **Prefix disambiguates entity type.** `M` milestone, `S` step, `T` brief topic, etc.
3. **Two layers.** On-disk canonical `id` fields + display labels in CLI human output.
4. **Decimals mean “derived from parent.”** Not “insert between” unrelated items.
5. **Natural sort.** `S3 < S3.1 < S3.2 < S3.10 < S4` (numeric segment comparison).

---

## 2. Milestones (`M`)

### On disk

| Field | Format | Example |
|-------|--------|---------|
| `milestone.id` | `NN` or `NN.N…` | `03`, `03.1`, `03.2` |
| Filename | `milestones/<id>-<slug>.json` | `03-oauth.json`, `03.1-oauth-ui.json` |

Top-level milestones use two-digit zero-padded IDs (`01`, `02`, `03`).  
**Split children** use decimal extensions: `03.1`, `03.2` (derived from `03`).

### Display (raul / human)

| On disk | Display |
|---------|---------|
| `01` | `M1 — Title` or `M01 — Title` (project config `milestone_prefix`) |
| `03` | `M3 — OAuth Login` |
| `03.1` | `M3.1 — OAuth UI` |

### Split semantics

Before: `M3` (one milestone).  
After split: `M3` (reduced scope) + `M3.1` + `M3.2` (slices).

- **M3** keeps remaining scope.
- **M3.1**, **M3.2** are new milestone files with `depends_on` pointing at parent chain.
- Use next integer **M4** only for **new** work, not a slice of M3.

```bash
mp milestone split 03
mp milestone split 03 --into 2 --titles "OAuth core,OAuth UI"
# → 03 (trimmed), 03.1, 03.2
```

### `depends_on`

References use on-disk ids: `depends_on = ["02", "03"]` or `depends_on = ["03"]` for `03.1`.

---

## 3. Steps (`S`)

Steps are **scoped to one milestone**. `S1` in M1 and `S1` in M2 are different steps.

### On disk

Stored in milestone file as top-level `[[steps]]` (**canonical**, [ADR-001](./DECISIONS.md#adr-001-steps-on-disk)).
Work packages are grouping metadata only — step IDs do **not** include WP numbers.

**Transitional:** Until P1 Rust migrates, `mp` also **reads** legacy `[[work_packages.steps]]`
and merges into the step list. New writes use top-level `[[steps]]` only. Test fixtures may
still use nested form until updated.

```json
[[work_packages]]
id = "WP1"
name = "OAuth endpoints"
goal = "Wire GitHub OAuth flow"
rollback = "git restore src/auth/"

[[steps]]
id = "S1"
work_package = "WP1"
order = 1
action = "Add OAuth config schema"
files = ["src/auth/config.rs"]
tests = "cargo test oauth_config"
done_when = "Config tests pass"
status = "done"
covers_ac = ["AC-01"]

[[steps]]
id = "S2"
work_package = "WP1"
order = 2
action = "Implement callback handler"
status = "in-progress"
```

| Field | Format | Example |
|-------|--------|---------|
| `steps[].id` | `S` + outline number | `S1`, `S3`, `S3.1`, `S3.2` |
| `steps[].work_package` | `WPn` | `WP1` (optional but recommended) |
| `steps[].order` | integer | explicit sort key when needed |

### Display

Same as on-disk id: `S1`, `S3.1`. In context: `M3 / S3.1`.

### Split semantics

Before: `S1`, `S2`, `S3`.  
**S3** is too large.

After:

```text
S1, S2, S3, S3.1, S3.2
```

- **S3** retains the portion that still belongs under the original step.
- **S3.1**, **S3.2** hold the carved-out work.
- **S4+** unchanged.

```bash
mp step split 03 S3
mp step split 03 S3 --json @-    # agent supplies bodies for S3, S3.1, S3.2
```

Further split of **S3.1** → `S3.1`, `S3.1.1`, `S3.1.2` (arbitrary depth allowed).

### CLI commands use step ids

```bash
mp step update 03 S3 --action "..."
mp step set-status 03 S3.1 in-progress
mp list steps --milestone 03
```

---

## 4. Other entities (flat IDs)

Use **flat numbered IDs** — no decimal hierarchy except milestones and steps.

| Prefix | Entity | Example | Splittable? |
|--------|--------|---------|-------------|
| `T` | Brief topic | `T01` | no |
| `AC` | Acceptance criterion | `AC-01` | no |
| `SC` | Scenario | `SC-01` | no |
| `FR` | Functional requirement | `FR-01` | no |
| `F` | Challenge finding | `F-01` | no |
| `BF` / `TW` | Track item | `BF-01` | no |
| `ID` | Idea | `ID-01` | no |
| `B` | Backlog | `B-01` | no |
| `WP` | Work package | `WP1` | no (grouping only) |

---

## 5. Challenge / review targets

Machine-readable `target` strings:

| Target | Meaning |
|--------|---------|
| `milestone` | whole milestone |
| `milestone:03.1` | specific milestone |
| `step:S3` | step in current challenge milestone |
| `step:S3.1` | explicit step |
| `ac:AC-01` | acceptance criterion |
| `wp:WP1` | work package grouping |

When challenge is scoped to milestone `03`, `step:S3` is shorthand (milestone implied).

---

## 6. Sorting algorithm

Apply **outline sort** to the numeric part after the prefix:

1. Split id into segments: `S3.1.2` → `[3, 1, 2]`.
2. Compare segment by segment as integers.
3. Shorter prefix wins if one is a prefix of the other (`S3` before `S3.1`).

Milestone ids: `03` → `[3]`, `03.1` → `[3, 1]`, `04` → `[4]`.

---

## 7. Auto-assignment

| Entity | Rule |
|--------|------|
| Milestone | Next integer `NN` (top-level). Splits get next decimal under parent. |
| Step | Next integer `Sn` within milestone; split adds `Sn.1`, `Sn.2`. |
| WP | `WP1`, `WP2`, … |

---

## 8. Migration note (from WP.step numbering)

**Deprecated:** step ids like `1.1`, `1.2` (work-package-local numbering).  
**Canonical:** `S1`, `S2`, … with `work_package = "WP1"` metadata.

---

## 9. References

- [SPEC.md](./SPEC.md) — data model
- [GROOMING.md](./GROOMING.md) — split & challenge flows
- [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md) — CLI examples
- [../schemas/milestone.schema.json](../schemas/milestone.schema.json)
