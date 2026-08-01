//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/interview_alias.rs"]
mod interview_alias;

#[path = "suites/interview_draft.rs"]
mod interview_draft;
