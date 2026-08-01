# Agent instructions

This fixture simulates a **work repo** with a gitignored plan directory.

- Plan zone: `.mp/` (not `master-plan/`) — auto-resolved by `mp`
- Profile: `hybrid` — tracks + ideas + session per branch
- See [docs/ADOPTION-PROFILES.md](../../../../docs/ADOPTION-PROFILES.md)
- Adoption walkthrough: [docs/ADOPTION-CHECKLIST.md](../../../../docs/ADOPTION-CHECKLIST.md) §3

```bash
mp validate
mp track list
mp session show feature-oauth
```
