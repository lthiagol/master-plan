use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ScratchCmd {
    /// Print (and create if absent) the in-repo scratch directory path
    Path,
    /// Create a unique subdirectory under the scratch dir
    New {
        /// Label for the subdirectory
        label: String,
    },
}
