# `mp-oracle` — Oracle boundary for `mp` integration tests

This workspace member exists for **one reason**: to give `jsonschema` and
its 30-crate transitive tree a dedicated link target, so the 69
integration-test binaries in `crates/mp/tests/` that do not use it stop
paying for the link on every `crates/mp/src/` edit.

Background: milestone **M161** (`master-plan/milestones/161-*.json`).

---

## The split at a glance

| Path | Crate | Why |
|------|-------|-----|
| `crates/mp/tests/` | `mp` | Default test surface. Adds zero exotic deps. |
| `crates/mp-oracle/tests/` | `mp-oracle` | Pulls in `jsonschema` for parity checks against `mp::mini_schema`. |

Before M161, `crates/mp/Cargo.toml` declared `jsonschema` as a
`[dev-dependency]` of `mp`. Cargo's link rule then forced **every** of the
69 integration-test binaries that depended on `mp` to link `jsonschema`
plus `fancy-regex`, `uuid-simd`, `email_address`, `idna`, `referencing`,
`fraction`, `ahash`, and ~25 more crates — even though only **two** test
files actually used it (`mini_schema_parity.rs` and `mini_schema_e2e.rs`).

After M161, those two files live here in `crates/mp-oracle/tests/` and
the `mp` test profile is clean.

---

## When to add a test to `mp-oracle`

Add a test here when **any** of these is true:

- The test imports `jsonschema::*` (the dev-only oracle).
- The test is a **parity check** between `mp`'s runtime validator
  (`mp::mini_schema::Validator`) and an external canonical
  implementation, and the external side is too heavy to link into the
  default `mp` test profile.
- The test is an **end-to-end check of `mp`'s schema-enforcement path**
  (spawning the `mp` CLI and asserting the runtime `mini_schema` accepts
  / rejects a milestone, plan, or other artifact). These pair with the
  parity tests in CI even though they don't import `jsonschema`.
- The test would otherwise pull a transitive tree with more than ~5
  crates into `mp`'s `[dev-dependencies]`.

## When NOT to add a test here

Put the test in `crates/mp/tests/` (or a sub-module thereof) when:

- The test only exercises `mp`'s public API (CLI spawn, JSON shape, gate
  logic, config plumbing, fixture validate, etc.).
- The test uses only the already-in-`mp`-profile deps: `serde`,
  `serde_json`, `tempfile`, `libc` (unix), and `mp` itself.
- The test is fast and small enough that putting it in `mp-oracle`
  would mean re-running `cargo build -p mp-oracle` for an unrelated
  reason.

**Rule of thumb:** if the test would not change if you deleted
`jsonschema` from this crate, it belongs in `mp`.

---

## Local helpers

`tests/common/mod.rs` is a deliberately small subset of
`crates/mp/tests/common/mod.rs`. It exposes:

- `repo_root()` — workspace root, two levels up from `CARGO_MANIFEST_DIR`.
- `mp_bin()` — path to the workspace-built `mp` binary
  (`target/debug/mp`). No hardlink snapshot, no retry loop — the oracle
  suite runs ≤2 tests, so the rebuild-race failure mode is unlikely to
  surface here. If `mp` becomes a flaky spawn target from here, lift the
  snapshot helpers from `crates/mp/tests/common/mod.rs`.
- `TestEnv::new()` / `TestEnv::run(...)` — tempdir wrapper that spawns
  `mp` with `MP_HOME` pointed at the real plan.

---

## CI

`make test` runs `cargo nextest run` across the workspace, which
includes `-p mp-oracle`. CI does not need to be reconfigured: the oracle
tests run on every PR, just from a separate workspace member.

---

## See also

- `master-plan/milestones/161-stop-linking-jsonschema-into-all-124-test-binaries.json`
- `crates/mp/Cargo.toml` `[dev-dependencies]` (now empty of `jsonschema`)
- `crates/mp/src/mini_schema.rs` (the runtime validator)
