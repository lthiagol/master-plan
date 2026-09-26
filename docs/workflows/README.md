# Workflows

Concrete recipes for using master-plan day-to-day. The companion to
[`../agent-guide/`](../agent-guide/) — which is written *for the agent* so it
can orient itself and pick the right fragment commands — this folder is written
*for the human* who prompts the agent.

After install and harness restart (see the main README's §5–§6), you don't
typically run `mp` commands yourself — you prompt the agent in natural language
and the agent drives the toolkit. The patterns below show what good prompts
look like for the situations you'll hit most often.

## The general shape of any prompt

A good prompt for master-plan usually has three things:

1. **The intent** — what you're trying to achieve. *"Ship user signup."*
2. **The lifecycle stage** — where you want the agent to operate.
   *"…and groom the scope first"*, *"…and start executing"*, *"…and approve it"*.
3. **The seam** — what you want the agent to surface back to you.
   *"Tell me when AC verification has evidence."*, *"Open the TUI when you're done."*

You don't need those exact words. The agent reads intent. But having all three
in mind produces clearer results than dropping the agent into the project with
no signal about stage or seam.

## Common scenarios

### First-time setup — go from a fresh install to a running plan

1. Prompt: *"Initialize master-plan in this folder."*
2. The agent runs `mp init`, scaffolds the `master-plan/` directory, and reports
   back.
3. Prompt: *"Open the TUI and show me the empty plan."*
4. You're ready to capture your first milestone.

### Capturing a milestone — the human side

1. Prompt: *"Create a milestone for shipping user signup."*
2. The agent asks follow-up questions (or composes from what you already said).
3. The agent drafts acceptance criteria and suggested steps.
4. You review and pick one:
   - *"Looks good. Approve it."*
   - *"Refine: add an AC for password strength, and break the first step into
      email and password validation separately."*
   - *"Groom the scope — I think this is two milestones, not one."*

### Execution — letting the agent run

1. Prompt: *"Start executing the signup milestone."*
2. The agent reads the spec, claims work packages, implements steps.
3. Each acceptance criterion produces evidence — a command and an exit code.
4. The agent surfaces a **seam** when it needs you:
   - *"Done with self-review. Ready for the external review pass."*
   - *"I hit a gate I can't pass — see finding <F-NN>."*

### Review — the human-in-the-loop moments

1. The agent opens the TUI or shows a summary at completion.
2. You review: tests, diffs, design choices.
3. You decide:
   - *"Ship it."* — milestone moves to `complete`.
   - *"File a finding: <description>"* — runner reopens for remediation.
   - *"Refine: <follow-up>"* — a small follow-up milestone is captured.

### Daily loop — once a plan is running

A typical day:

```
morning     →  "What's next on the plan?"            (mp status / mp next)
mid-morning →  "Add a milestone for X."              (capture)
mid-day     →  "Start executing Y."                  (run)
evening     →  "Show me what's done, what's open."   (review in TUI)
```

Each prompt is short. The agent does the bookkeeping; you decide the seams.

## Example prompts

A non-exhaustive list, grouped by intent.

**Setup**
- *"Initialize master-plan in this folder."*
- *"Open the TUI and show me the empty plan."*

**Capture**
- *"Create a milestone for shipping user signup."*
- *"Add an acceptance criterion: users can sign up with email and password."*
- *"Groom the signup milestone — split the ACs into must-have and
  nice-to-have."*

**Execution**
- *"Start executing the signup milestone."*
- *"Run the next step and show me evidence when done."*
- *"Mark step 3 as blocked — I want to think about the design first."*

**Review**
- *"Show me what's done and what's open."*
- *"File a finding: the password validator allows empty strings."*
- *"Approve the milestone — the runner's evidence looks clean."*

**Refinement**
- *"Add a follow-up milestone for adding 2FA."*
- *"Refine the signup milestone: add stricter error-handling requirements."*

**Daily check-in**
- *"What's next on the plan?"*
- *"Open the TUI and show me where we stand."*

## Where the human stays

The agent handles:
- Running `mp` commands
- Writing valid JSON to the plan directory
- Composing, grooming, and self-reviewing prompts
- Generating evidence (commands, exit codes)

The human stays at:
- Scope decisions — *"this is two milestones, not one"*
- Approval gates — *"ship it"*
- Ambiguity resolution — *"the AC for password strength — what's the policy?"*
- Cross-prompt integration — *"does this break the migration we're planning?"*

The fewer seams you take, the more autonomy the agent has. The more seams
you take, the more control you keep. master-plan supports either.

## See also

- [`../agent-guide/`](../agent-guide/) — orientation the agent reads at session
  start
- [`../mp/`](../mp/) — full `mp` command reference
- [`../raul/`](../raul/) — the human-facing TUI
- [`../milestones/`](../milestones/) — the state machine
- [`../skills/`](../skills/) — what skills your agent has loaded