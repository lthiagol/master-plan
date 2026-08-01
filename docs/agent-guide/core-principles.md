# Core principles for agents

The operational contract every agent follows when driving `mp`. These are
permanent rules, not session notes.

## 1. The plan zone is mediated

The plan directory holds the source of truth. **Never** read or edit those files
directly — always go through `mp`.

- ❌ `cat master-plan/milestones/0042.json`
- ❌ editing a milestone file with a text editor / `sed` / `jq` write
- ✅ `mp show milestone 42`
- ✅ `mp milestone ac pass 42 AC-01 --evidence "…"`

`mp validate` will not always catch drift from a hand-edit, but the audit trail
will show the gap. Hand-edits are the one thing that breaks trust in the whole
plan.

## 2. Reads are JSON; project, don't pipe

Read commands emit JSON by default. Omit `--format json` (it is redundant).
Reach for projection built into `mp` before `jq`:

| Need | Use | Not |
|------|-----|-----|
| One field | `mp show milestone 42 --fields 'milestone.lifecycle'` | `mp show … \| jq …` |
| A few fields | `--fields 'milestone.priority,steps[].status'` | dumping the whole doc |
| Health rollup | `mp show milestone 42 --summary` | counting with jq |
| Validate rollup | `mp validate --summary` | jq on validate output |
| Open findings | `mp reviews finding list 42 --open` | rebuilding via `update --json` |

`--fields` is validated server-side: unknown paths are a hard error, so a typo
fails fast instead of silently returning nothing.

`--format raw` exists but is a debug escape hatch (verbatim on-disk JSON for
`show`, GraphViz DOT for `graph`). Don't use it for normal work.

## 3. Fragment-first writes

Edit **one** AC / step / WP at a time. There are dedicated subcommand surfaces
so you never rebuild an array:

```bash
mp milestone ac  show 42 AC-03
mp milestone ac  update 42 AC-03 --evidence "cargo test … exit 0"
mp milestone step show 42 S2
mp milestone step done  42 S2
mp milestone wp   add   42 --name "Data layer"
```

`mp milestone update --json` accepts scalar spec fields but **rejects** the
`acceptance_criteria` and `steps` arrays by default. `--replace-arrays` is a
migration escape hatch — never a normal tool. `--accept-extra-fields` enables a
raw round-trip (`show --format raw` → `update --json`); use it only when you
intentionally mean to.

## 4. Evidence is test output, not prose

When you record evidence — `ac pass --evidence`, `step done`, `complete
--evidence` — record **what ran and its exit code**:

- ✅ `cargo nextest run -p mp --test config_set --no-fail-fast  exit 0`
- ✅ `cargo clippy -p mp --tests -- -D warnings  exit 0`
- ✅ `manual: ran `mp validate` in a clean checkout; 0 errors`
- ❌ "Test X verifies Y" (a claim, not evidence)
- ❌ "all tests pass" (an assertion, not a run record)

If you did not run it, do not claim it.

## 5. Never complete on red; `--force` is debt

`milestone complete` is a gate, not a label flip. It requires every step `done`
and every AC `pass`ed (or `fail`ed with a reason). Escape hatches exist but each
records visible debt:

- `--force` — bypass the AC gate; stamps `[force-bypassed]` in evidence.
- `--skip-verify` — skip AC *and* step verification; stamps `[skip-verify]`.

A force-bypassed milestone cannot reach `complete` until the bypass is resolved
or explicitly accepted by a reviewer. When you cannot complete honestly,
**block and escalate** instead:

```bash
mp milestone block <id> --reason "AC-05 blocked: <why>"
mp execution pause
# escalate to the user; do NOT call complete
```

## 6. Validate after every write

```bash
mp validate          # after every mp write
```

If a write returns success but the next read shows no change, **stop after 2
attempts** — read `--help`, check you're targeting the right id/path, then
`mp milestone block` + escalate. Looping silently wastes turns and hides the
real problem.

## 7. Use `mp search` to find things

Prefer `mp search` over grepping the plan directory. Hits carry a
`suggested_action` that maps to the matching fragment command:

```bash
mp search "config validation" --type ac --include object
```

`--include object` embeds the full matched fragment so you can often skip a
second `mp show`.

## 8. Scratch for big payloads

When a command takes `--json @file` (e.g. `milestone create`, `milestone
update`, `ac bulk`), write the payload under a scratch path, not the repo root
or a random `/tmp` file:

```bash
SCRATCH=$(mp scratch new m-update)
cat > "$SCRATCH/payload.json" <<'JSON'
{ "intent": { "outcome": "…" } }
JSON
mp milestone update 42 --json @"$SCRATCH/payload.json"
```

`mp scratch` owns the lifecycle (creation, cleanup) so long-running commands
don't race a `/tmp` cleanup.
