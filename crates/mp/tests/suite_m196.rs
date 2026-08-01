//! M196 integration tests:
//! - `lifecycle_review_gate` (AC-01 + AC-02): the review gate (non-track + no review
//!   → `executed`; passing records → `complete`; track fast-path bypasses;
//!   `--skip-review` bypasses with debt; `--force` does NOT bypass).
//! - `lifecycle_migration_m196` (AC-03): the auto-migration rewrites
//!   `lifecycle: done` → `executed` across plan files, idempotent.

mod common;

#[path = "suites/lifecycle_review_gate.rs"]
mod lifecycle_review_gate;

#[path = "suites/lifecycle_migration_m196.rs"]
mod lifecycle_migration_m196;
