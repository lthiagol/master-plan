# M232 Review cycle 1 findings

- F-01 (low, doc drift): `scripts/verify-m232-step-s1.sh` header claims
  "Same shape as verify-m232-ac01.sh: line-count < 100 plus the four
  submodule files", but the body has only 2 checks (line count + ctx.rs
  exists). mutation.rs / io.rs / capture.rs existence is NOT checked by
  this wrapper. AC-01's wrapper (the actual gate at complete time) does
  cover all 5, so this is cosmetic — but the step wrapper is what an
  agent uses to verify S1 locally, and the body/comment mismatch is
  misleading.

No HIGH or MEDIUM findings. Refactor is real and complete:

- AC-01: `lib_api.rs` is 59 lines (well under 100); bodies live in
  `lib_api/{ctx,mutation,io,capture}.rs` (all present, real content).
- AC-02: `seed_handoff_gate` / `init_git` / `capture_stdio` each have
  exactly one definition (verified with `rg 'fn <name>\b' crates/mp/tests`).
- AC-03: cold build independently confirms
  `cargo nextest list -p mp | wc -l` = 3856 = recorded baseline (zero
  tests dropped).
- AC-04: cold build runs `cargo nextest list -p mp` (which compiles the
  test binaries) without error; `cargo fmt --all -- --check` exits 0;
  `cargo clippy -p mp --tests --no-deps -- -D warnings` exits 0.
  Runner's claim of 3856 passed / 1 skipped matches the list count.
- All 6 wrappers are executable, have `set -euo pipefail`, and run the
  full sub-check set (no narrowing).
- Callers in `suites/plan_diff.rs`, `suites/digest.rs`,
  `suites/git_suggest.rs`, `suites/p12_recommendation_batch.rs` import
  from the canonical `common::{seed,git}` paths.
- mod.rs declares `pub mod git;` and `pub mod seed;`.

Note: the new shared `seed_handoff_gate` in `common/seed.rs` uses
`lib_api::run(...)` (in-process), where the prior
`suites/digest.rs::seed_handoff_gate` used `env.run(...)` (subprocess).
The runner's source comment (`seed.rs:5-9`) discloses this and chooses
the in-process path deliberately; `lib_api_parity.rs` guards against
drift between the two paths; all 3856 tests pass. Not a finding — the
disclosure is in-tree and the test suite is green.

Verdict: ok