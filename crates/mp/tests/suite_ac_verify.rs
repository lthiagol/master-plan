//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/ac_verify_drain_join_timeout.rs"]
mod ac_verify_drain_join_timeout;

#[path = "suites/ac_verify_pipe_deadlock.rs"]
mod ac_verify_pipe_deadlock;

#[path = "suites/ac_verify_shapes.rs"]
mod ac_verify_shapes;

#[path = "suites/ac_verify_timeout.rs"]
mod ac_verify_timeout;
