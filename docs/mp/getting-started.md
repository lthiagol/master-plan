# Getting started with `mp`

## Install

The toolkit ships an installer that places `mp` and `raul` under
`~/.agents/master-plan` and deploys agent skills for your chosen harness(es).

From a clone of this repo:

```bash
make build          # cargo build --release
make install        # toolkit + OpenCode + Cursor + Pi skills
```

Or, step by step:

```bash
make install-global    # mp, raul, env.sh, templates only
make install-opencode  # + OpenCode harness skills
make install-cursor    # + Cursor harness skills
make install-pi        # + Pi harness skills
```

Then put the toolkit on your shell's path:

```bash
export MP_HOME="$HOME/.agents/master-plan"
source "$MP_HOME/env.sh"
mp doctor              # health check
```

For local development against a checkout, point at the freshly built binary
instead of the global install:

```bash
eval "$(make dev-env)" # sets MP_HOME + PATH for the current shell
make build && mp init  # in another project
```

## Start a project

From the root of the project you want to plan:

```bash
mp init [--profile full|hybrid|session]
```

`mp init` writes a small set of starter files (an `AGENTS.md` for your agents, a
`config.json`, a `plan.json`) and runs `mp doctor`. The **profile** chooses the
shape of the plan and the workflow around it.

### Profiles

| Profile | Plan location | Gates | Best for |
|---------|---------------|-------|----------|
| **`full`** (default) | `master-plan/`, committed to git | strict (min 2 out-of-scope, min 1 AC) | Personal/open-source projects where the plan is part of the repo |
| **`hybrid`** | `.mp/`, gitignored; tracks + a session per branch | relaxed (min 1 out-of-scope, min 1 AC) | Work/corporate repos where the plan must not be committed |
| **`session`** | like `hybrid` | like `hybrid` | Short-lived, session-scoped work |

The `full` profile seeds the rich planning artifacts (`brief.json`,
`backlog.json`, `ideas.json`, `decisions.json`, `annotations.json`) and starts in
a *planning* phase. The `hybrid`/`session` profiles skip the brief and backlog,
seed lighter artifacts, and start *ready for execution* — they are for projects
that mostly ship tracks and sessions, not full milestone specs.

See [`config.md`](./config.md) for the full config surface.

## The recommended first session (full profile)

```bash
mp init                        # 1. create the plan
mp doctor                      # 2. confirm health
mp brief todo                  # 3. capture project context (one topic at a time)
# … answer the brief topics with the user …
mp brief done                  # 4. close the brief
```

Once the brief is done you can either:

- **Plan a feature** → `mp interview checklist --checklist-type milestone`, then
  `mp milestone create --json @-`.
- **Park a vague idea** → `mp idea create --title "…"`.
- **Capture deferred work** → `mp backlog add --desc "…"`.
- **Ship a small fix now** → `mp track add bugfix --title "…" --verification "…"`.

## Find out what to do next

```bash
mp status        # headline metrics + suggested next path
mp next          # the single head item of the default (execution) lane
mp path          # the full work queue across planning lanes
mp inbox         # things that need a human/agent decision
```

`mp path` organizes work into lanes: **blocked**, **execution**, **review**,
**grooming**, **backlog**. `mp next --lane review` returns the head of a specific
lane.

## Validate constantly

```bash
mp validate            # full structural validation
mp validate --summary  # ok/error counts + warnings grouped by code
```

Run `mp validate` after **every** write. It is how you know the plan is still
internally consistent. The CLI runs gates (spec-status transitions, dependency
done-ness, acceptance-criteria presence, scope minimums) and reports violations
by milestone.

## A complete milestone loop (cheat sheet)

```bash
# 1. Spec (planning phase)
mp interview checklist --checklist-type milestone
mp milestone create --json @-                # spec fields only
mp milestone set-spec-status <id> review
# … user approves …
mp milestone approve <id>

# 2. Plan (implementation)
mp milestone decompose <id>                  # scaffold work packages + steps
mp milestone step add <id> --wp WP1 --action "…" --files a.rs --tests "…" --done-when "…"
mp validate

# 3. Execute
mp milestone set-status <id> in-progress
mp milestone step set-status <id> S1 in-progress
# … make code changes …
mp milestone step done <id> S1

# 4. Verify & complete
mp milestone criterion pass <id> AC-01 --evidence "cargo test … exit 0"
mp milestone complete <id> --evidence "…"

# 5. Review (independent)
mp reviews pending
mp execution report <id>     # read the executor's claims
mp reviews pass <id> --verdict ok --reviewer alice   # different session than executor → complete
```

Each of these steps has detail in [`commands.md`](./commands.md), and the
lifecycle rationale in [`../milestone-lifecycle/`](../milestone-lifecycle/).
