//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/review_discovery.rs"]
mod review_discovery;

#[path = "suites/review_lifecycle.rs"]
mod review_lifecycle;

#[path = "suites/reviews_bulk.rs"]
mod reviews_bulk;

#[path = "suites/reviews_comment.rs"]
mod reviews_comment;

#[path = "suites/reviews_handoff.rs"]
mod reviews_handoff;

#[path = "suites/reviews_hunk_export.rs"]
mod reviews_hunk_export;

#[path = "suites/reviews_hunk_sidecar.rs"]
mod reviews_hunk_sidecar;

#[path = "suites/reviews_queue.rs"]
mod reviews_queue;

#[path = "suites/reviews_sweep.rs"]
mod reviews_sweep;

#[path = "suites/spec_review.rs"]
mod spec_review;
