# hybrid-work fixture

Simulates a **work repository** using the **hybrid** adoption profile:

- Plan lives in **`.mp/`** (gitignored), not `master-plan/`
- **Tracks** + **ideas** for daily fast lane
- **Session** `feature-oauth` under `sessions/` for branch-scoped spec (P3.1 target layout)

## Layout

```text
hybrid-work/
├── .gitignore              # .mp/
├── AGENTS.md
└── .mp/
    ├── config.toml         # workflow.profile = hybrid
    ├── plan.toml           # minimal charter
    ├── ideas.toml
    ├── tracks/
    ├── sessions/feature-oauth/
    │   ├── session.toml
    │   └── milestone.toml
    └── archive/
```

## Scenarios

| Scenario | Phase | Command |
|----------|-------|---------|
| [p3.1-hybrid-work-validate](../../scenarios/p3.1-hybrid-work-validate/) | implemented | `mp --plan-dir .mp validate` |
| [p3.1-hybrid-work-next-track](../../scenarios/p3.1-hybrid-work-next-track/) | implemented | `mp --plan-dir .mp next-step` |
| [p3.1-hybrid-work-session-show](../../scenarios/p3.1-hybrid-work-session-show/) | planned | `mp session show` |

## Rust today vs target

| Behavior | Today | Notes |
|----------|-------|-------|
| Plan dir | Auto-resolves `.mp/` | `config.workflow.plan.location` |
| `mp session *` | ✅ | `sessions/<id>/` tree (ADR-010) |
| `mp idea *` | ✅ | `ideas.toml` loaded |
| Session in `path` / `next-step` | ⚠️ M08 | Use `mp session show` until shipped |

See [docs/ADOPTION-PROFILES.md](../../../docs/ADOPTION-PROFILES.md) and [docs/ADOPTION-CHECKLIST.md](../../../docs/ADOPTION-CHECKLIST.md).
