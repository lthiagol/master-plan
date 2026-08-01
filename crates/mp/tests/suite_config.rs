//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/config_load_reliability.rs"]
mod config_load_reliability;

#[path = "suites/config_scope_default.rs"]
mod config_scope_default;

#[path = "suites/config_scope_no_sideeffect.rs"]
mod config_scope_no_sideeffect;

#[path = "suites/config_ui_keys.rs"]
mod config_ui_keys;
