# Adoption Checklist — Day One

Practical steps to adopt Master Plan on **personal** (`full`) and **work** (`hybrid`) repos.

**Status:** v1 CLI is **release-candidate** (milestones M01–M06 shipped). Safe to adopt today.

**Canonical model:** [ADOPTION-PROFILES.md](./ADOPTION-PROFILES.md)  
**Command matrix:** [AGENT-READINESS.md](./AGENT-READINESS.md)  
**First read:** [SIZE-ROUTING.md](./SIZE-ROUTING.md) — smallest-artifact-first decision matrix
**Example work repo:** [tests/fixtures/projects/hybrid-work](../tests/fixtures/projects/hybrid-work/)

---

## 0. Choose your profile

| You are… | Profile | Plan directory | Commit plan to app repo? |
|----------|---------|----------------|--------------------------|
| Owning a personal product backlog | `full` | `master-plan/` | Yes (recommended) |
| Doing work on a `$JOB` codebase | `hybrid` | `.mp/` (gitignored) | No |
| One branch / one PR only | `session` | `.mp/sessions/` | No |

When in doubt: **personal → `full`**, **work → `hybrid`**.

---

## 1. Toolkit setup (once per machine)

Full guide: [INSTALL.md](./INSTALL.md) (OpenCode, Cursor, Pi).

```bash
git clone …/master-plan && cd master-plan
make install              # toolkit + OpenCode + Cursor + Pi (v1 trio)
make doctor

# Or:
mp install --dev --source "$(pwd)" --harness opencode,cursor,pi
mp doctor
```

**Shell** (after install):

```bash
source "$HOME/.agents/master-plan/env.sh"
```

**Local dev** (this repo): `eval "$(make dev-env)"` then `make build`.

---

## 2. Personal project — `full` profile

### 2.1 Bootstrap

```bash
cd ~/Code/my-personal-app
mp init --profile full
# optional brownfield:
mp init --profile full --from-repo
# optional project skills:
mp init --profile full --with-cursor-skill --with-opencode-skill
```

Add root `AGENTS.md` from [templates/ROOT-AGENTS-SNIPPET.md](../templates/ROOT-AGENTS-SNIPPET.md).

### 2.2 Day-one checklist

| Step | Action | Command |
|------|--------|---------|
| ☐ | Plan dir exists | `master-plan/AGENTS.md`, `plan.json`, `brief.json`, `config.json` |
| ☐ | Validate clean | `mp validate` |
| ☐ | Brief (optional) | `mp brief todo` → edit topics → `mp brief done` |
| ☐ | Charter | `mp plan show` · goals via interview or agent draft |
| ☐ | First milestone | `mp milestone create` → `approve` → `plan` / `decompose` |
| ☐ | Git tracks plan | `git add master-plan/` |
| ☐ | Optional CI | [CI.md](./CI.md) — `mp validate` on PR (M07 will wire GHA) |

### 2.3 Ongoing rhythm

```text
Weekly:  backlog groom → milestone spec interviews
Daily:   mp status → mp path → mp next
Fast:    mp track add · mp idea create · mp brief reopen (if charter edits needed)
Publish: mp export · mp git commit
```

See [PM-WORKFLOWS.md](./PM-WORKFLOWS.md), [WALKTHROUGH.md](./WALKTHROUGH.md).

---

## 3. Work project — `hybrid` profile

### 3.1 Bootstrap

```bash
cd ~/Code/work/acme-api
mp init --profile hybrid
echo ".mp/" >> .gitignore
```

Plan dir **auto-resolves** to `.mp/` when `master-plan/` is absent. No `--plan-dir` flag needed in most cases.

Copy [hybrid-work fixture](../tests/fixtures/projects/hybrid-work/) layout as reference.

### 3.2 Day-one checklist

| Step | Action | Command |
|------|--------|---------|
| ☐ | `.mp/` with hybrid config | `mp init --profile hybrid` |
| ☐ | `.mp/` in `.gitignore` | Plan never in work PRs |
| ☐ | Tracks + ideas | `tracks/`, `ideas.json` from init |
| ☐ | Validate | `mp validate` |
| ☐ | Agent profile | `mp config show` |
| ☐ | Brownfield (optional) | `mp init --profile hybrid --from-repo` |

### 3.3 Three work modes

#### A — Quick fix (minutes)

```bash
mp track add bugfix --title "…" --problem "…" --verification "cargo test …"
mp track start bugfix BF-XX
# fix code, test
mp track done bugfix BF-XX --evidence "…"
mp validate
```

#### B — Park a thought

```bash
mp idea create --title "…" --body "…"
# or: mp idea list · promote later to backlog/milestone/track
```

#### C — Feature branch (spec-driven)

```bash
git checkout -b feature/my-change
mp session start --branch feature/my-change
# groom milestone in sessions/<id>/milestone.json
mp groom milestone <id>
mp milestone approve <id>
# implement on branch
mp session export <id> --format pr-body   # paste into PR description
# after merge:
mp session archive <id>
```

**Session layout (decided D-002 / ADR-010):**

```text
.mp/sessions/<branch-slug>/
├── session.json
└── milestone.json
```

**Caveat (M08 not finished):** session milestones are **not yet in `mp path` / `mp next`** for hybrid. Use `mp session show`, `mp groom`, and manual milestone commands until M08 ships. Tracks and ideas work fully in the queue.

### 3.4 What reviewers see

| Artifact | In PR? |
|----------|--------|
| Application code | Yes |
| `.mp/` plan files | No (gitignored) |
| Spec summary | Optional — `mp session export` in PR body |

---

## 4. Agent session start (both profiles)

```bash
mp doctor
mp config show
mp execution status
mp status
mp path
mp next
```

**Agent rules:** [AGENT-PLAYBOOK.md](./AGENT-PLAYBOOK.md) §1a — route by `workflow.profile`.

| Profile | If user says… | Agent routes to |
|---------|---------------|-----------------|
| `full` | “what’s next on the roadmap” | milestone / backlog via `path` |
| `hybrid` | “quick fix” | track |
| `hybrid` | “plan this branch” | `session start` → session milestone |
| `session` | anything scoped | active session only |

---

## 5. Graduating work → personal

```bash
# In personal repo:
mp init --profile full
mp session promote <id>   # from work .mp/ — folds session into milestones/
```

Or copy `sessions/<id>/milestone.json` → `milestones/` and run `mp sync`.

---

## 6. What's adoptable vs coming in v0.2

| Area | Adopt today? | Notes |
|------|--------------|-------|
| **Full profile** (brief → charter → milestones) | ✅ Yes | Complete pipeline |
| **Hybrid tracks + ideas** | ✅ Yes | `next-step` prefers track when configured |
| **Session start/show/export/archive/promote** | ✅ Yes | `sessions/` tree per ADR-010 |
| **Session in path queue** | ⚠️ Partial | M08 — use session commands directly |
| **One session per branch** | ⚠️ Policy only | D-003 accepted; strict enforce in M08 |
| **JSON schema on write** | ❌ Not yet | M07 |
| **CI validate workflow** | ❌ Not yet | M07 — add manually from [CI.md](./CI.md) |
| **Monorepo per-crate plans** | ❌ Deferred | D-004: single root plan for now |

---

## 7. Troubleshooting

| Symptom | Fix |
|---------|-----|
| `master-plan directory not found` | Run `mp init` or pass `--plan-dir .mp` |
| Session milestone ignored by `next-step` | Expected until M08 — use `mp session show` |
| Agent edits plan JSON by hand | Point to AGENTS.md — `mp` only |
| Validate fails G4 on small PR spec | `mp config set workflow.gates.strictness relaxed` |
| Plan accidentally committed | `git rm -r --cached .mp/`; confirm `.gitignore` |
| `brief reopen` needed after done | `mp brief reopen` |

---

## 8. Reference paths

| Asset | Path |
|-------|------|
| Full config preset | [templates/defaults/config.full.json](../templates/defaults/config.full.json) |
| Hybrid config preset | [templates/defaults/config.hybrid.json](../templates/defaults/config.hybrid.json) |
| Session config preset | [templates/defaults/config.session.json](../templates/defaults/config.session.json) |
| Hybrid fixture | [tests/fixtures/projects/hybrid-work/](../tests/fixtures/projects/hybrid-work/) |
| What Rust implements | [AGENT-READINESS.md](./AGENT-READINESS.md) |

---

## 9. Recommended defaults

```text
Personal repos     →  full, master-plan/ committed, brief + backlog + milestones
Work repos         →  hybrid, .mp/ gitignored, tracks + ideas + session per branch
Emergency at work  →  track bugfix (EMERGENCY.md), never bypass milestone gates
Toolkit repo       →  full, master-plan/ dogfoods meta plan (this repo)
```
