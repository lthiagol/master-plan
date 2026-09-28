---
name: mp-flow
description: Test fixture for the mp-flow SKILL.md / stages.toml lint. Deliberately drifted — the role-binding table must be rejected.
---

# mp-flow — fixture (role-binding table drifted from stages.toml)

Every `## <name>` section and every `mp` command still matches the
manifest, so the only thing wrong here is the role-binding table. That
isolates the check under test: a rename in the table alone must fail the
lint, exactly as it did silently before.

The drift mirrors the shape that shipped: seven Name cells reworded away
from the manifest's canonical names, plus one Owner cell flipped
(stage 6). All 12 rows are still present, so the row count agrees and
only the cell values are wrong.

## Role-binding table

| Stage | Name | Owner |
|-------|------|-------|
| 1 | Define outcome | coordinator |
| 2 | Interview & shape | coordinator |
| 3 | Write acceptance | coordinator |
| 4 | Approve spec | coordinator |
| 5 | Claim & execute | runner |
| 6 | Self-review | coordinator |
| 7 | Mark complete | runner |
| 8 | External review | coordinator |
| 9 | Remediate findings | runner |
| 10 | Re-review | coordinator |
| 11 | Document | coordinator |
| 12 | Hand-off | coordinator |

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
