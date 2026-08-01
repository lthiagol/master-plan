# `mp agent`

**Usage:**

```text
Usage: agent <COMMAND>
```

**Subcommands:**

| Name | Description |
|------|-------------|
| `role` |  |
| `harness` | M151: query the harness command registry (a single source of truth for the harness binaries `mp watch` invokes via `herdr agent start`). Subcommands: - `list` — enumerate every v1 entry. - `start-command <name>` — print the argv the registry would build for a given harness (with optional --model and --thinking-level overrides). Useful for previewing what `mp watch` would invoke without running it |

