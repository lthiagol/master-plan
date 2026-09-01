//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/complete_guardrail.rs"]
mod complete_guardrail;

#[path = "suites/milestone_archive.rs"]
mod milestone_archive;

#[path = "suites/milestone_bulk.rs"]
mod milestone_bulk;

#[path = "suites/milestone_complete_gate_caching.rs"]
mod milestone_complete_gate_caching;

#[path = "suites/milestone_create.rs"]
mod milestone_create;

#[path = "suites/milestone_from_handoff.rs"]
mod milestone_from_handoff;

#[path = "suites/milestone_priority.rs"]
mod milestone_priority;

#[path = "suites/milestone_trace.rs"]
mod milestone_trace;

#[path = "suites/milestone_update_conflict.rs"]
mod milestone_update_conflict;

#[path = "suites/milestone_verify.rs"]
mod milestone_verify;

#[path = "suites/transition_gating_blocks_done_until_self_findings_closed.rs"]
mod transition_gating_blocks_done_until_self_findings_closed;

#[path = "suites/status_consumes_lane_summary.rs"]
mod status_consumes_lane_summary;

#[path = "suites/migrate_kinds.rs"]
mod migrate_kinds;

#[path = "suites/path_lanes.rs"]
mod path_lanes;

#[path = "suites/reviews_finding_add_round_trip_with_all_flags.rs"]
mod reviews_finding_add_round_trip_with_all_flags;

#[path = "suites/schema_enums_accept_empty_string.rs"]
mod schema_enums_accept_empty_string;

#[path = "suites/findings_hunk_compat.rs"]
mod findings_hunk_compat;

#[path = "suites/show_review_trail.rs"]
mod show_review_trail;

#[path = "suites/milestone_bulk_set_stage.rs"]
mod milestone_bulk_set_stage;
