//! M164: top-level flags only. No subcommands — bare `raul` launches the TUI.

use clap::Parser;

/// Color-only CLI surface. Unknown subcommands are rejected with the M164
/// migration sentinel (see `main.rs`).
#[derive(Parser, Debug)]
#[command(
    name = "raul",
    version,
    about = "Human-facing TUI for the Master Plan toolkit",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Color output: on/off (default: from ui.color config)
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
    pub color: Option<bool>,

    /// Project root (forwarded to mp)
    #[arg(long)]
    pub project_root: Option<std::path::PathBuf>,

    /// Plan directory (forwarded to mp)
    #[arg(long)]
    pub plan_dir: Option<std::path::PathBuf>,
}

/// Sentinel printed when a legacy subcommand is passed.
pub const M164_SENTINEL: &str = "subcommands removed in M164; launch the TUI";
