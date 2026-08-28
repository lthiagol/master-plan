//! `mp watch` — automated milestone execution via the herdr agent
//! lifecycle (M149).
//!
//! Submodules:
//! - [`preconditions`] — startup checks (S0).
//! - [`herdr`] — pane spawn / list / send / wait (S3–S5).
//! - [`bridge`] — M150 stage-done sentinel: `mp-stage-done` custom-status
//!   emitted by `mp milestone complete` / `mp reviews pass` and consumed
//!   by the watch fast-path. Lifecycle poll remains the fallback when
//!   the bridge is absent.
//! - [`prompts`] — lifecycle-stage templates (S6).
//! - [`state_machine`] — driver loop mapping lifecycle → prompt → wait
//!   → handoff until the milestone reaches complete (S7).
//! - [`sequencer`] — cross-milestone sequencing with per-role pane
//!   reuse (S8).
//! - [`logging`] — structured JSONL watch log (S10).
//! - [`state`] — M152 crash-safe `.mp/watch.state.json` so a `--resume`
//!   run can re-attach to live herdr panes instead of double-spawning.
//! - [`resume`] — M152 herdr-list reconciliation + double-spawn guard.
//! - [`shutdown`] — M152 SIGINT/SIGTERM graceful-shutdown handler.
//! - [`run_state`] — M178 S1 latest-run control-plane state model
//!   (schema_version=2; AC-01 contract fields + v1→v2 migration).

pub mod bridge;
pub mod classification;
pub mod herdr;
pub mod herdr_version;
pub mod logging;
pub mod preconditions;
pub mod prompts;
pub mod resume;
pub mod run_state;
pub mod sequencer;
pub mod shutdown;
pub mod state;
pub mod state_machine;

pub use bridge::{
    build_clear_custom_status_args, build_report_agent_args, clear_stage_done_sentinel,
    detect_herdr_pane_id, emit_stage_done_best_effort, parse_custom_status_from_pane_get,
    read_custom_status_bounded, report_stage_done_bounded, run_herdr_with_timeout,
    sentinel_matches, DEFAULT_BRIDGE_POLL_TIMEOUT_MS, DEFAULT_SUBPROCESS_TIMEOUT_MS,
    STAGE_DONE_AGENT, STAGE_DONE_SENTINEL, STAGE_DONE_SOURCE,
};
pub use herdr::{
    build_pane_split_args, build_start_args, deliver_prompt, lifecycle_advanced_past,
    read_agent_status, read_lifecycle_via_mp, read_output, send_prompt, wait_for_lifecycle,
    wait_for_lifecycle_with, wait_for_readiness, wait_for_readiness_with,
};
pub use herdr::{
    ensure_pane, find_existing_pane, list_panes, pane_label_for, parse_pane_id_from_start_output,
    resolve_harness_kind, spawn_pane, which_herdr, LifecycleTarget, PaneHandle, ReadinessOptions,
    Role, WaitOptions, WaitOutcome, DEFAULT_PANE_N,
};
pub use herdr_version::{
    detect_herdr_cli, detect_herdr_cli_default, HerdrCliShape, VersionFloor, EXPECTED_START_FLAGS,
    REQUIRED_HERDR_VERSION_FLOOR,
};

/// M178 S5/S7 alias for [`herdr::which_herdr`]. The `status` /
/// `output` control-plane verbs use this to resolve the herdr binary
/// without shadowing the `which` name (which is also a shell builtin
/// readers will search for).
pub fn resolve_herdr_binary() -> std::io::Result<std::path::PathBuf> {
    which_herdr().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "herdr not on PATH; install with `make install` or add herdr to PATH",
        )
    })
}
pub use logging::{rfc3339_now, WatchLogEntry, WatchLogger};
pub use preconditions::{
    check_preconditions, default_log_path, try_lazy_auto_set, PreconditionCheck, PreconditionReport,
};
pub use prompts::{
    all_stages, build_prompt, build_prompt_full, build_prompt_with, build_prompt_with_request,
    load_override, BuildPromptRequest, OverrideDiagnostic, OverrideRefusalKind, OverrideRung,
    PromptRenderOptions, PromptStage, RenderedPrompt, TemplateSource, MAX_OVERRIDE_BYTES,
};
pub use resume::{reconcile, PaneStatus, Reconciliation};
pub use run_state::{
    default_run_state_path, MilestoneRunOutcome, RunOutcome, WatchRunState, WatchRunStore,
    WatchTransition, WATCH_RUN_STATE_SCHEMA_VERSION,
};
pub use sequencer::{run_milestones, MilestoneOutcome, SequencerReport};
pub use shutdown::{
    clear_shutdown_flag, install_signal_handlers, is_pid_alive, perform_graceful_shutdown,
    request_shutdown, shutdown_requested, write_shutdown_state_for_test,
};
pub use state::{
    default_state_path, MilestoneState, PaneState, WatchState, WATCH_STATE_SCHEMA_VERSION,
};
pub use state_machine::{
    drive_milestone, next_stage, should_skip, DriveOps, DriveOutcome, RoleConfigs, StagePlan,
    SystemDriveOps,
};
