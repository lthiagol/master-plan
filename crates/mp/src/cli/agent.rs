use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum AgentCmd {
    Role {
        /// Role name: coordinator | runner
        role: Option<String>,
        /// Clear the role token
        #[arg(long)]
        clear: bool,
    },
    /// M151: query the harness command registry (a single source of
    /// truth for the harness binaries `mp watch` invokes via
    /// `herdr agent start`). Subcommands:
    /// - `list` — enumerate every v1 entry.
    /// - `start-command <name>` — print the argv the registry would
    ///   build for a given harness (with optional --model and
    ///   --thinking-level overrides). Useful for previewing what
    ///   `mp watch` would invoke without running it.
    Harness {
        #[command(subcommand)]
        cmd: HarnessCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum HarnessCmd {
    /// List every harness the v1 registry knows about (opencode,
    /// pi, cursor). Each entry reports its id, display name,
    /// command, and which model / thinking flags it accepts.
    List,
    /// Print the argv `mp watch` would append after the
    /// `herdr agent start <pane> --cwd <root> --` separator for the
    /// named harness. Without `--model` / `--thinking-level` the
    /// output is just the base command (e.g. `["opencode"]`).
    StartCommand {
        /// Harness id to resolve (e.g. `opencode`, `pi`, `cursor`).
        name: String,
        /// Override the model argv appended (e.g. `claude-opus-4`).
        /// When omitted, no `--model` flag is appended even if the
        /// harness supports it — the registry's flag translator is
        /// caller-driven, not caller-inventing.
        #[arg(long)]
        model: Option<String>,
        /// Override the thinking-level argv appended (e.g. `high`).
        /// Only emitted when both the harness and the caller supply
        /// a value (cursor is the only v1 harness that exposes
        /// `--thinking`).
        #[arg(long = "thinking-level")]
        thinking_level: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SkillCmd {
    Context,
}
