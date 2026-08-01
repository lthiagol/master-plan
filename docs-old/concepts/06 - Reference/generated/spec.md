# `mp spec`

**Condensed spec-review surface (M80): review-oriented projection + since-last-approval spec diff**

**Usage:**

```text
Usage: spec <COMMAND>
```

**Subcommands:**

| Name | Description |
|------|-------------|
| `review` | Condensed review-oriented projection of a milestone spec (outcome, problem, scope, ACs with coverage + evidence + force-bypass, open questions, coverage gaps). Reuses M79 --fields for slicing |
| `diff` | What spec fields changed since the milestone's last approval (review). Anchors on git history of the milestone file |

