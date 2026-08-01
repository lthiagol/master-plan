# Cutover Guide — Plan Directory Rename / Relocation

## When to use this guide

- You've been using `master-plan/` and want to move to `.mp/` (or vice versa).
- Your project's plan directory name no longer matches your `config.json`'s `location` field.
- You want to adopt a non-default plan-dir name for consistency with other tools.

---

## Option 1: `mp plan relocate` (single step)

If your current plan directory exists and you want to rename it, use the built-in command:

```bash
# Rename master-plan/ to .mp/ and update location in config.json
mp plan relocate master-plan .mp
```

```bash
# Rename .mp/ to master-plan/
mp plan relocate .mp master-plan
```

The command:
1. Renames the directory atomically (`fs::rename`)
2. Updates `workflow.plan.location` in `config.json` to match the new name
3. Outputs confirmation

**Limitations:** The `mp` binary itself still discovers the plan directory using the standard resolution rules (see `paths.rs`). After rename the new directory is auto-detected. No post-cutover steps are needed.

---

## Option 2: Parallel-bootstrap cutover (manual)

Use when adopting `mp` into an existing project that already has plan content (e.g., a handoff doc or a previous planning attempt).

1. Create the **new** plan directory and init it:
   ```bash
   mp init --profile full --plan-dir .mp
   ```

2. Copy existing plan data:
   ```bash
   cp -r old-master-plan/milestones/ .mp/milestones/
   cp -r old-master-plan/brief.json .mp/brief.json  # if applicable
   cp -r old-master-plan/backlog.json .mp/backlog.json  # if applicable
   ```

3. Rebuild the index:
   ```bash
   MP_PLAN_DIR=.mp mp sync
   ```

4. Validate both old and new:
   ```bash
   mp validate                           # uses auto-detected plan dir
   MP_PLAN_DIR=.mp mp validate           # explicit
   ```

5. Remove old directory once validated:
   ```bash
   rm -rf old-master-plan/
   ```

---

## Post-cutover: reference check

After any cutover, check for stale references to the old plan directory name:

- **Skill/AGENTS.md files:** grep for the old directory name in `.opencode/skills/` or `.cursor/skills/` files.
- **CI scripts:** `.github/workflows/plan.yml` may reference the old path.
- **Documentation:** any project-specific docs that reference `master-plan/` or `.mp/`.

```bash
grep -rn "old-master-plan\|master-plan" --include="*.md" --include="*.yml" --include="*.yaml" .
```

---

## Verifying the cutover

After cutover:

```bash
mp doctor         # should pass
mp validate       # should pass
mp status         # should show correct plan data
```
