---
name: mp-flow
description: Test fixture for the mp-flow SKILL.md / stages.toml lint. Canonical pair — this one must pass.
---

# mp-flow — fixture (in sync with stages.toml)

## Role-binding table

| Stage | Name | Owner |
|-------|------|-------|
| 1 | Draft | coordinator |
| 2 | Groom | coordinator |
| 3 | Specify | coordinator |
| 4 | Approve | coordinator |
| 5 | Claim & execute | runner |
| 6 | Self-review | runner |
| 7 | Complete | runner |
| 8 | External review | coordinator |
| 9 | Remediate | runner |
| 10 | Re-review | coordinator |
| 11 | Document | coordinator |
| 12 | Hand off | coordinator |

Stages 1-4 are the coordinator's domain. Stages 5-7 and 9 are the
runner's. Stages 8-10 form the review loop.

## Stage ownership

The coordinator owns stages 1, 2, 3, 4, 8, 10, 11, 12. The runner owns
stages 5, 6, 7, 9.

## Draft

```
mp interview checklist
```

## Groom

```
mp milestone groom
```

## Specify

```
mp milestone set-spec-status <id> ready
```

## Approve

```
mp milestone approve <id>
```

## Claim & execute

```
mp milestone set-status <id> in-progress
```

## Self-review

```
mp reviews finding add <id> --phase self
```

## Complete

```
mp milestone complete <id>
```

## External review

```
mp reviews finding list <id>
```

## Remediate

```
mp reviews finding resolve <id> <finding-id>
```

## Re-review

```
mp milestone verify <id>
```

## Document

```
mp note add --title "..."
```

## Hand off

```
git commit -m "..."
```
