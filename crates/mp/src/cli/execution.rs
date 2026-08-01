use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ExecutionCmd {
    Check,
    Handoff {
        #[arg(long)]
        allow_tracks_only: bool,
        #[arg(long)]
        by: Option<String>,
    },
    HandoffShow,
    Pause {
        #[arg(long)]
        reason: Option<String>,
    },
    Status,
    Report {
        milestone: String,
    },
}
