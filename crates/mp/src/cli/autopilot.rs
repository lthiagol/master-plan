//! M207 / M208 / M209: `mp autopilot` CLI surface.
//!
//! Subcommand tree (clap derives the `Commands::Autopilot` variant
//! from this enum):
//!
//! ```text
//! mp autopilot start [IDS]... [--dry-run] [--log-file PATH] [--stall-timeout-ms N] [--poll-interval-ms N] [--resume] [--force] [--detach]
//! mp autopilot status [--summary]
//! mp autopilot stop [--pid N] [--timeout-secs N]
//! mp autopilot output [--max-bytes N] [--timeout-ms N] [--role ROLE]
//! mp autopilot result [--force]
//! mp autopilot session list
//! mp autopilot session show <id>
//! mp autopilot session transition --session <id> --role <role> --state <state> [--working-on <m:n>]
//! mp autopilot note add --session <id> --kind <kind> --body <body> [--cycle <n>] [--milestone <id>]
//! mp autopilot config get <key>
//! mp autopilot config set <key> <value> [--dry-run]
//! ```
//!
//! M229: the legacy `mp watch` and `mp watch-control` aliases plus
//! the `mp autopilot migrate` shim were removed by the breaking-
//! release cleanup. `mp autopilot start` is the canonical entry
//! point for driving milestones; `mp autopilot status|stop|output|
//! result` replace `mp watch-control *`.

use clap::{Args, Subcommand};

#[derive(Subcommand, Debug)]
pub enum AutopilotCmd {
    /// M208: drive one or more milestones through their lifecycle.
    /// Replaces the removed `mp watch <ids...>` — same args, same exit codes,
    /// same JSON output. Use `--dry-run` to preview without spawning agents.
    Start(AutopilotStartArgs),
    /// M208: read the latest autopilot run's control-plane state
    /// (queue, active milestone, lifecycle, stage, target, role, pane
    /// ids, log path, run outcome). Replaces the removed
    /// `mp watch-control status`.
    Status {
        /// Summary only (classification + pid_alive). Default false.
        #[arg(long)]
        summary: bool,
    },
    /// M208: gracefully stop the recorded autopilot run by signaling
    /// its PID. No-op (stable response) when no live run exists.
    Stop {
        /// Override the recorded PID.
        #[arg(long)]
        pid: Option<u32>,
        /// Max seconds to wait before giving up. Default 30s.
        #[arg(long, default_value_t = 30)]
        timeout_secs: u64,
    },
    /// M208: read bounded, structured output from the active pane.
    Output {
        /// Max bytes to read from the pane. Default 4096.
        #[arg(long, default_value_t = 4096)]
        max_bytes: usize,
        /// Max milliseconds to wait for the herdr subprocess to
        /// produce output. Default 5000ms.
        #[arg(long, default_value_t = 5_000)]
        timeout_ms: u64,
        /// Override the role to read from.
        #[arg(long)]
        role: Option<String>,
    },
    /// M208: read the latest terminal outcome (run_outcome + per
    /// milestone outcome log).
    Result {
        /// Always read the on-disk file; do not consult any cached
        /// state from this process. Default false.
        #[arg(long)]
        force: bool,
    },
    /// Per-session folder operations (`<plan_dir>/autopilot/<id>/session.json`).
    Session {
        #[command(subcommand)]
        cmd: AutopilotSessionCmd,
    },
    /// Typed runner notes. Cycle is required or derived from the
    /// session's active queue item — see `notes::derive_cycle`.
    Note {
        #[command(subcommand)]
        cmd: AutopilotNoteCmd,
    },
    /// Read or write autopilot config under the `autopilot.*`
    /// dotted-key namespace (mirrors `mp config get/set` but
    /// scoped to the autopilot section). See
    /// `crates/mp/src/commands/autopilot.rs::cmd_autopilot_config_*`.
    Config {
        #[command(subcommand)]
        cmd: AutopilotConfigCmd,
    },
}

/// M208: `mp autopilot start [IDS]...` — argument shape mirrors the
/// legacy `mp watch <ids...>` so the deprecation alias can dispatch
/// through the same code path with identical exit codes and stdout.
#[derive(Args, Debug)]
pub struct AutopilotStartArgs {
    /// One or more milestone IDs to process (e.g. `135` or `M135`).
    /// Processed sequentially in the order given.
    #[arg(value_name = "IDS")]
    pub ids: Vec<String>,
    /// Print the execution plan (milestone states, next actions,
    /// herdr commands) without modifying `plan.json` or spawning
    /// any agents.
    #[arg(long)]
    pub dry_run: bool,
    /// Override the structured-log path (default:
    /// `<plan_dir>/.mp/watch.log`).
    #[arg(long)]
    pub log_file: Option<std::path::PathBuf>,
    /// Max milliseconds the lifecycle poll waits before flagging
    /// the agent as hung. Default: 1_800_000 (30 min).
    #[arg(long)]
    pub stall_timeout_ms: Option<u64>,
    /// Lifecycle poll interval in milliseconds. Default: 1000.
    #[arg(long)]
    pub poll_interval_ms: Option<u64>,
    /// Re-attach to any herdr role panes that already exist for the
    /// active milestones. Crash / SIGINT recovery path.
    #[arg(long, conflicts_with = "force")]
    pub resume: bool,
    /// Bypass the double-spawn guard.
    #[arg(long, conflicts_with = "resume")]
    pub force: bool,
    /// Detach-safe mode: client exits once state is persisted; the
    /// driver runs detached and is re-discoverable via
    /// `mp autopilot status`.
    #[arg(long)]
    pub detach: bool,
}

/// `mp autopilot session …` subcommands.
#[derive(Subcommand, Debug)]
pub enum AutopilotSessionCmd {
    /// List every autopilot session under the plan (id, status,
    /// last_updated).
    List,
    /// Render the full session.json for `<id>` in canonical view
    /// (the typed struct as JSON).
    Show {
        /// Session id (e.g. `alpha`).
        id: String,
    },
    /// M225 F-01 production wiring (AC-03): run the startup
    /// recovery on `<id>`. Loads the session, runs
    /// `recover_event_tail` against the current binary
    /// provenance, and writes the session back if the cursor
    /// was bumped. A `Rejected` verdict (incompatible schema
    /// or binary) is surfaced as a structured report and the
    /// session is NOT mutated. This is the explicit CLI
    /// entry point for "resume from the last valid event
    /// sequence" — used by recovery scripts and by the F-02
    /// integration tests.
    Recover {
        /// Session id (e.g. `alpha`).
        id: String,
    },
    /// Apply a typed role-state transition to a session.
    ///
    /// Example:
    ///   mp autopilot session transition --session alpha \
    ///       --role runner --state working --working-on 207:1
    Transition(TransitionArgs),
}

/// `mp autopilot note …` subcommands.
#[derive(Subcommand, Debug)]
pub enum AutopilotNoteCmd {
    /// Append a typed runner note. Cycle is required or derivable.
    Add(NoteArgs),
}

/// Args for `mp autopilot note add`.
#[derive(Args, Debug)]
pub struct NoteArgs {
    /// Target session id.
    #[arg(long)]
    pub session: String,
    /// Note kind: `info | warn | blocker | decision | reminder | system`.
    #[arg(long, value_parser = ["info", "warn", "blocker", "decision", "reminder", "system"])]
    pub kind: String,
    /// Free-form note body. Empty body is rejected.
    #[arg(long)]
    pub body: String,
    /// Optional explicit cycle. If absent, derived from
    /// `session.working_on` or the unique in-progress queue item.
    #[arg(long)]
    pub cycle: Option<u32>,
    /// Optional explicit milestone id (defaults to the in-flight
    /// milestone at insertion time).
    #[arg(long)]
    pub milestone: Option<String>,
}

/// Args for `mp autopilot session transition`.
#[derive(Args, Debug)]
pub struct TransitionArgs {
    #[arg(long)]
    pub session: String,
    #[arg(long, value_parser = ["orchestrator", "runner", "reviewer"])]
    pub role: String,
    #[arg(long, value_parser = [
        "idle",
        "starting",
        "working",
        "blocked",
        "done",
        "unknown",
    ])]
    pub state: String,
    /// `milestone:cycle` (e.g. `207:1`). Required when transitioning
    /// to `working`; cleared on `idle` / `done`.
    #[arg(long)]
    pub working_on: Option<String>,
    /// Free-form actor token (e.g. `runner:M207`, pane id, etc.).
    /// Defaults to `mp-cli`.
    #[arg(long, default_value = "mp-cli")]
    pub actor: String,
}

/// `mp autopilot config …` subcommands. Shortcut for `mp config
/// get/set autopilot.<key>` so the dedicated surface can grow
/// (e.g. schema-aware help, deep unset) without touching the
/// umbrella `mp config` command.
#[derive(Subcommand, Debug)]
pub enum AutopilotConfigCmd {
    /// Read a single `autopilot.*` value. Same dotted-key path as
    /// `mp config get autopilot.<key>`.
    Get {
        /// Dotted key (e.g. `autopilot.topology`,
        /// `autopilot.roles.runner.harness`).
        key: String,
    },
    /// Write a single `autopilot.*` value. Validates against the
    /// role / field allow-list and the topology choice enum so a
    /// typo never lands on disk.
    Set {
        /// Dotted key (e.g. `autopilot.topology`,
        /// `autopilot.roles.runner.harness`).
        key: String,
        /// New value. Pass an empty string to clear a string field.
        value: String,
        /// Stage the change and report what *would* land without
        /// persisting.
        #[arg(long)]
        dry_run: bool,
    },
}
