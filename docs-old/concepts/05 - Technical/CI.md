# CI Integration

Run Master Plan validation in continuous integration so plan drift fails PRs.

**Status:** Implemented — `.github/workflows/plan.yml` runs `mp validate` and `make test-fixtures` on PRs touching the plan or toolkit.

---

## Recommended checks

```yaml
# .github/workflows/plan.yml
jobs:
  validate-plan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release -p mp
      - env:
          MP_HOME: ${{ github.workspace }}
        run: ./target/release/mp validate --format json
      - run: make test-fixtures
```

Exit code `2` = gate failure — fail the job.

---

## Scenario tests (implemented)

Golden CLI scenarios run in CI and locally:

```bash
make test-scenarios
# or: cargo test -p mp --test scenarios
```

Fixtures live in `tests/fixtures/`; scenarios in `tests/scenarios/`. Add scenarios when new commands stabilize.

---

## What CI should not do

- Agents editing `master-plan/` outside `mp` — validate catches some drift, not all
- Commit secrets in AC `evidence` — use CI log links or artifact refs instead

---

## References

- [TESTING.md](./TESTING.md)
- [AGENT-READINESS.md](./AGENT-READINESS.md)
