# `mp edit`

**Plan-shape mutations (M105 / B-41): bulk edits to milestone files that don't fit cleanly under `milestone add|update|…` because they touch many files at once. Currently only `strip-dropped-keys`**

**Usage:**

```text
Usage: edit <COMMAND>
```

**Subcommands:**

| Name | Description |
|------|-------------|
| `strip-dropped-keys` | M105 S4 (B-41): one-shot utility that strips every key in `milestone::DROPPED_CEREMONY_KEYS` from every milestone file in the plan. Idempotent — re-running on a clean plan is a no-op (no file is rewritten when no key matches). Pair with `mp validate --summary` to confirm post-run counts are 0/0 |
| `migrate-lifecycle` | M100 S10: bulk-migrate every milestone in the plan from the legacy `spec_status` + `execution_status` shape to the unified `lifecycle` field. Idempotent — re-running on an already-migrated plan is a no-op. Use `--dry-run` first to preview; `--yes` is required to actually write. After running, `mp validate` should exit 0; if it doesn't, the migration surfaces new gate failures (rare; lifecycle value may differ from legacy-derive) — review and re-validate |
| `strip-deferred-reason` | M177 S8: clear stale `deferred_reason` text where `deferred: false` |

