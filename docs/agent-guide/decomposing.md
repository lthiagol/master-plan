# Decomposing into an implementation plan

You do this **only after** the spec is `approved`. Implementation planning
produces `work_packages[]` and `steps[]` — never include these at spec-creation
time.

## Scaffold, then refine

```bash
mp milestone approve <id>
mp milestone decompose <id>            # scaffold WPs + steps from the spec
mp plan gaps <id>                      # coverage gaps (ACs no step covers?)
mp validate
```

`decompose` gives you a starting skeleton. Then refine it fragment by fragment.

## Work packages

A WP groups related steps behind a goal + rollback note.

```bash
mp milestone wp add <id> --name "Data layer" \
    --goal "Persistent milestone store" \
    --rollback "Drop the new files; old reader still works"
mp milestone wp update <id> WP1 --goal "…"
mp milestone wp remove <id> WP1        # fails if any step references it
```

## Steps

A step is a single unit of implementation work. Pin `files` and `tests` to
observable commands, not outcomes.

```bash
mp milestone step add <id> --wp WP1 \
    --action "Add config struct" \
    --files crates/mp/src/config.rs \
    --tests "cargo nextest run -p mp --test config_set --no-fail-fast" \
    --done-when "Config round-trips through mp config set" \
    --covers-ac AC-01
```

| Field | Guidance |
|-------|----------|
| `--action` | What to do, imperatively. |
| `--files` | Bare paths or comma-separated (`a.rs,b.rs`). **Not** JSON arrays. |
| `--tests` | The command that proves it — `cargo nextest run -p mp --test …`, a Makefile target, a script. Pin the command, not the outcome. |
| `--done-when` | The human-readable success condition. |
| `--covers-ac` | Which AC(s) this step advances (drives coverage analysis). |
| `--after <id>` | Insert after an existing step id. |
| `--id <S2>` | Explicit step id (else auto-numbered). |

### Step file value grammar

`--files` accepts a bare path or a comma-separated list:

```bash
--files crates/mp/src/config.rs
--files a.rs,b.rs
```

Quoted JSON arrays (`--files '["a.rs"]'`) and object literals are **rejected**
by the value parser. Pass bare paths or a comma list.

## Refined writes

```bash
mp milestone step show <id> S2                   # read one step
mp milestone step update <id> S2 --tests "…" --covers-ac AC-02
mp milestone step split <id> S2                  # S2 → S2, S2.1, S2.2
mp milestone step remove <id> S2                 # fails if another step depends on it
```

A step `tests` value that is **not** prefixed `manual:` is treated as a command
that will be executed from the project root during verification. Prefer real
commands. Use `manual: <note>` only for genuinely non-automatable checks.

## Coverage

Every AC should be covered by at least one step, and every step should advance
at least one AC. Check both:

```bash
mp plan coverage <id>            # AC ↔ step coverage matrix
mp plan gaps <id>                # uncovered ACs and orphan steps
mp plan verify-ac <id>           # AC verification integrity pre-flight
mp plan verify-lint              # WARN-only lint for broad verification strings
```

## When the plan is ready

Present it (or summarize it) and get confirmation before executing. Then move to
[`executing.md`](./executing.md). Don't start coding until the implementation
plan is confirmed.
