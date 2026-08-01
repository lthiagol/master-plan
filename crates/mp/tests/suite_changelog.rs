//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/changelog_add.rs"]
mod changelog_add;

#[path = "suites/changelog_generate.rs"]
mod changelog_generate;

#[path = "suites/changelog_init.rs"]
mod changelog_init;

#[path = "suites/changelog_show.rs"]
mod changelog_show;
