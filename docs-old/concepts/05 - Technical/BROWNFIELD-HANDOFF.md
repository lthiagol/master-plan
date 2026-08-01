# Brownfield Handoff

How to bootstrap Master Plan into an **existing project** with code already in place.

## Quick path

```text
1. mp init --profile full --from-repo
2. mp doctor
3. mp brief todo
4. (fill brief with user)
5. mp brief done
6. mp interview checklist --checklist-type charter
7. (fill charter)
8. mp interview checklist --checklist-type milestone --draft
```

## What `--from-repo` does

- Detects project name from `Cargo.toml`, `package.json`, or `pyproject.toml`
- Detects stack (Rust, TypeScript, Python, etc.)
- Detects brownfield likelihood (existing `src/`, `tests/`)
- Pre-fills `plan.json` project.name, project.description, project.stack
- Sets `planning_phase = charter` for brownfield repos

## Handoff mapping flow

When you inherit a project without specs, map the existing code to plan artifacts:

| Existing artifact | Maps to |
|-------------------|---------|
| `README.md` install instructions | `brief.json` topics |
| Existing tests | Validation scenarios |
| CI workflows | AC verification methods |
| `CONTRIBUTING.md` conventions | Charter principles |
| Open issues / bugs | Track items (`bugfix`) |
| Feature requests | Ideas → promote to milestones |
| Architecture docs | Domain specs (`specs/`) |

## Brownfield interview prompts

Ask the user:

1. "What does this codebase do? One sentence."
2. "What are the top 3 pain points right now?"
3. "What should NOT change (what's stable)?"
4. "What tests exist and what do they cover?"
5. "Are there any undocumented conventions?"

## Delta milestones (P4)

For behavior changes in existing code, use `change_kind: delta`:

```text
mp specs init api    # register domain
mp milestone create --json @-  # with change_kind=delta, delta.domain=api
```

Delta milestones track ADDED/MODIFIED/REMOVED per domain and merge on `milestone complete`.

Until P4 ships, use greenfield milestones with explicit before/after in the spec.

## References

- [BROWNFIELD.md](./BROWNFIELD.md) — greenfield vs brownfield routing
- [ADOPTION-CHECKLIST.md](./ADOPTION-CHECKLIST.md)
- [AGENTS.md](../AGENTS.md) — session start
