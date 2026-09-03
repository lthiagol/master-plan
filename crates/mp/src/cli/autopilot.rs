//! M207 / M209: `mp autopilot` CLI surface.
//!
//! Subcommand tree (clap derives the `Commands::Autopilot` variant
//! from this enum):
//!
//! ```text
//! mp autopilot session list
//! mp autopilot session show <id>
//! mp autopilot session transition --session <id> --role <role> --state <state> [--working-on <m:n>]
//! mp autopilot note add --session <id> --kind <kind> --body <body> [--cycle <n>] [--milestone <id>]
//! mp autopilot config get <key>
//! mp autopilot config set <key> <value> [--dry-run]
//! ```

use clap::{Args, Subcommand};

#[derive(Subcommand, Debug)]
pub enum AutopilotCmd {
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
