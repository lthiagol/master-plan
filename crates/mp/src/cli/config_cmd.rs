use std::path::PathBuf;

use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    Show,
    Get {
        key: String,
    },
    Set {
        key: String,
        value: String,
        /// Parse and validate the change without writing config.
        #[arg(long)]
        dry_run: bool,
    },
    /// Validate project config (current file, or a candidate via `--file`).
    /// Emits JSON `{ ok, errors[{field,message}], warnings[{field,message}] }`.
    Validate {
        /// Candidate config file to validate instead of the project's current config.
        #[arg(long)]
        file: Option<PathBuf>,
    },
}
