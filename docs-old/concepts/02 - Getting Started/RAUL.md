# RAUL — Human PM Entry Guide

**RAUL** = *Review, Approval, Unblock Loop* — the human-facing PM CLI for Master Plan.

Agents use `mp` (JSON stdout by default). You use **`raul`** for styled tables, digests, and the TUI
dashboard. raul reads plan state through `mp`; it never writes plan files directly.

**Install:** [INSTALL.md](./INSTALL.md) · **Visual index:** [00-Concepts.md § Process visuals](../00-Concepts.md#process-visuals) · **TUI keys:** [raul-tui-walkthrough.md](../raul-tui-walkthrough.md)

---

## 1. When to use raul vs mp

| You are… | Use | Why |
|----------|-----|-----|
| Human PM / developer checking status | `raul` | Tables, colors, dashboard |
| Coding agent in Cursor / OpenCode / Pi | `mp <cmd>` | JSON default — omit `--format json` |
| Quick poll while agent runs | `raul watch --interval 30` | Live status without JSON |
| “What changed since handoff?” | `raul digest --since-handoff` | Human digest; agent uses `mp plan diff --since-handoff` |

Methodology (spec lifecycle, grooming, gates) lives in concept docs — not duplicated here.
Start with [PM-WORKFLOWS.md](../03%20-%20Planning%20Methodology/PM-WORKFLOWS.md) and
[EXECUTION-MODES.md](../01%20-%20Agent%20Integration/EXECUTION-MODES.md).

---

## 2. Daily commands (5–10 min)

```bash
raul status              # mode, phase, milestone counts, next step
raul next                # head of execution queue
raul path                # ordered path + blockers
raul inbox               # items needing PM attention
raul validate            # human-readable gate report
raul watch --once        # single snapshot (good for scripts)
```

**Weekly / milestone review:**

```bash
raul milestones          # all milestones, spec/exec columns
raul show 75             # full milestone detail
raul digest --days 7     # progress over the last week
raul digest --since-handoff   # since last autonomous handoff
raul graph               # dependency graph
raul execution           # mode, handoff metadata, blockers
```

---

## 3. Command reference (by job)

### Orientation

| Command | What you get |
|---------|--------------|
| `raul onboard` | Project summary + how to get started |
| `raul explain health` | Plan health snapshot |
| `raul explain gates` | Plain-language G1–G14 + R1 |
| `raul explain why-blocked <id>` | Why milestone M-id is blocked |

### Plan browsing

| Command | What you get |
|---------|--------------|
| `raul status` | Summary tables: mode, milestones, queues, next |
| `raul milestones` | Milestone list |
| `raul show <id>` | Intent, scope, ACs, steps |
| `raul next` | Next runnable step |
| `raul path` | Suggested execution path |
| `raul graph` | Dependency nodes and edges |
| `raul backlog` | Deferred scope |
| `raul tracks` | Fast-lane bugfix/tweak items |
| `raul decisions` | Decision log |

### Review & approval

| Command | What you get |
|---------|--------------|
| `raul validate` | Validation errors/warnings grouped by code |
| `raul inbox` | Action hints (groom, review, track, decide) |
| `raul approval list` | Open approval-request annotations (G14) |
| `raul approval approve <id>` | Resolve + delegate milestone approve |

### Progress & monitoring

| Command | What you get |
|---------|--------------|
| `raul digest` | Progress digest (`--since-handoff`, `--since`, `--days`, `--open`, `--markdown`, `--out`) |
| `raul watch` | Poll status + inbox (`--interval SECS`, `--once`) |
| `raul execution` | Execution mode and path preview |

### Writes (delegated to mp)

| Command | Notes |
|---------|-------|
| `raul idea <title>` | Park an idea |
| `raul annotation create …` | Create annotation on milestone/step |

---

## 4. Dashboard & TUI

**Default entry:** run `raul` with no subcommand (or `raul -i`).

The **home dashboard** shows planning status, inbox count, pending reviews, path preview,
and next action. Keys:

| Key | Action |
|-----|--------|
| `m` / Enter | Milestones list |
| `t` | Tracks list |
| `b` | Backlog list |
| `r` | Refresh data |
| `?` | Help overlay |
| `q` / Esc | Quit (or back) |

From milestones you can drill into detail, annotation threads, and co-approval flows.
Full key map: [raul-tui-walkthrough.md](../raul-tui-walkthrough.md).

---

## 5. Workflow links (read next)

| Topic | Doc |
|-------|-----|
| PM cadences & intake funnel | [PM-WORKFLOWS.md](../03%20-%20Planning%20Methodology/PM-WORKFLOWS.md) |
| Planning vs autonomous handoff | [EXECUTION-MODES.md](../01%20-%20Agent%20Integration/EXECUTION-MODES.md) |
| Handoff + plan diff sequence | [EXECUTION-MODES §5.1](../01%20-%20Agent%20Integration/EXECUTION-MODES.md#51-handoff-sequence-m70-baseline) |
| End-to-end OAuth example | [WALKTHROUGH.md](../03%20-%20Planning%20Methodology/WALKTHROUGH.md) |
| Agent execute → review → remediate | [AGENT-PLAYBOOK §9](../01%20-%20Agent%20Integration/AGENT-PLAYBOOK.md#9-process-diagrams) |
| Install & harness wiring | [INSTALL.md](./INSTALL.md) |
| All diagram locations | [00-Concepts visual index](../00-Concepts.md#visual-index-shipped-flow-diagrams) |

---

## 6. mp → raul quick map

For agents summarizing JSON to humans, or when you need the agent counterpart:

| Agent (`mp`) | Human (`raul`) |
|--------------|----------------|
| `mp status` | `raul status` |
| `mp list milestones` | `raul milestones` |
| `mp show milestone <id>` | `raul show <id>` |
| `mp next` | `raul next` |
| `mp path` | `raul path` |
| `mp digest --since-handoff` | `raul digest --since-handoff` |
| `mp validate` | `raul validate` |
| `mp inbox` | `raul inbox` |

Full mapping: [MP-COMMANDS.md § Human surface](../06%20-%20Reference/MP-COMMANDS.md) (top of file).
