//! # mp-oracle
//!
//! Oracle parity tests for the `mp` toolkit. **This crate exists for one
//! reason: to give `jsonschema` and its 30-crate transitive tree a
//! dedicated link target, so the 69 integration-test binaries that do not
//! use it stop paying for the link on every `crates/mp/src/` edit.** See
//! M161 in `master-plan/milestones/`.
//!
//! The crate is intentionally minimal:
//!
//! - **No public lib API** — there is nothing to import. The crate
//!   compiles to a lib stub because integration tests under `tests/`
//!   must link against a crate lib.
//! - **No `[[bin]]`** — no shipped binary.
//! - **Tests live under `tests/`** — only two files today
//!   (`mini_schema_parity.rs`, `mini_schema_e2e.rs`). See the
//!   crate-root [`README.md`](../README.md) for the boundary rule.
//!
//! ## When to add a test here
//!
//! - The test depends on `jsonschema` (the dev-only oracle).
//! - The test is a parity / cross-check between mp's runtime path and an
//!   external canonical implementation.
//!
//! ## When NOT to add a test here
//!
//! - The test only exercises `mp` (CLI spawn, JSON shape, gate logic,
//!   config plumbing, fixture validate). Those belong in
//!   `crates/mp/tests/` so they ride the fast `cargo test -p mp` link
//!   profile.

// Defensive: this lib stub has no items on purpose (cargo integration
// tests under `tests/` must link against a crate lib, but this crate
// exposes no public API). The `dead_code` allow keeps any stray future
// import from producing an unused-warning when the stub is touched.
#![allow(dead_code)]
