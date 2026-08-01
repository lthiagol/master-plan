//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/note.rs"]
mod note;

#[path = "suites/note_add.rs"]
mod note_add;
