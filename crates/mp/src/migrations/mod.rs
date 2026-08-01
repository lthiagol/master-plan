//! One-shot plan migrations (M177+).
//!
//! Bulk shape repairs that walk every milestone on disk. CLI surface is
//! `mp migrate <name> [--dry-run] [--yes]`. Each migration is idempotent.

pub mod manual_prefix_backfill;

pub use manual_prefix_backfill::{
    run_manual_prefix_backfill, ManualPrefixBackfillReport, ManualPrefixHit,
};
