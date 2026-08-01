# Adoption Profiles — How a Project Uses Master Plan

How to adopt Master Plan at different intensities: **full product backlog** on personal
projects, **scoped branch work** on work repos, and **tracks/ideas** for quick fixes —
all via **per-project configuration**, not separate tools.

**Status:** v1 RC — profiles, `mp session *`, and `mp session focus` implemented. Session milestones in hybrid `path` via focus (M13).  
**Related:** [SPEC.md §14](./SPEC.md#14-preferences), [DECISIONS.md ADR-003](./DECISIONS.md#adr-003-execution-config-split), [DECISIONS.md ADR-008](./DECISIONS.md#adr-008-adoption-profiles-in-project-config), [ADOPTION-CHECKLIST.md](./ADOPTION-CHECKLIST.md), [BROWNFIELD.md](./BROWNFIELD.md), [PM-WORKFLOWS.md](./PM-WORKFLOWS.md).

---

## 1. Problem

Master Plan was designed around a **committed `master-plan/`** and a **full pipeline**
(brief → charter → milestones → execution). Real adoption looks like:

| Context | What you want |
|---------|----------------|
| **Personal projects** | Full flow — backlog, charter, milestones, tracks, ideas |
| **Work projects** | Spec-driven grooming on a branch, then PR — plan often **not** in the PR |
| **Any project** | Fast lane: `track` for tiny fixes, `idea` to park thoughts |

These are **policy choices per repository**, not global user preferences. The same person
runs `full` at home and `hybrid` at `$JOB`.

---

## 2. Three layers (do not conflate)

| Layer | File | Question it answers | Example values |
|-------|------|---------------------|----------------|
| **Adoption profile** | `config.json` `[workflow]` | Which artifacts and gates does this repo use? | `full`, `session`, `hybrid` |
| **Planning phase** | `plan.json` `planning_phase` | Where are we in the bootstrap pipeline? | `brief`, `charter`, `milestones`, `execution` |
| **Execution mode** | `plan.json` `[execution].mode` | May the agent implement code right now? | `planning`, `autonomous` |

**ADR-003** split execution **strategy** (`plan.json`) from **toolkit prefs** (`config.json`).
**ADR-008** adds **workflow profile** to `config.json` — stable per repo, read at session start.

```text
config.workflow.profile     →  what lanes exist (full vs mini)
plan.planning_phase         →  where we are in planning (state)
plan.execution.mode         →  implement now or spec-only (session)
```

---

## 3. Adoption profiles

A **profile** is a named preset for `[workflow]` in `master-plan/config.json`. Profiles
are not separate products — they toggle artifacts, gates, and init templates.

### 3.1 `full`

**Use when:** Personal projects, greenfield or brownfield, you own the backlog long-term.

| Aspect | Behavior |
|--------|----------|
| Artifacts | brief, charter, backlog, milestones, tracks, ideas, decisions |
| Plan in git | Default **yes** (`workflow.plan.in_repo = true`) |
| Gates | `strictness = full` (G3, G4 defaults) |
| Bootstrap | `mp init --profile full` → full tree; brownfield: `--from-repo` drafts charter |
| End state | Milestones complete → archive; backlog drives roadmap |

### 3.2 `session`

**Use when:** One scoped unit of work (feature branch, PR-sized slice). Disposable plan.

| Aspect | Behavior |
|--------|----------|
| Artifacts | **One** session milestone (or `sessions/<id>/`), tracks optional, ideas optional |
| Skips | brief, backlog (unless explicitly enabled) |
| Plan in git | Default **no** (`workflow.plan.in_repo = false`, gitignored `.mp/` or `master-plan/`) |
| Gates | `strictness = relaxed` (fewer ACs / out-of-scope mins — see §6) |
| Lifecycle | `mp session start` → groom → spec → execute → `mp session export` → merge PR → `mp session archive` |
| End state | Session archived or deleted; optional `session promote` into full plan later |

**Session** is the documented name for the earlier “mini master plan” idea: same milestone
+ steps model, **branch/time-bounded**, not a second schema.

### 3.3 `hybrid` (recommended for work repos)

**Use when:** Work codebases — structured planning when you need it, fast lane always on.

| Aspect | Behavior |
|--------|----------|
| Artifacts | tracks + ideas **always**; milestones **session-scoped** (not full backlog) |
| Plan in git | Default **no** |
| Gates | `relaxed` for session milestones; track gates (T1–T4) unchanged |
| Typical flow | Quick fix → `track`; branch feature → `session start`; park thought → `idea` |
| Charter | Optional minimal `plan.json` (stack, name) — no brief interview |

```text
hybrid repo
├── tracks/          ← always (BF-01, TW-01)
├── ideas.json       ← always (ID-01)
├── sessions/        ← one active session per branch when auto_bind_branch (D-003)
│   └── oauth-branch/
│       └── milestone.json   (or milestones/03-oauth.json)
└── plan.json        ← minimal charter / index
```

---

## 4. Profile comparison

| | `full` | `session` | `hybrid` |
|--|--------|-----------|----------|
| Brief | ✓ | ✗ | ✗ |
| Backlog | ✓ | ✗ | optional |
| Multi-milestone roadmap | ✓ | ✗ | session-only |
| Tracks | ✓ | optional | ✓ |
| Ideas | ✓ | optional | ✓ |
| Plan committed to app repo | default yes | default no | default no |
| Brownfield bootstrap | full charter draft | minimal + session | minimal + tracks |
| Best for | Personal / product | Single PR / branch | Work repos |

---

## 5. Configuration model

### 5.1 Precedence

```text
CLI flags  >  project master-plan/config.json  >  global ~/.agents/config.json  >  defaults
```

**Adoption profile is always per project.** Global config may set `init.default_profile`
for new `mp init` only — it does not override an existing project `config.json`.

### 5.2 Global (`~/.agents/config.json`)

Toolkit defaults — display, optional init presets:

```json
[display]
milestone_prefix = "M"

[init]
default_profile = "full"        # mp init with no --profile
# work_profile = "hybrid"     # optional alias for docs/scripts; not auto-detected
```

> **Note (v2.0 / M76):** `[output] default_format` removed — `mp` always emits JSON by default.

### 5.3 Project (`master-plan/config.json`)

**Target shape (P3.1):**

```json
[workflow]
profile = "hybrid"              # full | session | hybrid

[workflow.artifacts]
brief = false
backlog = false
milestones = "session"          # true | false | "session"
tracks = true
ideas = true
decisions = true

[workflow.plan]
in_repo = false                 # if false, add path to .gitignore
location = ".mp"                # relative to project root; default master-plan/

[workflow.gates]
strictness = "relaxed"          # full | relaxed | minimal

[workflow.session]
auto_bind_branch = true         # mp session start uses current git branch name
archive_on_merge = true         # suggest archive when branch merged (manual confirm)
# focus = "oauth-session-id"    # explicit session focus (when auto_bind_branch=false)

[git]
auto_commit = false             # plan commits only if in_repo = true

[next]
prefer = "track"                # track | milestone | session — bias when queue empty

[planning]
require_min_out_of_scope = 1    # overrides for relaxed (full default: 2)
require_min_acceptance_criteria = 1
```

### 5.4 `workflow.artifacts.milestones` values

| Value | Meaning |
|-------|---------|
| `true` | Full milestone backlog under `milestones/` |
| `false` | No milestone files (tracks-only mode) |
| `"session"` | Milestones only under `sessions/<id>/` or tagged `scope = session` |

### 5.5 Plan directory resolution

```text
1. --plan-dir CLI flag
2. config.workflow.plan.location (if set)
3. <project-root>/master-plan/
```

Environment `MP_PROJECT` sets project root; plan dir is always resolved relative to it.

When `workflow.plan.in_repo = false`, `mp init` should append the plan path to
`.gitignore` (or document the line in init output). CI validate in the app repo is
optional — run `mp validate` locally pre-push.

---

## 6. Gate strictness

Maps to `[planning]` overrides and validate behavior ([SPEC.md §5](./SPEC.md#5-gates-enforced-by-mp-validate)):

| `strictness` | G3 (min ACs at review) | G4 (min out-of-scope) | Notes |
|--------------|------------------------|------------------------|-------|
| `full` | 1+ | 2+ | Default for `full` profile |
| `relaxed` | 1+ | 1+ | Default for `session` / `hybrid` |
| `minimal` | 1+ | 0 | Session hot path; still requires ≥1 AC |

Track gates (T1–T4) are **unchanged** by profile. Emergency policy ([EMERGENCY.md](./EMERGENCY.md)):
hotfixes use tracks, not gate bypass.

---

## 7. Workflows by scenario

### 7.1 Personal — full adoption

```text
mp init --profile full
mp brief todo → … → mp brief done
charter interview → mp plan update …
backlog grooming → mp milestone create …
mp milestone approve → mp execution handoff
autonomous loop → complete → archive
```

Parallel: `idea create`, `track add` for small work without new milestones.

### 7.2 Work — hybrid + feature branch

```text
mp init --profile hybrid --from-repo    # minimal plan, gitignored .mp/
# daily: track fix or idea capture
git checkout -b feature/oauth
mp session start --branch feature/oauth
  → groom outcome, 2–5 steps, ACs
  → mp milestone approve (session scope)
  → implement on branch
mp session export --format pr-body       # paste into PR description
open PR (code only)
after merge: mp session archive
```

### 7.3 Work — quick fix (any profile with tracks)

```text
mp track add bugfix --title "…" --problem "…" --verification "…"
mp track start bugfix BF-03
# fix + test
mp track done bugfix BF-03
mp validate
```

No milestone, no session — **T1–T4** only.

### 7.4 Plan-only grooming (no code)

Respects `execution.mode = planning` regardless of profile:

```text
mp session start (or milestone create in full profile)
interview → spec review → approve
STOP — do not hand off or implement unless user directs
```

See [AGENT-PLAYBOOK.md §4.6](./AGENT-PLAYBOOK.md#46-plan-only-session-no-code).

---

## 8. Session scope (mini plan) — data model

**Implemented layout (D-002, v1 RC):**

```text
master-plan/
├── plan.json
├── config.json
├── sessions/
│   └── feature-oauth/
│       ├── session.json      # id, branch, started, status
│       └── milestone.json    # same schema as milestones/*.json
└── tracks/
```

**Option B — milestone tag:** `milestones/03-oauth.json` with `scope = "session"` and
`session_id = "feature-oauth"`.

**Session lifecycle:**

| Status | Meaning |
|--------|---------|
| `draft` | Grooming, spec not ready |
| `ready` | Spec approved, may implement |
| `in-progress` | Steps underway |
| `done` | Merged or abandoned with evidence |
| `archived` | Moved under `archive/sessions/` |

**Promotion (optional):** `mp session promote <id>` copies session milestone into
`milestones/` when a work repo “graduates” to full adoption.


### Session focus (M13)

When `auto_bind_branch` is false (multiple sessions, no branch binding), use `mp session focus <id>` to designate the active session:

```bash
mp session focus oauth-feature   # sets workflow.session.focus in config.json
mp session unfocus               # clears focus
mp session list                  # includes "focused": true/false per session
mp session show                  # resolves focused session when no id given
```

The focused session milestone is surfaced in `mp path` and `mp next` even when
`auto_bind_branch` is off, enabling explicit session switching in multi-session workflows.
---

## 9. Brownfield bootstrap (profile-aware)

`mp init --from-repo` (planned, P3.1 / P4) uses code zone + `mp doctor` to **propose**
drafts; human approves. Profile controls depth:

| Profile | Auto-fill |
|---------|-----------|
| `full` | stack, `brownfield_likely`, charter goals from README, backlog candidates from issues (optional) |
| `hybrid` | stack, minimal `plan.json`, empty tracks + ideas; **no** brief |
| `session` | stack + empty session template only |

Agent rule ([AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md)): on brownfield, **propose from repo**,
do not re-ask what README/Cargo.toml already states. Profile gates which files get written.

---

## 10. Agent behavior matrix

At **session start**, agent reads:

```bash
mp config show     # merged config including workflow.profile
mp status 
```

| `workflow.profile` | Skip | Always offer |
|--------------------|------|--------------|
| `full` | — | brief → charter → milestones |
| `hybrid` | brief, backlog interview | track, idea, session |
| `session` | brief, charter, backlog | session groom → spec |

| User intent | Route |
|-------------|-------|
| “quick fix” | `track` |
| “park this” | `idea` |
| “plan this branch” | `session start` or milestone (per profile) |
| “full roadmap” | requires `full` profile or `session promote` |

If `workflow.plan.in_repo = false`, agent must **not** commit plan files unless user asks.

---

## 11. Commands (implemented — v1 RC)

| Command | Purpose |
|---------|---------|
| `mp init --profile <full\|session\|hybrid>` | Write `config.json` preset + sparse file tree |
| `mp init --from-repo` | Brownfield bootstrap per profile |
| `mp session start [--branch NAME]` | Create session dir, bind branch |
| `mp session show` | Active session status |
| `mp session archive <id>` | End session lifecycle |
| `mp session export [--format pr-body\|markdown]` | Human summary for PR |
| `mp session promote <id>` | Fold into full milestones (optional) |
| `mp session focus <id>` | Set active session in config.json |
| `mp session unfocus` | Clear session focus |
| `mp config set workflow.profile hybrid` | Change profile (doctor warns on mismatch) |

**Implemented:** session milestones in hybrid `path` / `next-step` via focus (M13).

Full signatures: [MP-COMMANDS.md](../06 - Reference/MP-COMMANDS.md#bootstrap--health).

---

## 12. Implementation status (v1 RC)

| Piece | Status | Notes |
|-------|--------|-------|
| `[workflow]` in config schema | ✅ | `ProjectConfig` in Rust |
| `mp init --profile` | ✅ | `templates/defaults/config.*.json` |
| `mp session *` | ✅ | `sessions/` tree (ADR-010) |
| `mp init --from-repo` | ✅ | Brownfield scan assist |
| `mp doctor` profile checks | ✅ | `--project`, harness, gitignore |
| Agent playbook + skill | ✅ | Profile branches in skill |
| Fixture `hybrid-work` | ✅ | [tests/fixtures/projects/hybrid-work/](../tests/fixtures/projects/hybrid-work/) |
| Session in path queue | ✅ | Via `mp session focus` (M13) |
| `mp session focus` / `unfocus` | ✅ | Config-backed session switching (M13) |

---

## 13. Example configs

### Personal (`full`)

```json
[workflow]
profile = "full"

[workflow.artifacts]
brief = true
backlog = true
milestones = true
tracks = true
ideas = true

[workflow.plan]
in_repo = true
location = "master-plan"

[workflow.gates]
strictness = "full"

[git]
auto_commit = true
commit_on_milestone_complete = true

[next]
prefer = "milestone"
```

### Work (`hybrid`)

```json
[workflow]
profile = "hybrid"

[workflow.artifacts]
brief = false
backlog = false
milestones = "session"
tracks = true
ideas = true

[workflow.plan]
in_repo = false
location = ".mp"

[workflow.gates]
strictness = "relaxed"

[workflow.session]
auto_bind_branch = true

[next]
prefer = "track"
```

---

## 14. Open questions

| Topic | Options |
|-------|---------|
| Session storage | `sessions/` tree vs tagged `milestones/` (resolved: `sessions/` tree, ADR-010) |
| Multiple active sessions | One per branch vs explicit `session focus` (resolved: both, M13) |
| Monorepo | Per-crate `config.json` vs root profile |
| IDE default | Detect work tree → suggest `hybrid` on init (opt-in) |

Record decisions in [DECISIONS.md](./DECISIONS.md) when resolved.

---

## 15. Cross-references

- [ADOPTION-CHECKLIST.md](./ADOPTION-CHECKLIST.md) — day-one steps (personal vs work)
- [EXECUTION-MODES.md](./EXECUTION-MODES.md) — `planning` vs `autonomous` (orthogonal to profile)
- [EMERGENCY.md](./EMERGENCY.md) — tracks for hotfix
- [CI.md](./CI.md) — validate when plan is in repo
- [TEMPLATES.md](./TEMPLATES.md) — config presets per profile
