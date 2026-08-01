# Storage Format Decision

**Decision:** JSON on disk **and** JSON at the CLI boundary — one `serde_json` path
through the `mp::store` layer. Plan artifacts are `.json` at rest; agents read/write
the same shape `mp` persists.

---

## Why JSON on disk

JSON is canonical at rest because the product is agent-first. Humans interact through
**raul** (TUI) or a prose summary, never by hand-editing plan files. Since JSON Schema
already validates agent I/O at the CLI boundary, persisting that exact payload removes
a second (TOML) serialization surface and the drift risk it carries.

| Criterion | JSON on disk (chosen) | (Prior: TOML on disk) |
|-----------|----------------------|------------------------|
| Agent fidelity | **Exact** — disk == CLI shape, no re-encoding | Re-encoded to JSON on read; two surfaces to keep in sync |
| Schema validation | One JSON Schema validates both I/O and at-rest | Needed a TOML→JSON hop before validation |
| Rust ecosystem | `serde_json` — first-class, one dependency | `toml` + `serde_json` — two deps |
| Multiline text (outcomes, evidence) | Escaped `\n` (fine — never hand-edited) | Native `"""` blocks (human nicety, unused now) |
| Diff-friendly | Good — stable key order via serde | Good for small files |

**Verdict:** JSON is canonical on disk. Hand-editing is forbidden regardless of
format — agents write through `mp`, humans use raul.

## Why JSON at the CLI boundary

- Agents parse JSON reliably in every harness
- Same struct (`serde_json`) reads and writes — one serialization path, no format split
- Schema validation via `schemars` / JSON Schema validates the exact bytes persisted
- Piping: `mp milestone create --json @-`

## CLI output modes (v2.0+)

| Mode | Flag | Audience | Content |
|------|------|----------|---------|
| **agent** | _(default, `--format json`)_ | Coding agents | JSON — structured, schema-validated |
| **debug raw** | `--format raw` | Debugging | Verbatim on-disk JSON passthrough (`show milestone`, `track show`) or GraphViz DOT (`graph`) |

Default: **JSON always** — omit `--format` on read commands.

```bash
mp show milestone 03              # JSON (agent contract) — same path mp writes
mp show milestone 03 --format raw   # verbatim on-disk JSON (debug)
mp graph --format raw             # GraphViz DOT (debug)
```

> **Note (M92):** the `--format toml` value was removed. The debug escape hatch is now
> `--format raw` (verbatim on-disk JSON passthrough, or DOT for `graph`). There is no
> longer a separate "lean" view for `show milestone` — default JSON serializes the loaded
> `MilestoneFile` struct directly, identical to the write path.

Human display lives in **raul**, not mp.

## Migration & legacy snapshot

- **One-time conversion:** `mp::migrate` (milestone M92) reads every `*.toml` under a
  plan dir, converts each to an equivalent `*.json`, and removes the original. Run once
  over the dogfood plan and fixtures; not in the hot path.
- **`toml` crate** remains a dependency **only** for `Cargo.toml` parsing (`mp::install`)
  and the one-time `mp::migrate` module — not for plan artifacts.
- **Frozen rollback reference:** a pre-M92 TOML snapshot is preserved at the repo-root
  `legacy-toml/` directory, outside the plan dirs. It is **never loaded by `mp`**; it
  exists solely as a rollback/diff reference.
