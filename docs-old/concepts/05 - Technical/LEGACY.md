# Legacy Markdown + Bash Workflow

Master Plan **used to** be markdown files (`STATUS.md`, `milestones/*.md`) edited by
agents and a Bash `bin/master-plan` script.

**Superseded by:** JSON on disk + Rust `mp` CLI + `master-plan/AGENTS.md`.

---

## Do not use for new projects

| Legacy | Current |
|--------|---------|
| `instructions.md` | [templates/AGENTS-TEMPLATE.md](../templates/AGENTS-TEMPLATE.md) |
| `master-plan/STATUS.md` | `mp status`, `plan.json` |
| `milestones/NN-slug.md` | `milestones/NN-slug.json` |
| `bin/master-plan` | `mp` ([crates/mp](../crates/mp)) |
| Hand-edit plan files | `mp` commands only |

---

## Bash CLI reference (archival)

Kept in [README.md §2](../README.md#2-legacy-bash-cli-binmaster-plan) for command parity
during migration. No new features will be added to Bash.

---

## Migration notes (markdown → JSON)

1. `mp init` in the target repo
2. For each legacy milestone: `mp milestone create --json @-` from parsed markdown
3. `mp validate`
4. Archive old `STATUS.md` / `*.md` milestones outside `master-plan/` or under `archive/`

**No automated migrator yet** — planned P3 optional `mp import legacy`.

---

## References

- [SPEC.md](./SPEC.md)
- [instructions.md](../instructions.md) (bannered superseded)
