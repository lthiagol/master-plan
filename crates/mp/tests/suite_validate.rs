//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/gate_matrix.rs"]
mod gate_matrix;

#[path = "suites/json_input_hardening.rs"]
mod json_input_hardening;

// mini_schema_e2e and mini_schema_parity moved to crates/mp-oracle/tests
// in M161 (oracle boundary split). The jsonschema-using oracle tests now
// live in a dedicated workspace member so `cargo test -p mp` does not
// link jsonschema into every test binary.

#[path = "suites/mini_schema_fixtures.rs"]
mod mini_schema_fixtures;

#[path = "suites/mini_schema_unit.rs"]
mod mini_schema_unit;

#[path = "suites/schema_lean.rs"]
mod schema_lean;

#[path = "suites/schema_on_step_write.rs"]
mod schema_on_step_write;

#[path = "suites/schema_validate.rs"]
mod schema_validate;

#[path = "suites/validate_drift.rs"]
mod validate_drift;

#[path = "suites/validate_fixture.rs"]
mod validate_fixture;

#[path = "suites/validate_readiness.rs"]
mod validate_readiness;

#[path = "suites/verify_field_validate.rs"]
mod verify_field_validate;

#[path = "suites/verify_lint_broad_scope.rs"]
mod verify_lint_broad_scope;

#[path = "suites/verify_lint_portability.rs"]
mod verify_lint_portability;

#[path = "suites/verify_lint_scope.rs"]
mod verify_lint_scope;
