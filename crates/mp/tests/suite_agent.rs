//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/agent_filters.rs"]
mod agent_filters;

#[path = "suites/agent_projection.rs"]
mod agent_projection;

#[path = "suites/agent_summary.rs"]
mod agent_summary;
