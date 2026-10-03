//! In-process test surface for `mp` integration tests.
//!
//! `lib_api` is the **in-process** alternative to spawning the `mp`
//! binary via `TestEnv::run`. Each public function maps 1:1 to a CLI
//! command's JSON-output contract; tests can call them directly from
//! the test process and skip the subprocess spawn (~50 ms saved per
//! call).
//!
//! Background: milestones **M162** / **M175**
//! (`master-plan/milestones/162-*.json`, `175-*.json`).
//! Taxonomy: see CONTRIBUTING.md (Testing section).
//!
//! ## When to use `lib_api` vs `env.run`
//!
//! - **Use `lib_api`** when the test asserts on the JSON shape of a
//!   read-only command, or a fragment read/write that goes through
//!   `mp::milestone::*` / `mp::step::*` / `mp::validate::*`.
//! - **Use `env.run`** for install / uninstall / doctor / watch / TUI /
//!   init / end-to-end smoke. Those categories MUST stay subprocess.
//!
//! ## Module layout (M232)
//!
//! The bodies live in dedicated submodules; this file is a thin
//! re-export shim so callers can keep writing `lib_api::*`:
//!
//! - [`ctx`] — `PlanContext` builders + read-only `validate`.
//! - [`mutation`] — milestone / step / wp / finding / session /
//!   plan_diff / trace / execution_report wrappers.
//! - [`io`] — in-process CLI runner (`run`, `run_json`, `run_at_repo`,
//!   `run_validate`) — drop-in for `TestEnv::run`.
//! - [`capture`] — `capture_stdio` fd-redirect helper used by [`io`].
//!
//! ## Parity guard
//!
//! `crates/mp/tests/lib_api_parity.rs` runs each wrapper side-by-side
//! with `env.run` on the same fixture and asserts key-set / value-type
//! shape parity. If a wrapper drifts from the CLI, that parity test
//! fails before the wrapper gets merged.

#![allow(dead_code)] // not every test uses every wrapper

pub mod capture;
pub mod ctx;
pub mod io;
pub mod mutation;

// `pub use ...::*` re-exports make every pub item in the submodule
// reachable as `lib_api::*`. The `unused_imports` lint fires here
// because the symbols are referenced from external test binaries
// (`crate::common::lib_api::xxx`), not from lib_api.rs — that is the
// point of the re-export.
#[allow(unused_imports)]
pub use capture::*;
#[allow(unused_imports)]
pub use ctx::*;
#[allow(unused_imports)]
pub use io::*;
#[allow(unused_imports)]
pub use mutation::*;
