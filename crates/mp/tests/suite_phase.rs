//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/p10_track_promote.rs"]
mod p10_track_promote;

#[path = "suites/p11_sync_split.rs"]
mod p11_sync_split;

#[path = "suites/p12_recommendation_batch.rs"]
mod p12_recommendation_batch;

#[path = "suites/p4_brownfield.rs"]
mod p4_brownfield;

#[path = "suites/p5_polish_batch.rs"]
mod p5_polish_batch;

#[path = "suites/p7_optional_batch.rs"]
mod p7_optional_batch;

#[path = "suites/p9_backlog_promote.rs"]
mod p9_backlog_promote;
