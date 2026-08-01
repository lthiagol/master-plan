//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/doctor_discoverability.rs"]
mod doctor_discoverability;

#[path = "suites/doctor_integrity.rs"]
mod doctor_integrity;

#[path = "suites/doctor_registry.rs"]
mod doctor_registry;
