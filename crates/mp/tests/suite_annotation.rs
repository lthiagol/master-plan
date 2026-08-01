//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/annotation.rs"]
mod annotation;

#[path = "suites/annotation_gate.rs"]
mod annotation_gate;

#[path = "suites/annotation_inbox.rs"]
mod annotation_inbox;

#[path = "suites/annotation_store.rs"]
mod annotation_store;

#[path = "suites/annotation_validate.rs"]
mod annotation_validate;
