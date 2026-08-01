//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/fragment_ac_read.rs"]
mod fragment_ac_read;

#[path = "suites/fragment_ac_write.rs"]
mod fragment_ac_write;

#[path = "suites/fragment_groom_scenario.rs"]
mod fragment_groom_scenario;

#[path = "suites/fragment_projection.rs"]
mod fragment_projection;

#[path = "suites/fragment_step_read.rs"]
mod fragment_step_read;

#[path = "suites/fragment_step_write.rs"]
mod fragment_step_write;

#[path = "suites/fragment_update_guard.rs"]
mod fragment_update_guard;

#[path = "suites/fragment_wp_write.rs"]
mod fragment_wp_write;
