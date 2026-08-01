//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/step_after.rs"]
mod step_after;

#[path = "suites/step_claim.rs"]
mod step_claim;

#[path = "suites/step_deps.rs"]
mod step_deps;

#[path = "suites/step_files_value.rs"]
mod step_files_value;
