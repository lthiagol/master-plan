# `mp docgen`

**M173 S3: walk the clap Command tree and emit markdown tables for each command group under `docs/concepts/06 - Reference/generated/`. The generator covers description, usage, options, and subcommands; `<!-- mp:include <fragment> -->` markers in `MP-COMMANDS.md` / `AGENT-READINESS.md` resolve to one of those generated files at build time**

**Usage:**

```text
Usage: docgen [OPTIONS]
```

**Options:**

| Flag | Description |
|------|-------------|
| `--out` | Output directory for the generated markdown bundle. Default: `<plan_dir>/../docs/concepts/06 - Reference/generated/` (i.e. `<project_root>/docs/concepts/06 - Reference/generated/`) |
| `--group` | Only emit a single named command group (e.g. `milestone`, `reviews`). Default emits every group |

