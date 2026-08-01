# `mp review`

**M173 S4: write a hunk-compatible agent-context sidecar at the given path, listing the milestone's findings (optionally scoped to one `F-NN`) + comments as inline annotations. hunk loads the sidecar at startup via `hunk diff --agent-context <path>`; the file is not hot-reloaded**

M173 S4: write a hunk-compatible agent-context sidecar at the given path, listing the milestone's findings (optionally scoped to one `F-NN`) + comments as inline annotations. hunk loads the sidecar at startup via `hunk diff --agent-context <path>`; the file is not hot-reloaded.

Singular `review` (vs the existing plural `reviews`) is the documented surface per the M173 spec. `--finding F-XX` filters to one finding; without it, every open finding on the milestone is exported.

**Usage:**

```text
Usage: review <COMMAND>
```

**Subcommands:**

| Name | Description |
|------|-------------|
| `sidecar` | Write a hunk-compatible agent-context sidecar at `--output`. Loads via `hunk diff --agent-context <path>`; not hot-reloaded. `--finding F-XX` filters to one finding; default exports every finding on the milestone |

