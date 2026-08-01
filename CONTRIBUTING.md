# Contributing to master-plan

Thanks for contributing. This guide is short on purpose — most of the
real rules live in the [`docs/`](docs/) tree.

---

## Before you open a PR

1. **Spec before code.** If your change touches `crates/mp/` or
   `crates/raul/` runtime behavior, the relevant milestone in
   `master-plan/milestones/` must have `spec_status: ready`. See
   [`docs/milestone-lifecycle/planning.md`](docs/milestone-lifecycle/planning.md).
2. **Use `mp`, not hand edits,** for everything under `master-plan/`.
3. **Read [AGENTS.md](AGENTS.md)** for the high-level workflow and
   `master-plan/AGENTS.md` for the plan-zone rules.

---

## Testing

When you add a test, you need to pick **subprocess vs in-process**.

- **Subprocess:** call `env.run([...])` from
  `crates/mp/tests/common/mod.rs`. Spawns the `mp` binary, ~50 ms per
  spawn.
- **In-process:** call `lib_api::…()` from
  `crates/mp/tests/common/lib_api.rs`. Calls `mp::…` directly from the
  test process, ~1 ms per call.

Test taxonomy summary (read before adding a test):

| Category | Surface | Why |
|----------|---------|-----|
| install / uninstall / doctor / watch / TUI / init | subprocess | real-fs, process supervision, end-to-end smoke |
| validate / fragment ops / projection / plan_io / JSON shape | in-process | pure logic, no subprocess needed |

**Rule of thumb:** if the test's assertions are reachable via a public
`mp::…` function, use `lib_api` and skip the spawn. If you're not sure,
file an `idea` (`mp idea create --title …`) and ask.

---

## Lint / format / test commands

```bash
make test         # cargo nextest run + cargo fmt --check (parallel)
make lint         # cargo clippy --all-targets -- -D warnings + fmt --check
make ci           # lint + test (local one-shot)
make dep-audit    # transitive-count gate
make test-fixtures   # mp validate on hand-crafted fixtures
make test-scenarios  # golden CLI scenarios
```

See [AGENTS.md](AGENTS.md) for the full toolchain.

---

## Style

- **No comments unless asked.** Code should be self-explanatory.
- **Match the file's existing style.** Read 1-2 neighboring files
  before writing yours.
- **Cargo:** follow the workspace's pinned versions and `default-features = false`
  pattern in `crates/mp/Cargo.toml`.

---

## Hand-off

When your change is ready:

1. `mp validate` (must be `ok: true`, no new warnings).
2. `cargo fmt --all`.
3. `cargo clippy --all-targets -- -D warnings`.
4. `cargo nextest run --no-fail-fast`.
5. Open a PR. The CI runs the same gates.

For milestones: see `master-plan/AGENTS.md §3` for the full
execute → review → remediate → re-review loop.

---

## Branch model

- **`stable`** — default branch. PRs land here from `wip`. Receives release
  tags. CI must be green; one approving review required.
- **`wip`** — working branch. Day-to-day commits and milestone batches
  land here first. CI must be green before merge to `stable`.

Fork flow:

```bash
git clone git@github.com:<your-fork>/master-plan.git
cd master-plan
git remote add upstream git@github.com:lthiagol/master-plan.git
git checkout wip            # branch from wip, not stable
# ... make changes ...
git push origin wip
# open PR from your-fork:wip -> lthiagol/master-plan:wip
```

PRs from external forks are reviewed against `wip` first; promotion to
`stable` happens after merge.