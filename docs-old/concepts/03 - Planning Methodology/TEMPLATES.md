# Templates

Templates serve two roles in master-plan:

1. **Defaults** (`templates/defaults/`) — empty JSON structures copied or merged by `mp init`
2. **Views** (`templates/views/`) — **deprecated** (v2.0); human display moved to `raul`

Agents write via `mp` + JSON. Humans read via `raul`.

---

## File map

```text
templates/
├── defaults/
│   ├── plan.json
│   ├── brief.json             # bootstrap brainstorm (mp init --profile full)
│   ├── milestone.json
│   ├── track.json
│   ├── ideas.json             # quick captures (mp idea *)
│   ├── backlog.json
│   ├── decisions.json
│   ├── config.full.json       # mp init --profile full
│   ├── config.hybrid.json     # mp init --profile hybrid
│   ├── config.session.json    # mp init --profile session
│   ├── session.json           # session scope metadata
│   ├── challenge.json         # challenge skeleton (mp challenge start)
│   └── spec-domain.json       # domain truth seed (brownfield)
├── views/
│   ├── charter.md
│   ├── brief.md               # brief human view (mp export)
│   ├── milestone-list.md      # deprecated view (v2.0; use raul milestones)
│   ├── milestone.md
│   ├── track.md
│   ├── ideas.md               # ideas list human view (mp export)
│   ├── plan-status.md
│   └── backlog.md
├── AGENTS-TEMPLATE.md
├── ROOT-AGENTS-SNIPPET.md
├── skills/
│   └── master-planner/
│       └── SKILL.md           # → ~/.agents/skills/master-planner/ on install
└── harness/                   # optional project snippets (P0.9 / P3.1)
    ├── cursor/
    │   ├── skills/master-planner/
    │   └── rules/master-plan.mdc
    └── opencode/
        └── skills/master-planner/
```

Per-project challenge sessions live in `master-plan/reviews/challenges/` (not copied from templates).

ID rules for milestones (`03`, `03.1`) and steps (`S1`, `S3.1`): [IDS.md](../docs/IDS.md).

---

## Brief template → topic mapping

Built-in topics are seeded from `templates/defaults/brief.json` at `mp init`.
See `brief` in `schemas/interview-checklist.json` for interview rounds.

| Topic key | Required for `done` | Prompt theme |
|-----------|---------------------|--------------|
| `problem` | yes | Why this project exists |
| `audience` | yes | Who it's for (and not for) |
| `capabilities` | yes | Rough feature brainstorm |
| `constraints` | yes | Stack, time, platform limits |
| `inspiration` | no | References and similar products |
| `unknowns` | yes | Open questions, research needed |
| `success` | yes | Vague success criteria OK |
| `non_starters` | yes | Probably not v1 |

---

## Track template → interview mapping

| Template field | Required to start | Interview question |
|----------------|-------------------|-------------------|
| `title` | yes | (from user request) |
| `problem` | yes | What is broken or wrong? |
| `verification` / `done_when` | yes | How do we verify the fix? |
| `steps` | yes (min 1) | What steps/files are involved? |

See `track_item` in `schemas/interview-checklist.json`.

---

## Config profile presets (P3.1)

Copied to project `config.json` by `mp init --profile <name>`:

| File | Profile | Use |
|------|---------|-----|
| `config.full.json` | `full` | Personal — full pipeline, plan in repo |
| `config.hybrid.json` | `hybrid` | Work — tracks + ideas + session milestones, gitignored `.mp/` |
| `config.session.json` | `session` | Single branch/PR scope only |

See [ADOPTION-PROFILES.md](./ADOPTION-PROFILES.md).

---

## Harness templates (P0.9)

Optional project files for Cursor / OpenCode — installed by `mp init --with-*-skill` or copied manually.

| Path | Purpose |
|------|---------|
| `templates/harness/cursor/skills/master-planner/` | Pointer README → canonical skill |
| `templates/harness/cursor/rules/master-plan.mdc` | Optional Cursor rule (off by default) |
| `templates/harness/opencode/skills/master-planner/` | Pointer README → canonical skill |

Install guide: [INSTALL.md](./INSTALL.md).

---

## Ideas template

No interview. **Create requires `title` only.**

| Field | Required | Purpose |
|-------|----------|---------|
| `title` | yes | Short label |
| `body` | no | Notes, context from conversation |
| `tags` | no | Filter/group (`installer`, `ux`, …) |
| `source` | no | `conversation` (default), `planning`, `review` |

Promotion copies content into milestone, backlog, or track. See `schemas/idea.schema.json`.

---

## Archive metadata

`archive/meta.json` is maintained by `mp archive` commands:

```json
[[entries]]
entity_type = "milestone"
entity_id = "03"
original_path = "milestones/03-oauth.json"
archived_path = "archive/milestones/03-oauth.json"
archived_at = "2026-06-17T12:00:00Z"
```

Track items use inline `status = archived` in track files; meta may still log the event.

---

## View templates

View templates use Tera syntax (`{{ variable }}`, `{% for %}`, `{% if %}`). `mp` renders
them when producing human output in `raul`. Views are read-only — editing rendered output
has no effect.
