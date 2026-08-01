//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/bare_rs_fail.rs"]
mod bare_rs_fail;

#[path = "suites/brief_import.rs"]
mod brief_import;

#[path = "suites/charter_edit.rs"]
mod charter_edit;

#[path = "suites/cli_hints.rs"]
mod cli_hints;

#[path = "suites/cli_taxonomy.rs"]
mod cli_taxonomy;

#[path = "suites/code_review_gate.rs"]
mod code_review_gate;

#[path = "suites/collection_remove.rs"]
mod collection_remove;

#[path = "suites/concurrency.rs"]
mod concurrency;

#[path = "suites/dependency_query.rs"]
mod dependency_query;

#[path = "suites/digest.rs"]
mod digest;

#[path = "suites/embedded_assets_parity.rs"]
mod embedded_assets_parity;

#[path = "suites/embedded_assets_zero_config.rs"]
mod embedded_assets_zero_config;

#[path = "suites/format_dispatch.rs"]
mod format_dispatch;

#[path = "suites/fuzzy_search.rs"]
mod fuzzy_search;

#[path = "suites/gaps_validate_agree.rs"]
mod gaps_validate_agree;

#[path = "suites/git_suggest.rs"]
mod git_suggest;

#[path = "suites/idea_dup.rs"]
mod idea_dup;

#[path = "suites/inbox_phantoms.rs"]
mod inbox_phantoms;

#[path = "suites/index_auto_sync.rs"]
mod index_auto_sync;

#[path = "suites/json_migration.rs"]
mod json_migration;

#[path = "suites/json_shape_baseline.rs"]
mod json_shape_baseline;

#[path = "suites/lifecycle_gates_post_migration.rs"]
mod lifecycle_gates_post_migration;

#[path = "suites/lifecycle_migration.rs"]
mod lifecycle_migration;

#[path = "suites/m144_lifecycle_at.rs"]
mod m144_lifecycle_at;

#[path = "suites/read_ergonomics.rs"]
mod read_ergonomics;

#[path = "suites/remove_legacy_placeholder.rs"]
mod remove_legacy_placeholder;

#[path = "suites/scratch.rs"]
mod scratch;

#[path = "suites/search_fragment.rs"]
mod search_fragment;

#[path = "suites/show_parity.rs"]
mod show_parity;

#[path = "suites/skill_context.rs"]
mod skill_context;

#[path = "suites/sort_regression.rs"]
mod sort_regression;

#[path = "suites/spec_and_steps.rs"]
mod spec_and_steps;

#[path = "suites/spec_errors.rs"]
mod spec_errors;

#[path = "suites/state_tracking.rs"]
mod state_tracking;

#[path = "suites/status_readiness.rs"]
mod status_readiness;

#[path = "suites/store_json_only.rs"]
mod store_json_only;

#[path = "suites/strip_dropped_keys.rs"]
mod strip_dropped_keys;

#[path = "suites/strip_deferred_reason.rs"]
mod strip_deferred_reason;

#[path = "suites/manual_prefix_backfill.rs"]
mod manual_prefix_backfill;

#[path = "suites/ws3_ws4_batch.rs"]
mod ws3_ws4_batch;
