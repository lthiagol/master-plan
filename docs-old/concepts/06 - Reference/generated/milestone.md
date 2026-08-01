# `mp milestone`

**Usage:**

```text
Usage: milestone <COMMAND>
```

**Subcommands:**

| Name | Description |
|------|-------------|
| `create` |  |
| `approve` |  |
| `update` |  |
| `set-spec-status` |  |
| `set-lifecycle` | Set the canonical lifecycle field directly (post-M100). Mutates `lifecycle` and derives the `spec_status` / `execution_status` legacy aliases so on-disk files stay internally consistent. Use `--dry-run` to preview; pass `""` to reset to a blank lifecycle |
| `set-priority` |  |
| `set-status` |  |
| `set-target-version` | Set the target release version for a milestone |
| `complete` |  |
| `verify` |  |
| `criterion` |  |
| `ac` | Agent-friendly short alias for `criterion` (M93). Lets agents say `mp milestone ac show 87 AC-03` without loading the whole document |
| `decompose` |  |
| `plan` |  |
| `block` |  |
| `unblock` |  |
| `defer` |  |
| `reopen` |  |
| `split` |  |
| `question` |  |
| `delete` |  |
| `archive` |  |
| `restore` |  |
| `purge` |  |
| `groom` |  |
| `trace` |  |
| `log` | M112 S4: read-only milestone history log. Emits created/updated from the on-disk milestone plus a `commits` array of `git log --oneline` for the milestone file. No on-disk writes |
| `dependents` | List milestones that depend on <ID> (reverse deps) |
| `deps` | List milestones that <ID> depends on (forward deps) |
| `impact` | Transitive blast radius: recursive reverse deps + path pins + ordering implications |
| `list-pending-review` | List milestones pending code review (spec_status=implemented, code_review=true) |
| `challenge` |  |
| `step` |  |
| `wp` |  |
| `design-decision` |  |
| `bulk` | Bulk milestone metadata operations (M94). Targets resolve via --ids and/or --where (same filter syntax as `list milestones`). Sequential execution with per-id result reporting; --dry-run previews mutations |

