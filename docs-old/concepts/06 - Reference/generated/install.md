# `mp install`

**Usage:**

```text
Usage: install [OPTIONS]
```

**Options:**

| Flag | Description |
|------|-------------|
| `--harness` |  |
| `-g`, `--global` |  |
| `--dev` |  |
| `--source` |  |
| `--print-paths` |  |
| `--toolkit-only` |  |
| `--skills` | Deploy only the listed skills (comma-separated). Omit to deploy the 3 base CPD skills (mp-flow, mp-runner, mp-coordinator). Pass `spec-grill` (alone or with the base set) to include the optional grill skill |
| `--agents` | M173 S2: deploy only the listed agents (comma-separated). Agents live at `templates/harness/<harness>/agents/<id>.md` and deploy to `<agent_profile_dir>/<id>.md`. Pass `mp-planner` (alone or alongside other ids) to deploy the dedicated planning agent. Omit to skip agent deploy |
| `--check` | Validate the skill registry consistency without deploying |
| `--list-skills` | M146: list the registry skills with deployment state per harness. Does not deploy |

