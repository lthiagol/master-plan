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
    /// M201: emit the typed config schema as JSON. The shape is stable:
    /// `{ "$schema_version": "1.0", "keys": [ {key, type, default, allowed?, description}, ... ] }`.
    /// `type` is one of `bool | choice | string | integer | path | keybind`. `allowed` is
    /// present only for `choice`. `default` reflects the live defaults from
    /// `ProjectConfig::default()` and `KEYBIND_DEFAULTS`; keybinds surface their canonical
    /// chord string (e.g. `Ctrl-R`).
    Schema,
}
