//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/track_json_parity.rs"]
mod track_json_parity;

#[path = "suites/track_lifecycle.rs"]
mod track_lifecycle;

#[path = "suites/track_listing.rs"]
mod track_listing;
