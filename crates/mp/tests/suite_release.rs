//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/release_assign.rs"]
mod release_assign;

#[path = "suites/release_ship.rs"]
mod release_ship;

#[path = "suites/release_show.rs"]
mod release_show;
