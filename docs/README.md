# Master Plan — Documentation

**Master Plan** is a spec-driven planning layer for software projects and coding
agents. Agents drive the plan through the **`mp`** CLI; humans read it through the
**`raul`** terminal UI. The plan itself lives in a directory of JSON files that
are owned by `mp` — they are never hand-edited.

| Tool | Audience | Default output | Purpose |
|------|----------|----------------|---------|
| `mp`  | Agents (and anyone scripting) | JSON | Create, mutate, and read the plan |
| `raul` | Humans | Styled terminal UI | Browse, review, and triage the plan read-only |

> **The golden rule:** the plan directory is *mediated*. Every read and every
> write goes through `mp`. Editing plan files by hand defeats validation and
> leaves no audit trail.

## What lives here

This folder is the end-user reference for the toolkit. Each topic has its own
subfolder with a primary `README.md` and, where useful, deeper reference files.

| Folder | Covers |
|--------|--------|
| [`mp/`](./mp/) | Using the `mp` CLI — global flags, output conventions, command reference, project config |
| [`raul/`](./raul/) | Using the `raul` terminal UI — lanes, key bindings, themes, settings |
| [`milestone-lifecycle/`](./milestone-lifecycle/) | The milestone state machine: planning → execution → review, plus overlays and remediation |
| [`milestone-details/`](./milestone-details/) | The anatomy of a milestone document — every field and what it is for |
| [`skills/`](./skills/) | The skills that ship with the toolkit and how they are deployed |
| [`agent-guide/`](./agent-guide/) | **For agents.** A load-once orientation doc plus on-demand detail files for each workflow |

## Where to start

- **You are a human who wants to look at a plan** → install, then run `raul`. See [`raul/`](./raul/).
- **You are setting up a project for the first time** → [`mp/getting-started.md`](./mp/getting-started.md).
- **You want to understand what a milestone is made of** → [`milestone-details/`](./milestone-details/).
- **You are an agent that will drive the CLI** → [`agent-guide/README.md`](./agent-guide/README.md).

## Intake at a glance

Not every change deserves a full milestone. `mp` offers separate intake surfaces
sized to the work:

| Change shape | Reach for | Commitment |
|---|---|---|
| One-line bug or tweak with a verification command | `mp track add …` | minutes |
| Multi-step feature with shared acceptance criteria | `mp milestone create …` | days |
| Vague "someday" idea | `mp idea create …` | open note, no commitment |
| Concrete but not-now work | `mp backlog add …` | prioritized, promotable later |

See [`milestone-lifecycle/`](./milestone-lifecycle/) for how milestones move,
and [`mp/commands.md`](./mp/commands.md) for the full command surface.
