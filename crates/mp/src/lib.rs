pub mod ac_verify;
pub mod activity;
pub mod annotation;
pub mod integration_test_map;
pub use mp_model;
pub mod backlog;
pub mod bootstrap;
pub mod brownfield;
pub mod challenge;
pub mod delta;
pub mod digest;
pub mod harness;
pub mod install;

pub mod app;
pub mod assets;
pub mod brief;
pub mod charter;
pub mod cli;
pub mod commands;
pub mod config;
pub mod config_cmd;
pub mod decisions;
pub mod doctor;
pub mod execution;
pub mod git;
pub mod graph;
pub mod groom;
pub mod hygiene;
pub mod idea;
pub mod inbox;
pub mod interview;
pub mod json_input;
pub mod migrate;
pub mod migrations;
pub mod milestone;
pub mod milestone_trace;
pub mod mini_schema;
pub mod model;
pub(crate) mod mutation_txn;
pub mod note;
pub mod overview;
pub mod path_engine;
pub mod path_prefs;
pub mod paths;
pub mod perf_measure;
pub mod plan_diff;
pub mod plan_gaps;
pub mod plan_io;
pub mod projection;
pub mod schema;
pub mod session;
pub mod skill;
pub mod spec_review;
pub mod specs;
pub mod step;
pub mod step_claim;
pub mod sync;
pub mod wp;

pub mod execution_report;
pub mod milestone_health;
pub mod reviews;
pub mod search;
pub mod store;
pub mod track_kind;
pub mod validate;
pub mod watch;

pub use paths::PlanContext;

/// Silent sentinel error that carries a process exit code without a
/// displayable message. Used by commands (e.g. bulk partial failure) that
/// have already emitted their own report to stdout and just need `main` to
/// exit with a specific code — `main` downcasts to this and skips the
/// default `Error:` printing.
#[derive(Debug)]
pub struct ExitCode(pub i32);

impl ExitCode {
    /// Bulk partial-failure exit code (see `mp milestone bulk …`).
    pub fn partial_failure() -> Self {
        ExitCode(2)
    }
}

impl std::fmt::Display for ExitCode {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Intentionally empty: callers have already printed their report.
        Ok(())
    }
}

impl std::error::Error for ExitCode {}
